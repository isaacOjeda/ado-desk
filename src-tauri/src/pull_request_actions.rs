//! Pull request write actions: vote, comment on diff lines, reply to and resolve threads.
//!
//! Every write goes through `AzdoClient::write`, so a token without Code (Read & Write) never
//! signs the user out and server text is never echoed. Values are validated before any URL or
//! body is built; error messages are fixed text and never contain what was sent.

use reqwest::Method;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tauri::State;

use crate::AppState;
use crate::azdo::AzdoClient;
use crate::commands::current_client;
use crate::error::{AppError, AppResult};
use crate::pull_requests::pr_url;
use crate::queries;

/// The votes Azure DevOps accepts: approve, approve with suggestions, no vote, wait for author, reject.
pub const VOTES: [i64; 5] = [10, 5, 0, -5, -10];
pub const MAX_COMMENT_CHARS: usize = 4000;
const MAX_PATH_CHARS: usize = 1024;
const MAX_LINE: i64 = 1_000_000;
const MAX_OFFSET: i64 = 100_000;
/// Comment ids are 16-bit in the Azure DevOps API.
const MAX_COMMENT_ID: i64 = i16::MAX as i64;

/// Which version of the file the selected lines belong to: the changed one (`right`, added and
/// unchanged lines) or the original one (`left`, deleted lines).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffSide {
    Left,
    Right,
}

/// A selected line range of one file, as sent by the diff view.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineSelection {
    pub file_path: String,
    pub side: DiffSide,
    pub start_line: i64,
    pub end_line: i64,
    /// Column just past the last selected character of `end_line` (1-based).
    pub end_offset: i64,
}

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

/// Comment text as sent: Markdown as typed (like the web), trimmed, 1-4000 characters.
fn comment_content(content: &str) -> AppResult<&str> {
    let content = content.trim();
    if content.is_empty() {
        return Err(AppError::InvalidChange("Write a comment first"));
    }
    if content.chars().count() > MAX_COMMENT_CHARS {
        return Err(AppError::InvalidChange("Comments can have at most 4000 characters"));
    }
    Ok(content)
}

/// A reply in an existing thread, answering comment `parent_comment_id` (the thread's first one).
pub fn reply_body(parent_comment_id: i64, content: &str) -> AppResult<Value> {
    let content = comment_content(content)?;
    if !(1..=MAX_COMMENT_ID).contains(&parent_comment_id) {
        return Err(AppError::InvalidChange("Unknown comment"));
    }
    Ok(json!({ "parentCommentId": parent_comment_id, "content": content, "commentType": 1 }))
}

/// Resolving (`fixed`) or reactivating (`active`) a thread; no other status is sent.
pub fn thread_status_body(status: &str) -> AppResult<Value> {
    let code = match status {
        "active" => 1,
        "fixed" => 2,
        _ => return Err(AppError::InvalidChange("Unknown thread status")),
    };
    Ok(json!({ "status": code }))
}

fn valid_thread_id(thread_id: i64) -> AppResult<i64> {
    if (1..=i64::from(i32::MAX)).contains(&thread_id) {
        Ok(thread_id)
    } else {
        Err(AppError::InvalidChange("Unknown thread"))
    }
}

