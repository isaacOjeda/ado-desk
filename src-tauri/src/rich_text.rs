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

/// Accepts only the two attachment shapes Azure DevOps produces (relative to the org):
/// `[project/]_apis/wit/attachments/{id}` and
/// `[project/]_apis/git/repositories/{repo}/pullRequests/{id}/attachments/{name}`.
/// Dot segments and encoded separators are rejected so nothing can be re-targeted upstream.
pub fn is_attachment_path(segments: &[&str]) -> bool {
    let safe = segments.iter().all(|s| {
        let lower = s.to_ascii_lowercase();
        !s.is_empty() && *s != "." && *s != ".." && !lower.contains("%2f") && !lower.contains("%5c") && !lower.contains("%2e")
    });
    let Some(apis) = segments.iter().position(|s| *s == "_apis") else {
        return false;
    };
    if !safe || apis > 1 {
        return false;
    }
    let lower: Vec<String> = segments[apis + 1..].iter().map(|s| s.to_ascii_lowercase()).collect();
    let tail: Vec<&str> = lower.iter().map(String::as_str).collect();
    matches!(
        tail.as_slice(),
        ["wit", "attachments", _] | ["git", "repositories", _, "pullrequests", _, "attachments", _]
    )
}

fn is_proxy_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("adoimg:") || lower.starts_with("http://adoimg.localhost")
}

/// Sanitizes untrusted HTML and rewrites attachment images of `organization`.
pub fn sanitize_html(html: &str, organization: &str) -> String {
    sanitize(html, organization, ammonia::UrlRelative::PassThrough)
}

fn sanitize(html: &str, organization: &str, relative: ammonia::UrlRelative) -> String {
    let organization = organization.to_string();
    // The proxy scheme is deliberately NOT allowed: ammonia validates schemes before the
    // attribute filter, so only our own rewritten `src` values can carry it.
    let schemes: HashSet<&str> = ["http", "https", "mailto"].into_iter().collect();

    ammonia::Builder::default()
        .url_schemes(schemes)
        .link_rel(Some("noopener noreferrer"))
        .url_relative(relative)
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

/// Renders a repository Markdown file for a preview. Relative links and images point into the
/// repository, which the webview cannot reach, so they are dropped instead of left broken.
pub fn markdown_preview_html(markdown: &str, organization: &str) -> String {
    sanitize(&render_markdown(markdown), organization, ammonia::UrlRelative::Deny)
}

/// Renders CommonMark (with tables, strikethrough and task lists) and sanitizes the result.
pub fn markdown_to_html(markdown: &str, organization: &str) -> String {
    sanitize_html(&render_markdown(markdown), organization)
}

fn render_markdown(markdown: &str) -> String {
    use pulldown_cmark::{Options, Parser, html};

    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut rendered = String::new();
    html::push_html(&mut rendered, Parser::new_ext(markdown, options));
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_preview_drops_relative_urls_and_scripts() {
        let html = markdown_preview_html(
            "# Title\n\n![logo](./img/logo.png) [docs](docs/a.md) [web](https://example.com)\n\n<script>alert(1)</script>",
            ORG_FOR_PREVIEW,
        );
        assert!(html.contains("<h1>Title</h1>"), "{html}");
        assert!(html.contains(r#"href="https://example.com""#), "{html}");
        assert!(!html.contains("logo.png"), "{html}");
        assert!(!html.contains("docs/a.md"), "{html}");
        assert!(!html.contains("<script"), "{html}");
    }

    #[test]
    fn markdown_preview_keeps_attachment_proxying() {
        let html = markdown_preview_html(
            &format!("![a](https://dev.azure.com/{ORG_FOR_PREVIEW}/_apis/wit/attachments/0f1e2d3c-4b5a-6978-8091-a2b3c4d5e6f7)"),
            ORG_FOR_PREVIEW,
        );
        assert!(html.contains("adoimg"), "{html}");
    }

    const ORG_FOR_PREVIEW: &str = "contoso";

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
    fn drops_proxy_urls_obfuscated_with_whitespace() {
        let html = "<img src=\"ado&#9;img://localhost/contoso/p/_apis/wit/attachments/1\"><img src=\"ado\nimg://localhost/x\">";

        let clean = sanitize_html(html, "contoso");

        assert!(!clean.contains("img://"), "{clean}");
    }

    #[test]
    fn attachment_paths_accept_only_work_item_and_pull_request_shapes() {
        let ok = |p: &str| is_attachment_path(&p.split('/').collect::<Vec<_>>());
        assert!(ok("proj/_apis/wit/attachments/1234"));
        assert!(ok("_apis/wit/attachments/1234"));
        assert!(ok("proj/_apis/git/repositories/r1/pullRequests/7/attachments/shot.png"));
        assert!(!ok("_apis/projects/attachments/x"));
        assert!(!ok("a/b/_apis/wit/attachments/1"));
        assert!(!ok("proj/_apis/wit/attachments/x%2F..%2F..%2F_apis%2Fprojects"));
        assert!(!ok("proj/_apis/wit/attachments/x%5Cy"));
        assert!(!ok("proj/_apis/wit/attachments"));
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
