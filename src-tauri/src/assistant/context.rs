//! What the main window shows, as the assistant's `current_view` tool reports it. The webview
//! sends it on every navigation; it is validated here and tagged with the active organization
//! by Rust, never by the webview.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

pub const MAX_TITLE_CHARS: usize = 512;
const LISTS: &[&str] = &["inbox", "workItems", "pullRequests"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ViewContext {
    #[serde(rename_all = "camelCase")]
    WorkItem { id: i64, project: String, title: String },
    #[serde(rename_all = "camelCase")]
    PullRequest { id: i64, project: String, repository_id: String, repository: String, title: String },
    /// A list (`inbox`, `workItems`, `pullRequests`) with no item open.
    #[serde(rename_all = "camelCase")]
    List { list: String },
}

fn clip(text: &str, max: usize) -> String {
    text.trim().chars().filter(|c| !c.is_control()).take(max).collect()
}

/// Checks ids, project and repository id the way the read commands do; titles are clipped.
pub fn validate(view: ViewContext) -> AppResult<ViewContext> {
    let unknown = || AppError::Assistant("Unknown view");
    Ok(match view {
        ViewContext::WorkItem { id, project, title } => {
            if id <= 0 {
                return Err(unknown());
            }
            let project = crate::pull_request_actions::valid_project(&project).map_err(|_| unknown())?;
            ViewContext::WorkItem { id, project, title: clip(&title, MAX_TITLE_CHARS) }
        }
        ViewContext::PullRequest { id, project, repository_id, repository, title } => {
            if id <= 0 {
                return Err(unknown());
            }
            ViewContext::PullRequest {
                id,
                project: crate::pull_request_actions::valid_project(&project).map_err(|_| unknown())?,
                repository_id: crate::pull_request_actions::valid_repository_id(&repository_id).map_err(|_| unknown())?,
                repository: clip(&repository, 256),
                title: clip(&title, MAX_TITLE_CHARS),
            }
        }
        ViewContext::List { list } => {
            if !LISTS.contains(&list.as_str()) {
                return Err(unknown());
            }
            ViewContext::List { list }
        }
    })
}

/// The `current_view` answer. A view recorded under another organization than the active one
/// is stale (the user switched), so it reads as nothing open.
pub fn describe(view: Option<&(String, ViewContext)>, active_organization: Option<&str>) -> Value {
    let Some(organization) = active_organization else {
        return json!({ "signedIn": false, "note": "ADO Desk is not signed in to any organization." });
    };
    match view {
        Some((org, view)) if org == organization => json!({ "organization": organization, "view": view }),
        _ => json!({ "organization": organization, "view": null, "note": "No work item or pull request is open." }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "0F2A8C3E-1B2C-4D5E-8F90-123456789ABC";

    #[test]
    fn views_are_validated_and_normalized() {
        let pr = validate(ViewContext::PullRequest {
            id: 7,
            project: "Web".into(),
            repository_id: REPO.into(),
            repository: "web-app".into(),
            title: " Add\u{0007} retry ".into(),
        })
        .unwrap();
        assert_eq!(
            pr,
            ViewContext::PullRequest {
                id: 7,
                project: "Web".into(),
                repository_id: REPO.to_ascii_lowercase(),
                repository: "web-app".into(),
                title: "Add retry".into(),
            }
        );
        let long = validate(ViewContext::WorkItem { id: 1, project: "Web".into(), title: "x".repeat(2000) }).unwrap();
        assert!(matches!(long, ViewContext::WorkItem { title, .. } if title.chars().count() == MAX_TITLE_CHARS));
        assert!(validate(ViewContext::List { list: "pullRequests".into() }).is_ok());
    }

    #[test]
    fn bad_views_are_rejected_without_echoing_them() {
        let bad = [
            ViewContext::WorkItem { id: 0, project: "Web".into(), title: String::new() },
            ViewContext::WorkItem { id: 1, project: "".into(), title: String::new() },
            ViewContext::PullRequest { id: 1, project: "Web".into(), repository_id: "../x".into(), repository: String::new(), title: String::new() },
            ViewContext::List { list: "settings<script>".into() },
        ];
        for view in bad {
            let err = validate(view).unwrap_err();
            assert_eq!(err.to_string(), "Unknown view");
        }
    }

    #[test]
    fn the_webview_payload_shape_is_camel_case() {
        let view: ViewContext = serde_json::from_value(json!({
            "kind": "pullRequest", "id": 7, "project": "Web", "repositoryId": REPO, "repository": "web-app", "title": "t"
        }))
        .unwrap();
        assert!(matches!(view, ViewContext::PullRequest { id: 7, .. }));
        assert_eq!(serde_json::to_value(ViewContext::List { list: "inbox".into() }).unwrap(), json!({ "kind": "list", "list": "inbox" }));
    }

    #[test]
    fn describe_hides_views_of_another_organization() {
        let view = ("contoso".to_string(), ViewContext::WorkItem { id: 5, project: "Web".into(), title: "t".into() });
        let shown = describe(Some(&view), Some("contoso"));
        assert_eq!(shown["view"]["id"], 5);
        assert_eq!(shown["organization"], "contoso");
        assert_eq!(describe(Some(&view), Some("fabrikam"))["view"], Value::Null);
        assert_eq!(describe(None, Some("contoso"))["view"], Value::Null);
        assert_eq!(describe(Some(&view), None)["signedIn"], false);
    }
}
