//! Minimal Open VSX client: theme search and extension download. It is separate from the
//! Azure DevOps client (no shared headers or credentials) and only starts requests against
//! `https://open-vsx.org/api`; downloads may redirect to Open VSX's storage, HTTPS only.

use std::time::Duration;

use reqwest::{redirect, StatusCode};
use serde::{Deserialize, Serialize};
use url::Url;

use super::vsix::clean_text;
use crate::error::{AppError, AppResult};

const API: &str = "https://open-vsx.org/api";
const API_HOST: &str = "open-vsx.org";
/// Largest `.vsix` downloaded; real theme packages are well under 1 MB.
pub const MAX_VSIX_BYTES: usize = 20 * 1024 * 1024;
/// Results shown per search.
const PAGE_SIZE: u32 = 24;
pub const MAX_QUERY_CHARS: usize = 100;
const MAX_ID_CHARS: usize = 128;
const MAX_DESCRIPTION_CHARS: usize = 240;

/// An Open VSX extension in the Themes category, as shown in the import list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeExtension {
    pub namespace: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub download_count: u64,
    pub verified: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawExtension {
    namespace: String,
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    download_count: Option<u64>,
    verified: Option<bool>,
    files: Option<RawFiles>,
}

#[derive(Debug, Deserialize)]
struct RawFiles {
    download: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    extensions: Vec<RawExtension>,
}

impl RawExtension {
    fn summary(&self) -> Option<ThemeExtension> {
        if !is_valid_id(&self.namespace) || !is_valid_id(&self.name) {
            return None;
        }
        let display_name = self.display_name.as_deref().map(|n| clean_text(n, 80)).filter(|n| !n.is_empty());
        Some(ThemeExtension {
            namespace: self.namespace.clone(),
            name: self.name.clone(),
            display_name: display_name.unwrap_or_else(|| self.name.clone()),
            description: clean_text(self.description.as_deref().unwrap_or(""), MAX_DESCRIPTION_CHARS),
            version: clean_text(self.version.as_deref().unwrap_or(""), 40),
            download_count: self.download_count.unwrap_or(0),
            verified: self.verified.unwrap_or(false),
        })
    }
}

/// Open VSX namespaces and names: ASCII letters, digits, `-`, `_` and `.`, starting with a letter or digit.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_CHARS
        && id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Trimmed search text; empty means "most downloaded themes".
pub fn normalize_query(query: &str) -> AppResult<String> {
    let query = query.trim();
    if query.chars().count() > MAX_QUERY_CHARS || query.chars().any(char::is_control) {
        return Err(AppError::InvalidThemeSearch);
    }
    Ok(query.to_string())
}

/// Search URL for themes; an empty query lists the most downloaded ones.
pub fn search_url(query: &str) -> Url {
    let mut url = Url::parse(&format!("{API}/-/search")).expect("static URL");
    {
        let mut params = url.query_pairs_mut();
        if !query.is_empty() {
            params.append_pair("query", query);
        }
        params
            .append_pair("category", "Themes")
            .append_pair("size", &PAGE_SIZE.to_string())
            .append_pair("sortBy", if query.is_empty() { "downloadCount" } else { "relevance" })
            .append_pair("sortOrder", "desc");
    }
    url
}

/// Metadata URL of one extension; segments are percent-encoded, never concatenated raw.
pub fn extension_url(namespace: &str, name: &str) -> AppResult<Url> {
    if !is_valid_id(namespace) || !is_valid_id(name) {
        return Err(AppError::InvalidExtension);
    }
    let mut url = Url::parse(API).expect("static URL");
    url.path_segments_mut().expect("https URL has segments").push(namespace).push(name);
    Ok(url)
}

/// Only HTTPS download links on the Open VSX API host are followed.
pub fn is_trusted_download(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str() == Some(API_HOST) && url.port().is_none() && url.username().is_empty()
}

pub struct OpenVsx {
    http: reqwest::Client,
}

