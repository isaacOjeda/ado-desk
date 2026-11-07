//! `adoimg://` protocol: serves Azure DevOps attachment images to the webview
//! using the PAT held in Rust. Only attachment paths of the signed-in
//! organization on dev.azure.com are fetched.

use tauri::http::{Method, Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, Runtime};
use url::Url;

use crate::AppState;
use crate::commands::current_client;
use crate::rich_text::is_attachment_path;

/// Maps a proxy request path (`/{org}/.../_apis/.../attachments/...`) to the upstream URL.
pub fn upstream_url(path: &str, query: Option<&str>, organization: &str) -> Option<Url> {
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let (org, rest) = segments.split_first()?;
    if !org.eq_ignore_ascii_case(organization) || !is_attachment_path(rest) {
        return None;
    }
    let mut url = Url::parse("https://dev.azure.com").ok()?;
    url.set_path(path);
    url.set_query(query);
    // Re-check after URL normalization (percent-decoding, dot segments).
    let normalized: Vec<&str> = url.path_segments()?.collect();
    let (first, tail) = normalized.split_first()?;
    (first.eq_ignore_ascii_case(organization) && is_attachment_path(tail)).then_some(url)
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .body(Vec::new())
        .expect("static response is valid")
}

pub async fn handle<R: Runtime>(app: &AppHandle<R>, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    if request.method() != Method::GET {
        return status(StatusCode::METHOD_NOT_ALLOWED);
    }
    let Ok(client) = current_client(&app.state::<AppState>()) else {
        return status(StatusCode::UNAUTHORIZED);
    };
    let uri = request.uri();
    let Some(url) = upstream_url(uri.path(), uri.query(), client.org()) else {
        return status(StatusCode::NOT_FOUND);
    };

    match client.get_bytes(url).await {
        Ok((bytes, content_type)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "private, max-age=3600")
            .header("X-Content-Type-Options", "nosniff")
            .body(bytes)
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(_) => status(StatusCode::BAD_GATEWAY),
    }
}

#[cfg(test)]
mod tests {
    use super::upstream_url;

    #[test]
    fn maps_attachment_paths_of_the_signed_in_org() {
        let url = upstream_url(
            "/contoso/b1c2/_apis/wit/attachments/1234",
            Some("fileName=shot.png"),
            "contoso",
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://dev.azure.com/contoso/b1c2/_apis/wit/attachments/1234?fileName=shot.png"
        );
    }

    #[test]
    fn rejects_other_orgs_non_attachments_and_traversal() {
        for path in [
            "/fabrikam/p/_apis/wit/attachments/1",
            "/contoso/_apis/projects",
            "/contoso/p/_apis/wit/attachments",
            "/contoso/p/_apis/wit/attachments/../../../_apis/projects",
            "/contoso/p/_apis/wit/attachments/%2e%2e/%2e%2e/x",
        ] {
            assert!(upstream_url(path, None, "contoso").is_none(), "accepted {path}");
        }
    }
}
