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

/// Largest attachment served through the proxy.
pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

/// Content type to serve, or an error status. Only images are served (never HTML or
/// scripts from the proxy origin), within a size cap. Azure DevOps often labels
/// attachments `application/octet-stream`, so the type falls back to the file extension.
pub fn serveable_image(content_type: &str, file_name: Option<&str>, len: usize) -> Result<String, StatusCode> {
    if len > MAX_IMAGE_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let declared = content_type.trim().to_ascii_lowercase();
    if declared.starts_with("image/") {
        return Ok(content_type.trim().to_string());
    }
    let extension = file_name
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase());
    let inferred = match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE),
    };
    Ok(inferred.to_string())
}

/// File name of an attachment: the `fileName` query parameter (work items) or the last path segment (PRs).
fn attachment_file_name(url: &Url) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k.eq_ignore_ascii_case("fileName"))
        .map(|(_, v)| v.into_owned())
        .or_else(|| url.path_segments()?.next_back().map(str::to_string))
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

    let file_name = attachment_file_name(&url);
    match client.get_bytes(url).await {
        Ok((bytes, content_type)) => match serveable_image(&content_type, file_name.as_deref(), bytes.len()) {
            Ok(content_type) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CACHE_CONTROL, "private, max-age=3600")
                .header("X-Content-Type-Options", "nosniff")
                .header("Content-Security-Policy", "sandbox")
                .body(bytes)
                .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)),
            Err(code) => status(code),
        },
        Err(_) => status(StatusCode::BAD_GATEWAY),
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_IMAGE_BYTES, serveable_image, upstream_url};
    use tauri::http::StatusCode;

    #[test]
    fn serves_only_images_within_the_size_cap() {
        assert_eq!(serveable_image("image/png", None, 10).unwrap(), "image/png");
        assert_eq!(serveable_image("Image/JPEG", None, 10).unwrap(), "Image/JPEG");
        assert_eq!(serveable_image("text/html", None, 10), Err(StatusCode::UNSUPPORTED_MEDIA_TYPE));
        assert_eq!(serveable_image("text/html", Some("page.html"), 10), Err(StatusCode::UNSUPPORTED_MEDIA_TYPE));
        assert_eq!(serveable_image("image/png", None, MAX_IMAGE_BYTES + 1), Err(StatusCode::PAYLOAD_TOO_LARGE));
    }

    #[test]
    fn infers_image_type_from_the_file_name_for_octet_streams() {
        let octet = "application/octet-stream";
        assert_eq!(serveable_image(octet, Some("Screen Shot.PNG"), 10).unwrap(), "image/png");
        assert_eq!(serveable_image(octet, Some("photo.jpeg"), 10).unwrap(), "image/jpeg");
        assert_eq!(serveable_image(octet, Some("evil.svg"), 10), Err(StatusCode::UNSUPPORTED_MEDIA_TYPE));
        assert_eq!(serveable_image(octet, None, 10), Err(StatusCode::UNSUPPORTED_MEDIA_TYPE));
    }

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
