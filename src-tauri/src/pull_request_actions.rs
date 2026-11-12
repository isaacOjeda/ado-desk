//! Pull request write actions: vote and comment on diff lines.
//!
//! Every write goes through `AzdoClient::write`, so a token without Code (Read & Write) never
//! signs the user out and server text is never echoed. Values are validated before any URL or
//! body is built; error messages are fixed text and never contain what was sent.

use reqwest::Method;
use serde_json::{Value, json};
use tauri::State;

use crate::AppState;
use crate::azdo::AzdoClient;
use crate::commands::current_client;
use crate::error::{AppError, AppResult};
use crate::pull_requests::pr_url;
use crate::queries;

/// The votes Azure DevOps accepts: approve, approve with suggestions, no vote, wait for author, reject.
pub const VOTES: [i64; 5] = [10, 5, 0, -5, -10];

// ---------------------------------------------------------------------------
// Pure validation and payloads
// ---------------------------------------------------------------------------

pub fn vote_body(vote: i64) -> AppResult<Value> {
    if VOTES.contains(&vote) {
        Ok(json!({ "vote": vote }))
    } else {
        Err(AppError::InvalidChange("Unknown vote"))
    }
}

/// Repository ids are GUIDs; anything else never reaches a URL.
pub fn valid_repository_id(repository_id: &str) -> AppResult<String> {
    uuid_like(repository_id)
        .then(|| repository_id.to_ascii_lowercase())
        .ok_or(AppError::InvalidChange("Unknown repository"))
}

fn uuid_like(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(group, len)| {
            group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit())
        })
}

fn valid_pull_request_id(id: i64) -> AppResult<i64> {
    if id > 0 { Ok(id) } else { Err(AppError::InvalidChange("Unknown pull request")) }
}

fn valid_project(project: &str) -> AppResult<String> {
    queries::validate_project(project).map_err(|_| AppError::InvalidChange("Unknown project"))
}

/// A refused write names the scope pull request actions need, not the work item one.
pub fn pull_request_write_error(err: AppError) -> AppError {
    match err {
        AppError::WriteDenied => AppError::PullRequestWriteDenied,
        other => other,
    }
}

/// The signed-in user's vote among `reviewers` (0 when they are not a reviewer).
pub fn my_vote(reviewers: &[crate::pull_requests::Reviewer], user_id: &str) -> i64 {
    reviewers
        .iter()
        .find(|r| r.id.eq_ignore_ascii_case(user_id))
        .map_or(0, |r| r.vote)
}

async fn send_write(client: &AzdoClient, method: Method, url: url::Url, body: &Value) -> AppResult<()> {
    client
        .write(method, url, Some((body, "application/json")))
        .await
        .map_err(pull_request_write_error)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Sets the signed-in user's vote (`0` resets it). Adds them as an optional reviewer when
/// they are not one yet, like the web does.
#[tauri::command]
pub async fn vote_pull_request(
    state: State<'_, AppState>,
    project: String,
    repository_id: String,
    id: i64,
    vote: i64,
) -> AppResult<()> {
    let body = vote_body(vote)?;
    let project = valid_project(&project)?;
    let repository_id = valid_repository_id(&repository_id)?;
    let id = valid_pull_request_id(id)?;
    let client = current_client(&state)?;
    let user_id = client.session().user_id.clone();
    let url = pr_url(&client, &project, &repository_id, id, &["reviewers", &user_id]);
    send_write(&client, Method::PUT, url, &body).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pull_requests::Reviewer;

    const REPO: &str = "3411ebc1-d5aa-464f-9615-0b527bc66719";

    fn reviewer(id: &str, vote: i64) -> Reviewer {
        Reviewer {
            id: id.into(),
            name: "R".into(),
            vote,
            is_required: false,
            has_declined: false,
            is_group: false,
        }
    }

    #[test]
    fn vote_body_accepts_only_azure_devops_votes() {
        for vote in VOTES {
            assert_eq!(vote_body(vote).unwrap(), json!({ "vote": vote }));
        }
        for vote in [1, 3, 11, -1, -11, i64::MAX] {
            assert!(matches!(vote_body(vote), Err(AppError::InvalidChange(_))), "{vote}");
        }
    }

    #[test]
    fn repository_and_pull_request_ids_are_validated() {
        assert_eq!(valid_repository_id(REPO).unwrap(), REPO);
        assert_eq!(valid_repository_id(&REPO.to_uppercase()).unwrap(), REPO);
        for bad in ["", "..", "../x", "repo", "3411ebc1-d5aa-464f-9615-0b527bc6671", "3411ebc1/d5aa-464f-9615-0b527bc66719"] {
            assert!(matches!(valid_repository_id(bad), Err(AppError::InvalidChange(_))), "{bad}");
        }
        assert_eq!(valid_pull_request_id(42).unwrap(), 42);
        assert!(valid_pull_request_id(0).is_err());
        assert!(valid_pull_request_id(-3).is_err());
    }

    #[test]
    fn denied_writes_name_the_code_write_scope_and_keep_their_kind() {
        let err = pull_request_write_error(AppError::WriteDenied);
        assert_eq!(err.kind(), "writeDenied");
        assert!(err.to_string().contains("Code (Read & Write)"), "{err}");
        assert!(!err.to_string().contains("work items"), "{err}");
        assert!(matches!(pull_request_write_error(AppError::WriteRejected), AppError::WriteRejected));
        assert!(matches!(pull_request_write_error(AppError::NotFound), AppError::NotFound));
    }

    #[test]
    fn my_vote_matches_the_signed_in_user_case_insensitively() {
        let reviewers = [reviewer("AAA", 5), reviewer("bbb", -10)];
        assert_eq!(my_vote(&reviewers, "aaa"), 5);
        assert_eq!(my_vote(&reviewers, "BBB"), -10);
        assert_eq!(my_vote(&reviewers, "ccc"), 0);
    }
}