/// New active thread anchored to `selection`, with `content` as its first comment (Markdown,
/// as typed, like the web).
pub fn line_comment_body(selection: &LineSelection, content: &str) -> AppResult<Value> {
    let content = comment_content(content)?;
    let LineSelection { file_path, side, start_line, end_line, end_offset } = selection;
    let path_ok = file_path.starts_with('/')
        && file_path.chars().count() <= MAX_PATH_CHARS
        && !file_path.chars().any(char::is_control);
    if !path_ok {
        return Err(AppError::InvalidChange("Unknown file"));
    }
    let range_ok = (1..=MAX_LINE).contains(start_line)
        && (*start_line..=MAX_LINE).contains(end_line)
        && (1..=MAX_OFFSET).contains(end_offset);
    if !range_ok {
        return Err(AppError::InvalidChange("Select the lines again"));
    }

    let prefix = match side {
        DiffSide::Left => "left",
        DiffSide::Right => "right",
    };
    let mut context = Map::new();
    context.insert("filePath".into(), json!(file_path));
    context.insert(format!("{prefix}FileStart"), json!({ "line": start_line, "offset": 1 }));
    context.insert(format!("{prefix}FileEnd"), json!({ "line": end_line, "offset": end_offset }));
    Ok(json!({
        "comments": [{ "parentCommentId": 0, "content": content, "commentType": 1 }],
        "status": 1,
        "threadContext": context,
    }))
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

/// Starts a comment thread on the selected lines of one file.
#[tauri::command]
pub async fn add_pull_request_line_comment(
    state: State<'_, AppState>,
    project: String,
    repository_id: String,
    id: i64,
    selection: LineSelection,
    content: String,
) -> AppResult<()> {
    let body = line_comment_body(&selection, &content)?;
    let project = valid_project(&project)?;
    let repository_id = valid_repository_id(&repository_id)?;
    let id = valid_pull_request_id(id)?;
    let client = current_client(&state)?;
    let url = pr_url(&client, &project, &repository_id, id, &["threads"]);
    send_write(&client, Method::POST, url, &body).await
}

/// Replies in an existing thread, under its first comment.
#[tauri::command]
pub async fn reply_to_pull_request_thread(
    state: State<'_, AppState>,
    project: String,
    repository_id: String,
    id: i64,
    thread_id: i64,
    parent_comment_id: i64,
    content: String,
) -> AppResult<()> {
    let body = reply_body(parent_comment_id, &content)?;
    let project = valid_project(&project)?;
    let repository_id = valid_repository_id(&repository_id)?;
    let id = valid_pull_request_id(id)?;
    let thread_id = valid_thread_id(thread_id)?.to_string();
    let client = current_client(&state)?;
    let url = pr_url(&client, &project, &repository_id, id, &["threads", &thread_id, "comments"]);
    send_write(&client, Method::POST, url, &body).await
}

/// Resolves (`fixed`) or reactivates (`active`) a thread.
#[tauri::command]
pub async fn set_pull_request_thread_status(
    state: State<'_, AppState>,
    project: String,
    repository_id: String,
    id: i64,
    thread_id: i64,
    status: String,
) -> AppResult<()> {
    let body = thread_status_body(&status)?;
    let project = valid_project(&project)?;
    let repository_id = valid_repository_id(&repository_id)?;
    let id = valid_pull_request_id(id)?;
    let thread_id = valid_thread_id(thread_id)?.to_string();
    let client = current_client(&state)?;
    let url = pr_url(&client, &project, &repository_id, id, &["threads", &thread_id]);
    send_write(&client, Method::PATCH, url, &body).await
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

    fn selection(side: DiffSide, start_line: i64, end_line: i64) -> LineSelection {
        LineSelection { file_path: "/src/main.rs".into(), side, start_line, end_line, end_offset: 12 }
    }

    #[test]
    fn line_comment_body_anchors_the_selected_side_and_lines() {
        assert_eq!(
            line_comment_body(&selection(DiffSide::Right, 3, 5), "  Looks **off**  ").unwrap(),
            json!({
                "comments": [{ "parentCommentId": 0, "content": "Looks **off**", "commentType": 1 }],
                "status": 1,
                "threadContext": {
                    "filePath": "/src/main.rs",
                    "rightFileStart": { "line": 3, "offset": 1 },
                    "rightFileEnd": { "line": 5, "offset": 12 }
                }
            })
        );
        let left = line_comment_body(&selection(DiffSide::Left, 7, 7), "why?").unwrap();
        assert_eq!(left["threadContext"]["leftFileStart"], json!({ "line": 7, "offset": 1 }));
        assert_eq!(left["threadContext"]["leftFileEnd"], json!({ "line": 7, "offset": 12 }));
        assert!(left["threadContext"].get("rightFileStart").is_none());
    }

    #[test]
    fn line_comment_body_rejects_bad_text_without_echoing_it() {
        let too_long = "x".repeat(MAX_COMMENT_CHARS + 1);
        for content in ["", "   \n\t", too_long.as_str()] {
            let err = line_comment_body(&selection(DiffSide::Right, 1, 1), content).unwrap_err();
            assert!(matches!(err, AppError::InvalidChange(_)));
            assert!(content.trim().is_empty() || !err.to_string().contains(content));
        }
        // The limit counts characters, not bytes.
        assert!(line_comment_body(&selection(DiffSide::Right, 1, 1), &"é".repeat(MAX_COMMENT_CHARS)).is_ok());
    }

    #[test]
    fn line_comment_body_rejects_bad_ranges_and_paths() {
        let bad_ranges = [(0, 1, 1), (2, 1, 1), (1, MAX_LINE + 1, 1), (-1, 3, 1), (1, 1, 0), (1, 1, MAX_OFFSET + 1)];
        for (start_line, end_line, end_offset) in bad_ranges {
            let sel = LineSelection { end_offset, ..selection(DiffSide::Right, start_line, end_line) };
            assert!(matches!(line_comment_body(&sel, "x"), Err(AppError::InvalidChange(_))), "{sel:?}");
        }
        let long_path = format!("/{}", "a".repeat(MAX_PATH_CHARS));
        for file_path in ["", "src/main.rs", "/a\nb", "/a\u{0}b", long_path.as_str()] {
            let sel = LineSelection { file_path: file_path.into(), ..selection(DiffSide::Right, 1, 1) };
            assert!(matches!(line_comment_body(&sel, "x"), Err(AppError::InvalidChange(_))), "{file_path:?}");
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
    fn reply_body_answers_the_given_comment_with_the_trimmed_text() {
        assert_eq!(
            reply_body(1, "  Done, **thanks**  ").unwrap(),
            json!({ "parentCommentId": 1, "content": "Done, **thanks**", "commentType": 1 })
        );
        assert_eq!(reply_body(MAX_COMMENT_ID, "x").unwrap()["parentCommentId"], json!(MAX_COMMENT_ID));
    }

    #[test]
    fn reply_body_rejects_bad_text_and_parents_without_echoing_them() {
        let too_long = "x".repeat(MAX_COMMENT_CHARS + 1);
        for content in ["", " \n\t", too_long.as_str()] {
            let err = reply_body(1, content).unwrap_err();
            assert!(matches!(err, AppError::InvalidChange(_)));
            assert!(content.trim().is_empty() || !err.to_string().contains(content));
        }
        assert!(reply_body(1, &"é".repeat(MAX_COMMENT_CHARS)).is_ok());
        for parent in [0, -1, MAX_COMMENT_ID + 1, i64::MAX] {
            let err = reply_body(parent, "x").unwrap_err();
            assert!(matches!(err, AppError::InvalidChange(_)), "{parent}");
            assert!(!err.to_string().contains(&parent.to_string()), "{err}");
        }
    }

    #[test]
    fn reply_limits_count_code_points_after_trimming() {
        let padded = format!("  \n{}\t ", "x".repeat(MAX_COMMENT_CHARS));
        assert_eq!(reply_body(3, &padded).unwrap()["content"].as_str().unwrap().chars().count(), MAX_COMMENT_CHARS);
        assert!(reply_body(3, &"😀".repeat(MAX_COMMENT_CHARS)).is_ok());
        assert!(reply_body(3, &"😀".repeat(MAX_COMMENT_CHARS + 1)).is_err());
        assert_eq!(reply_body(0, " ").unwrap_err().to_string(), "Write a comment first");
    }

    #[test]
    fn reply_and_status_errors_are_fixed_text() {
        let secret = "SECRET-PAT-abcdef";
        let err = reply_body(40000, secret).unwrap_err().to_string();
        assert!(!err.contains(secret) && !err.contains("40000"), "{err}");
        assert!(!thread_status_body(secret).unwrap_err().to_string().contains(secret));
        assert!(!valid_thread_id(-7).unwrap_err().to_string().contains('7'));
        assert_eq!(thread_status_body("fixed").unwrap().as_object().unwrap().len(), 1);
    }

    #[test]
    fn thread_status_body_only_resolves_or_reactivates() {
        assert_eq!(thread_status_body("fixed").unwrap(), json!({ "status": 2 }));
        assert_eq!(thread_status_body("active").unwrap(), json!({ "status": 1 }));
        for status in ["", "Fixed", "closed", "wontFix", "byDesign", "pending", "unknown", "2", "fixed "] {
            let err = thread_status_body(status).unwrap_err();
            assert!(matches!(err, AppError::InvalidChange(_)), "{status:?}");
            assert!(status.is_empty() || !err.to_string().contains(status), "{err}");
        }
    }

    #[test]
    fn thread_ids_are_validated() {
        assert_eq!(valid_thread_id(7).unwrap(), 7);
        assert_eq!(valid_thread_id(i64::from(i32::MAX)).unwrap(), i64::from(i32::MAX));
        for bad in [0, -1, i64::from(i32::MAX) + 1] {
            assert!(matches!(valid_thread_id(bad), Err(AppError::InvalidChange(_))), "{bad}");
        }
    }

    #[test]
    fn my_vote_matches_the_signed_in_user_case_insensitively() {
        let reviewers = [reviewer("AAA", 5), reviewer("bbb", -10)];
        assert_eq!(my_vote(&reviewers, "aaa"), 5);
        assert_eq!(my_vote(&reviewers, "BBB"), -10);
        assert_eq!(my_vote(&reviewers, "ccc"), 0);
    }
}
