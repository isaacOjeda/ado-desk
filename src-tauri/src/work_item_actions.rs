//! Work item writes (roadmap phase 6): add a comment, change the state, change the assignee.
//!
//! Every value the webview sends is validated here before it reaches Azure DevOps, and every
//! write goes through [`AzdoClient::write`], whose failures never sign the user out.

use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::State;

use crate::AppState;
use crate::azdo::model::ListResponse;
use crate::azdo::{AzdoClient, WriteAccess};
use crate::commands::current_client;
use crate::error::{AppError, AppResult};
use crate::queries::{self, TeamMember};
use crate::themes::is_format_char;

/// Longest comment accepted, in characters of plain text (before HTML escaping).
pub const MAX_COMMENT_CHARS: usize = 4000;
/// Longest work item type, state or assignee name accepted.
const MAX_NAME_CHARS: usize = 256;

const JSON_PATCH: &str = "application/json-patch+json";
const JSON: &str = "application/json";
const COMMENTS_API_VERSION: &str = "7.1-preview.4";

// ---------------------------------------------------------------------------
// Pure validation and payloads
// ---------------------------------------------------------------------------

/// Plain comment text -> the HTML Azure DevOps stores: control and format characters removed
/// (line breaks kept), trimmed, 1 to [`MAX_COMMENT_CHARS`] characters, escaped, lines joined
/// with `<br>`.
pub fn comment_html(input: &str) -> AppResult<String> {
    let cleaned: String = input
        .chars()
        .filter(|c| (*c == '\n' || !c.is_control()) && !is_format_char(*c))
        .collect();
    let text = cleaned.trim();
    let length = text.chars().count();
    if length == 0 || length > MAX_COMMENT_CHARS {
        return Err(AppError::InvalidChange(
            "Comments need 1 to 4000 characters",
        ));
    }
    let lines: Vec<String> = text.split('\n').map(escape_html).collect();
    Ok(lines.join("<br>"))
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// The requested state, spelled as the type defines it, or an error when the type has no
/// such state.
pub fn validate_state(requested: &str, states: &[String]) -> AppResult<String> {
    let requested = requested.trim();
    states
        .iter()
        .find(|state| state.as_str() == requested)
        .or_else(|| {
            states
                .iter()
                .find(|state| !requested.is_empty() && state.eq_ignore_ascii_case(requested))
        })
        .cloned()
        .ok_or(AppError::InvalidChange(
            "Pick a state of this work item type",
        ))
}

/// The assignee to send: `""` to unassign, otherwise the unique name of a default team member
/// or of the signed-in user (`me`), spelled as Azure DevOps returned it.
pub fn validate_assignee(
    requested: Option<&str>,
    members: &[TeamMember],
    me: Option<&str>,
) -> AppResult<String> {
    let requested = requested.map(str::trim).unwrap_or_default();
    if requested.is_empty() {
        return Ok(String::new());
    }
    members
        .iter()
        .map(|member| member.unique_name.as_str())
        .chain(me)
        .find(|name| name.eq_ignore_ascii_case(requested))
        .map(str::to_string)
        .ok_or(AppError::InvalidChange(
            "Pick a member of the project's team",
        ))
}

/// A work item type name as received from the webview.
pub fn validate_type(work_item_type: &str) -> AppResult<String> {
    let trimmed = work_item_type.trim();
    let valid = !trimmed.is_empty()
        && trimmed.chars().count() <= MAX_NAME_CHARS
        && !work_item_type
            .chars()
            .any(|c| c.is_control() || is_format_char(c));
    if valid {
        Ok(trimmed.to_string())
    } else {
        Err(AppError::InvalidChange("Unknown work item type"))
    }
}

/// JSON Patch that sets one field only if the work item is still at revision `rev`.
pub fn field_patch(rev: i64, field: &str, value: &str) -> AppResult<Value> {
    Ok(json!([
        revision_test(rev)?,
        { "op": "add", "path": field, "value": value },
    ]))
}

/// JSON Patch used to probe write access: a lone test of the revision, sent with
/// `validateOnly=true`, so nothing is saved.
pub fn probe_patch(rev: i64) -> AppResult<Value> {
    Ok(json!([revision_test(rev)?]))
}

/// What the probe outcome means for the session: `Some` outcomes are remembered.
pub fn probe_access(outcome: &AppResult<()>) -> Option<WriteAccess> {
    match outcome {
        Ok(()) => Some(WriteAccess::Granted),
        Err(AppError::WriteDenied) => Some(WriteAccess::Denied),
        Err(_) => None,
    }
}

fn revision_test(rev: i64) -> AppResult<Value> {
    if rev > 0 {
        Ok(json!({ "op": "test", "path": "/rev", "value": rev }))
    } else {
        Err(AppError::InvalidChange(
            "Reload the work item and try again",
        ))
    }
}

fn valid_id(id: i64) -> AppResult<String> {
    if id > 0 {
        Ok(id.to_string())
    } else {
        Err(AppError::InvalidChange("Unknown work item"))
    }
}

fn valid_project(project: &str) -> AppResult<String> {
    queries::validate_project(project).map_err(|_| AppError::InvalidChange("Unknown project"))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawState {
    name: String,
}

/// States of one work item type, in the order the process defines them.
async fn type_states(
    client: &AzdoClient,
    project: &str,
    work_item_type: &str,
) -> AppResult<Vec<String>> {
    let url = client.url(&[
        project,
        "_apis",
        "wit",
        "workitemtypes",
        work_item_type,
        "states",
    ]);
    let list: ListResponse<Value> = client.get_json(url).await?;
    Ok(list
        .value
        .into_iter()
        .filter_map(|item| serde_json::from_value::<RawState>(item).ok())
        .map(|state| state.name)
        .filter(|name| !name.trim().is_empty() && name.chars().count() <= MAX_NAME_CHARS)
        .collect())
}

/// Refuses early when this session already knows the token cannot write work items.
fn ensure_writable(client: &AzdoClient) -> AppResult<()> {
    match client.work_write_access() {
        WriteAccess::Denied => Err(AppError::WriteDenied),
        _ => Ok(()),
    }
}

/// Sends a write and records a denial, so the detail switches to its read-only hint.
async fn send_write(
    client: &AzdoClient,
    method: Method,
    url: url::Url,
    body: &Value,
    content_type: &'static str,
) -> AppResult<()> {
    let result = client.write(method, url, body, content_type).await;
    match &result {
        Ok(()) => client.record_work_write_access(WriteAccess::Granted),
        Err(AppError::WriteDenied) => client.record_work_write_access(WriteAccess::Denied),
        Err(_) => {}
    }
    result
}

/// Whether the actions should show for this organization: `false` only when Azure DevOps
/// refused a write (or the write probe) for lack of access.
#[tauri::command]
pub async fn work_item_write_access(
    state: State<'_, AppState>,
    id: i64,
    rev: i64,
) -> AppResult<bool> {
    let client = current_client(&state)?;
    if client.work_write_access() == WriteAccess::Unknown {
        let id = valid_id(id)?;
        let mut url = client.url(&["_apis", "wit", "workitems", &id]);
        url.query_pairs_mut().append_pair("validateOnly", "true");
        let outcome = client
            .write(Method::PATCH, url, &probe_patch(rev)?, JSON_PATCH)
            .await;
        if let Err(err) = &outcome {
            eprintln!("work item write probe: {}", err.kind());
        }
        if let Some(access) = probe_access(&outcome) {
            client.record_work_write_access(access);
        }
    }
    Ok(client.work_write_access() != WriteAccess::Denied)
}

#[tauri::command]
pub async fn list_work_item_states(
    state: State<'_, AppState>,
    project: String,
    work_item_type: String,
) -> AppResult<Vec<String>> {
    let client = current_client(&state)?;
    let project = valid_project(&project)?;
    let work_item_type = validate_type(&work_item_type)?;
    type_states(&client, &project, &work_item_type).await
}

#[tauri::command]
pub async fn set_work_item_state(
    state: State<'_, AppState>,
    project: String,
    work_item_type: String,
    id: i64,
    rev: i64,
    value: String,
) -> AppResult<()> {
    let client = current_client(&state)?;
    ensure_writable(&client)?;
    let project = valid_project(&project)?;
    let work_item_type = validate_type(&work_item_type)?;
    let id = valid_id(id)?;
    let states = type_states(&client, &project, &work_item_type).await?;
    let value = validate_state(&value, &states)?;
    let patch = field_patch(rev, "/fields/System.State", &value)?;
    let url = client.url(&[&project, "_apis", "wit", "workitems", &id]);
    send_write(&client, Method::PATCH, url, &patch, JSON_PATCH).await
}

#[tauri::command]
pub async fn set_work_item_assignee(
    state: State<'_, AppState>,
    project: String,
    id: i64,
    rev: i64,
    assignee: Option<String>,
) -> AppResult<()> {
    let client = current_client(&state)?;
    ensure_writable(&client)?;
    let project = valid_project(&project)?;
    let id = valid_id(id)?;
    let members = match assignee.as_deref().map(str::trim) {
        // Unassigning needs no lookup.
        None | Some("") => Vec::new(),
        Some(_) => queries::default_team_members(&client, &project).await?,
    };
    let value = validate_assignee(
        assignee.as_deref(),
        &members,
        client.session().email.as_deref(),
    )?;
    let patch = field_patch(rev, "/fields/System.AssignedTo", &value)?;
    let url = client.url(&[&project, "_apis", "wit", "workitems", &id]);
    send_write(&client, Method::PATCH, url, &patch, JSON_PATCH).await
}

#[tauri::command]
pub async fn add_work_item_comment(
    state: State<'_, AppState>,
    project: String,
    id: i64,
    text: String,
) -> AppResult<()> {
    let client = current_client(&state)?;
    ensure_writable(&client)?;
    let project = valid_project(&project)?;
    let id = valid_id(id)?;
    let html = comment_html(&text)?;
    let url = client.url_with_version(
        &[&project, "_apis", "wit", "workItems", &id, "comments"],
        COMMENTS_API_VERSION,
    );
    send_write(&client, Method::POST, url, &json!({ "text": html }), JSON).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(display: &str, unique: &str) -> TeamMember {
        TeamMember {
            display_name: display.into(),
            unique_name: unique.into(),
        }
    }

    fn rejected<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        match result {
            Err(AppError::InvalidChange(message)) => message.to_string(),
            other => panic!("expected InvalidChange, got {other:?}"),
        }
    }

    #[test]
    fn comment_html_escapes_markup_and_keeps_line_breaks() {
        assert_eq!(
            comment_html("  Fixed <b>it</b> & \"done\"\nsee 'notes'\r\n").unwrap(),
            "Fixed &lt;b&gt;it&lt;/b&gt; &amp; &quot;done&quot;<br>see &#39;notes&#39;"
        );
    }

    #[test]
    fn comment_html_strips_control_and_format_characters() {
        assert_eq!(
            comment_html("a\u{0007}b\u{202E}c\u{200B}d").unwrap(),
            "abcd"
        );
    }

    #[test]
    fn comment_html_requires_1_to_4000_characters() {
        let message = rejected(comment_html(" \n\u{202E} "));
        assert_eq!(message, "Comments need 1 to 4000 characters");
        assert!(comment_html(&"x".repeat(MAX_COMMENT_CHARS)).is_ok());
        assert_eq!(
            rejected(comment_html(&"x".repeat(MAX_COMMENT_CHARS + 1))),
            message
        );
        // Characters, not bytes: 4000 accented letters are accepted.
        assert!(comment_html(&"é".repeat(MAX_COMMENT_CHARS)).is_ok());
    }

    #[test]
    fn validate_state_returns_the_spelling_of_the_type() {
        let states = vec!["New".to_string(), "Active".into(), "Resolved".into()];
        assert_eq!(validate_state("Active", &states).unwrap(), "Active");
        assert_eq!(validate_state(" resolved ", &states).unwrap(), "Resolved");
        assert_eq!(
            rejected(validate_state("Closed", &states)),
            "Pick a state of this work item type"
        );
        assert_eq!(
            rejected(validate_state("", &states)),
            "Pick a state of this work item type"
        );
    }

    #[test]
    fn validate_assignee_accepts_team_members_me_or_nobody() {
        let members = vec![
            member("Ana Ruiz", "ana@contoso.com"),
            member("Leo Park", "leo@contoso.com"),
        ];
        assert_eq!(validate_assignee(None, &members, None).unwrap(), "");
        assert_eq!(validate_assignee(Some("  "), &members, None).unwrap(), "");
        assert_eq!(
            validate_assignee(Some("LEO@contoso.com"), &members, None).unwrap(),
            "leo@contoso.com"
        );
        assert_eq!(
            validate_assignee(Some("me@contoso.com"), &members, Some("Me@Contoso.com")).unwrap(),
            "Me@Contoso.com"
        );
        assert_eq!(
            rejected(validate_assignee(
                Some("eve@evil.com"),
                &members,
                Some("me@contoso.com")
            )),
            "Pick a member of the project's team"
        );
    }

    #[test]
    fn validate_type_rejects_empty_long_or_control_names() {
        assert_eq!(validate_type(" Bug ").unwrap(), "Bug");
        assert_eq!(validate_type("User Story").unwrap(), "User Story");
        for bad in [
            "",
            "  ",
            "Bug\n",
            "B\u{202E}ug",
            &"x".repeat(MAX_NAME_CHARS + 1),
        ] {
            assert_eq!(
                rejected(validate_type(bad)),
                "Unknown work item type",
                "{bad:?}"
            );
        }
    }

    #[test]
    fn field_patch_tests_the_revision_before_setting_the_field() {
        assert_eq!(
            field_patch(7, "/fields/System.State", "Active").unwrap(),
            json!([
                { "op": "test", "path": "/rev", "value": 7 },
                { "op": "add", "path": "/fields/System.State", "value": "Active" },
            ])
        );
        assert_eq!(
            rejected(field_patch(0, "/fields/System.State", "Active")),
            "Reload the work item and try again"
        );
    }

    #[test]
    fn probe_patch_only_tests_the_revision() {
        assert_eq!(
            probe_patch(3).unwrap(),
            json!([{ "op": "test", "path": "/rev", "value": 3 }])
        );
        assert!(probe_patch(-1).is_err());
    }

    #[test]
    fn probe_access_remembers_only_definite_answers() {
        assert_eq!(probe_access(&Ok(())), Some(WriteAccess::Granted));
        assert_eq!(
            probe_access(&Err(AppError::WriteDenied)),
            Some(WriteAccess::Denied)
        );
        assert_eq!(probe_access(&Err(AppError::WriteRejected)), None);
        assert_eq!(probe_access(&Err(AppError::NotFound)), None);
        assert_eq!(
            probe_access(&Err(AppError::Network("timeout".into()))),
            None
        );
    }
}