impl OpenVsx {
    pub fn new() -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("ado-desk/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .https_only(true)
            .redirect(redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 || attempt.url().scheme() != "https" {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()?;
        Ok(Self { http })
    }

    async fn get(&self, url: Url) -> AppResult<reqwest::Response> {
        let response = self.http.get(url).send().await?;
        match response.status() {
            status if status.is_success() => Ok(response),
            StatusCode::NOT_FOUND => Err(AppError::NotFound),
            status => Err(AppError::OpenVsx { status: status.as_u16() }),
        }
    }

    pub async fn search(&self, query: &str) -> AppResult<Vec<ThemeExtension>> {
        let response: SearchResponse = self
            .get(search_url(query))
            .await?
            .json()
            .await
            .map_err(|_| AppError::ThemeImport("Unexpected response from Open VSX"))?;
        Ok(response.extensions.iter().filter_map(RawExtension::summary).collect())
    }

    /// Downloads the latest `.vsix` of `namespace/name`, returning its summary and bytes.
    pub async fn download(&self, namespace: &str, name: &str) -> AppResult<(ThemeExtension, Vec<u8>)> {
        let raw: RawExtension = self
            .get(extension_url(namespace, name)?)
            .await?
            .json()
            .await
            .map_err(|_| AppError::ThemeImport("Unexpected response from Open VSX"))?;
        let summary = raw.summary().ok_or(AppError::ThemeImport("Unexpected response from Open VSX"))?;
        let download = raw
            .files
            .and_then(|f| f.download)
            .and_then(|link| Url::parse(&link).ok())
            .filter(is_trusted_download)
            .ok_or(AppError::ThemeImport("Open VSX has no downloadable package for this extension"))?;

        let mut response = self.get(download).await?;
        if response.content_length().is_some_and(|len| len > MAX_VSIX_BYTES as u64) {
            return Err(AppError::ThemeImport("The extension package is too large to import"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > MAX_VSIX_BYTES {
                return Err(AppError::ThemeImport("The extension package is too large to import"));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((summary, bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_url_targets_the_themes_category_and_encodes_the_query() {
        let url = search_url("one dark & pro");
        assert_eq!(url.host_str(), Some("open-vsx.org"));
        assert_eq!(url.path(), "/api/-/search");
        let pairs: Vec<(String, String)> = url.query_pairs().map(|(k, v)| (k.into(), v.into())).collect();
        assert!(pairs.contains(&("query".into(), "one dark & pro".into())));
        assert!(pairs.contains(&("category".into(), "Themes".into())));
        assert!(pairs.contains(&("sortBy".into(), "relevance".into())));
    }

    #[test]
    fn empty_search_lists_the_most_downloaded_themes() {
        let url = search_url("");
        assert!(url.query_pairs().all(|(k, _)| k != "query"));
        assert!(url.query_pairs().any(|(k, v)| k == "sortBy" && v == "downloadCount"));
    }

    #[test]
    fn queries_are_trimmed_and_bounded() {
        assert_eq!(normalize_query("  nord  ").unwrap(), "nord");
        assert!(matches!(normalize_query(&"x".repeat(MAX_QUERY_CHARS + 1)), Err(AppError::InvalidThemeSearch)));
        assert!(matches!(normalize_query("a\nb"), Err(AppError::InvalidThemeSearch)));
    }

    #[test]
    fn extension_ids_are_validated_and_never_change_the_path() {
        assert_eq!(extension_url("enkia", "tokyo-night").unwrap().as_str(), "https://open-vsx.org/api/enkia/tokyo-night");
        for bad in ["", "../x", "a/b", "a?b", "-lead", "a b", "%2e%2e"] {
            assert!(matches!(extension_url(bad, "x"), Err(AppError::InvalidExtension)), "{bad}");
            assert!(matches!(extension_url("x", bad), Err(AppError::InvalidExtension)), "{bad}");
        }
    }

    #[test]
    fn only_https_links_on_the_api_host_are_downloaded() {
        let ok = Url::parse("https://open-vsx.org/api/a/b/1.0.0/file/a.b-1.0.0.vsix").unwrap();
        assert!(is_trusted_download(&ok));
        for bad in [
            "http://open-vsx.org/api/a.vsix",
            "https://evil.example/api/a.vsix",
            "https://open-vsx.org.evil.example/a.vsix",
            "https://open-vsx.org:8443/a.vsix",
            "https://user@open-vsx.org/a.vsix",
        ] {
            assert!(!is_trusted_download(&Url::parse(bad).unwrap()), "{bad}");
        }
    }

    #[test]
    fn summaries_clean_text_and_drop_invalid_ids() {
        let raw: RawExtension = serde_json::from_value(serde_json::json!({
            "namespace": "enkia", "name": "tokyo-night", "displayName": "Tokyo\u{0}Night",
            "description": "x".repeat(500), "downloadCount": 7
        }))
        .unwrap();
        let summary = raw.summary().unwrap();
        assert_eq!(summary.display_name, "TokyoNight");
        assert_eq!(summary.description.chars().count(), MAX_DESCRIPTION_CHARS);
        assert_eq!(summary.download_count, 7);

        let bad: RawExtension = serde_json::from_value(serde_json::json!({ "namespace": "../x", "name": "y" })).unwrap();
        assert!(bad.summary().is_none());
    }
}
