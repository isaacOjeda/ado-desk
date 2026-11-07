//! Turns Azure DevOps rich text (work item HTML, PR Markdown) into HTML that is
//! safe to inject into the webview. All sanitization happens here, in Rust.
//!
//! Attachment images require the PAT, which the webview never sees. Their `src`
//! is rewritten to the `adoimg` protocol, served by [`crate::image_proxy`].

use std::borrow::Cow;
use std::collections::HashSet;

use url::Url;

pub const PROXY_SCHEME: &str = "adoimg";

/// Base URL under which the webview can reach the custom protocol.
/// Windows (WebView2) exposes custom schemes as `http://<scheme>.localhost`.
pub fn proxy_base() -> &'static str {
    if cfg!(windows) {
        "http://adoimg.localhost"
    } else {
        "adoimg://localhost"
    }
}

/// Maps an attachment URL of `organization` to its proxied form, or `None`
/// when the URL is not an attachment of the signed-in organization.
pub fn proxied_attachment_url(src: &str, organization: &str) -> Option<String> {
    let url = Url::parse(src).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let mut segments: Vec<&str> = url.path_segments()?.collect();

    if host == "dev.azure.com" {
        let org = segments.first()?;
        if !org.eq_ignore_ascii_case(organization) {
            return None;
        }
        segments.remove(0);
    } else if host.strip_suffix(".visualstudio.com")? != organization.to_ascii_lowercase() {
        return None;
    }

    if !is_attachment_path(&segments) {
        return None;
    }

    let mut proxied = format!("{}/{}/{}", proxy_base(), organization, segments.join("/"));
    if let Some(query) = url.query() {
        proxied.push('?');
        proxied.push_str(query);
    }
    Some(proxied)
}

/// Accepts `.../_apis/.../attachments/...` paths only (work item and PR attachments).
pub fn is_attachment_path(segments: &[&str]) -> bool {
    let apis = segments.iter().position(|s| *s == "_apis");
    let attachments = segments.iter().rposition(|s| s.eq_ignore_ascii_case("attachments"));
    matches!((apis, attachments), (Some(a), Some(b)) if a < b && b + 1 < segments.len())
        && segments.iter().all(|s| *s != ".." && *s != ".")
}

fn is_proxy_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("adoimg:") || lower.starts_with("http://adoimg.localhost")
}

/// Sanitizes untrusted HTML and rewrites attachment images of `organization`.
pub fn sanitize_html(html: &str, organization: &str) -> String {
    let organization = organization.to_string();
    let mut schemes: HashSet<&str> = ["http", "https", "mailto"].into_iter().collect();
    schemes.insert(PROXY_SCHEME);

    ammonia::Builder::default()
        .url_schemes(schemes)
        .link_rel(Some("noopener noreferrer"))
        .attribute_filter(move |element, attribute, value| {
            if element == "img" && attribute == "src" {
                // Proxy URLs are only produced by us; never trust one coming from content.
                if is_proxy_url(value) {
                    return None;
                }
                if let Some(proxied) = proxied_attachment_url(value, &organization) {
                    return Some(Cow::Owned(proxied));
                }
            }
            if attribute == "href" && is_proxy_url(value) {
                return None;
            }
            Some(Cow::Borrowed(value))
        })
        .clean(html)
        .to_string()
}

/// Renders CommonMark (with tables, strikethrough and task lists) and sanitizes the result.
pub fn markdown_to_html(markdown: &str, organization: &str) -> String {
    use pulldown_cmark::{Options, Parser, html};

    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut rendered = String::new();
    html::push_html(&mut rendered, Parser::new_ext(markdown, options));
    sanitize_html(&rendered, organization)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATTACHMENT: &str =
        "https://dev.azure.com/contoso/b1c2/_apis/wit/attachments/1234-abcd?fileName=shot.png";

    #[test]
    fn strips_scripts_handlers_and_javascript_links() {
        let html = r#"<p onclick="steal()">Hi<script>alert(1)</script></p><a href="javascript:alert(1)">x</a><img src=x onerror="alert(1)">"#;

        let clean = sanitize_html(html, "contoso");

        assert!(!clean.contains("script"));
        assert!(!clean.contains("onclick"));
        assert!(!clean.contains("onerror"));
        assert!(!clean.contains("javascript:"));
        assert!(clean.contains("<p>Hi</p>"));
    }

    #[test]
    fn rewrites_same_org_attachment_images_to_the_proxy() {
        let clean = sanitize_html(&format!(r#"<img src="{ATTACHMENT}">"#), "contoso");

        let expected = format!(
            "{}/contoso/b1c2/_apis/wit/attachments/1234-abcd?fileName=shot.png",
            proxy_base()
        );
        assert!(clean.contains(&expected.replace('&', "&amp;")), "{clean}");
    }

    #[test]
    fn rewrites_legacy_visualstudio_host() {
        let src = "https://contoso.visualstudio.com/b1c2/_apis/wit/attachments/9?fileName=a.png";
        assert_eq!(
            proxied_attachment_url(src, "contoso").unwrap(),
            format!("{}/contoso/b1c2/_apis/wit/attachments/9?fileName=a.png", proxy_base())
        );
    }

    #[test]
    fn does_not_proxy_other_orgs_or_non_attachment_urls() {
        let other_org = ATTACHMENT.replace("contoso", "fabrikam");
        assert_eq!(proxied_attachment_url(&other_org, "contoso"), None);
        assert_eq!(
            proxied_attachment_url("https://dev.azure.com/contoso/_apis/projects", "contoso"),
            None
        );
        assert_eq!(
            proxied_attachment_url("http://dev.azure.com/contoso/p/_apis/wit/attachments/1", "contoso"),
            None
        );
        assert_eq!(
            proxied_attachment_url("https://evil.com/contoso/p/_apis/wit/attachments/1", "contoso"),
            None
        );
    }

    #[test]
    fn drops_proxy_urls_injected_by_content() {
        let html = r#"<img src="adoimg://localhost/contoso/_apis/projects"><a href="adoimg://localhost/x">l</a>"#;

        let clean = sanitize_html(html, "contoso");

        assert!(!clean.contains("adoimg"), "{clean}");
    }

    #[test]
    fn renders_markdown_tables_and_sanitizes_embedded_html() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |\n\n<script>alert(1)</script>\n\n- [x] done";

        let html = markdown_to_html(md, "contoso");

        assert!(html.contains("<table>"));
        assert!(html.contains("<td>1</td>"));
        assert!(!html.contains("<script"));
        assert!(html.contains("done"));
    }
}
