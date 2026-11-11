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
use crate::azdo::{AzdoClient, WriteAccess, WriteBody};
use crate::commands::current_client;
use crate::error::{AppError, AppResult};
use crate::queries::{self, TeamMember};
use crate::themes::is_format_char;
use crate::work_items::{self, RawComment, WorkItemComment};

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

/// Plain text of a stored comment, when the app can edit it without dropping anything: HTML
/// format (not markdown), no tags but `<br>`, only the entities [`comment_html`] writes, and
/// text a new comment would accept unchanged (no leading or trailing whitespace). Formatting,
/// mentions or markdown give `None`.
pub fn editable_comment_text(text: &str, format: Option<&str>) -> Option<String> {
    if format.is_some_and(|f| !f.eq_ignore_ascii_case("html")) {
        return None;
    }
    let mut plain = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let taken = match c {
            '<' => {
                let tag = LINE_BREAKS.iter().find(|tag| {
                    rest.get(..tag.len())
                        .is_some_and(|head| head.eq_ignore_ascii_case(tag))
                })?;
                plain.push('\n');
                tag.len()
            }
            '&' => {
                let (entity, decoded) = ENTITIES
                    .iter()
                    .find(|(entity, _)| rest.starts_with(entity))?;
                plain.push(*decoded);
                entity.len()
            }
            // A raw line break renders as a space in HTML; editing would turn it into `<br>`.
            c if c.is_control() || is_format_char(c) => return None,
            c => {
                plain.push(c);
                c.len_utf8()
            }
        };
        rest = &rest[taken..];
    }
    // A save trims, so leading or trailing whitespace would be lost.
    (plain.trim() == plain && comment_html(&plain).is_ok()).then_some(plain)
}

const LINE_BREAKS: [&str; 3] = ["<br>", "<br/>", "<br />"];

const ENTITIES: [(&str, char); 7] = [
    ("&amp;", '&'),
    ("&lt;", '<'),
    ("&gt;", '>'),
    ("&quot;", '"'),
    ("&#39;", '\''),
    ("&#x27;", '\''),
    ("&apos;", '\''),
];

/// Refuses an edit or delete of `comment` (`None` when Azure DevOps reports it deleted)
/// unless it is the user's and still at `seen_version`; an edit also needs editable text.
pub fn check_comment_change(
    comment: Option<&WorkItemComment>,
    seen_version: i64,
    edit: bool,
) -> AppResult<()> {
    let stale = || AppError::InvalidChange("Reload the work item and try again");
    let comment = comment.ok_or_else(stale)?;
    if !comment.is_mine {
        return Err(AppError::InvalidChange(
            "You can only change your own comments",
        ));
    }
    if comment.version != seen_version {
        return Err(stale());
    }
    if edit && comment.editable_text.is_none() {
        return Err(AppError::InvalidChange(
            "This comment has formatting; edit it in Azure DevOps",
        ));
    }
    Ok(())
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

/// The error of a read made before a write (states, team members), without server text: the
/// failure of a write command must not quote Azure DevOps. Other errors pass through.
pub fn pre_read_error(err: AppError) -> AppError {
    const MESSAGE: &str = "could not check the work item before the change";
    match err {
        AppError::Api { status, .. } => AppError::Api {
            status,
            message: MESSAGE.into(),
        },
        AppError::Decode(_) => AppError::Decode(MESSAGE.into()),
        other => other,
    }
}

/// A work item type name as received from the webview.
pub fn validate_type(work_item_type: &str) -> AppResult<String> {
    let trimmed = work_item_type.trim();
    // `.` and `..` would be dropped from the URL path instead of failing.
    let valid = !trimmed.is_empty()
        && trimmed.chars().any(|c| c != '.')
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

/// What the session remembers after a write or the probe answered `outcome`: an accepted
/// write grants; a denial forgets, since it may concern one item only (an area the user cannot
/// edit), so the next detail probes again; anything else changes nothing.
pub fn remembered_access(current: WriteAccess, outcome: &AppResult<()>) -> WriteAccess {
    match outcome {
        Ok(()) => WriteAccess::Granted,
        Err(AppError::WriteDenied) => WriteAccess::Unknown,
        Err(_) => current,
    }
}

/// Whether a detail shows the actions after its probe answered `outcome`: only a denial for
/// lack of access hides them.
pub fn shows_actions(outcome: &AppResult<()>) -> bool {
    !matches!(outcome, Err(AppError::WriteDenied))
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

fn valid_comment_id(comment_id: i64) -> AppResult<String> {
    if comment_id > 0 {
        Ok(comment_id.to_string())
    } else {
        Err(AppError::InvalidChange("Unknown comment"))
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

/// Sends a write and updates what the session remembers about write access.
async fn send_write(
    client: &AzdoClient,
    method: Method,
    url: url::Url,
    body: Option<WriteBody<'_>>,
) -> AppResult<()> {
    let result = client.write(method, url, body).await;
    client.record_work_write_access(remembered_access(client.work_write_access(), &result));
    result
}

/// Whether the detail of work item `id` (at revision `rev`) shows the actions: `false` only
/// when Azure DevOps refused a validate-only update for lack of access. Once a write was
/// accepted, the session skips the probe.
#[tauri::command]
pub async fn work_item_write_access(
    state: State<'_, AppState>,
    id: i64,
    rev: i64,
) -> AppResult<bool> {
    let client = current_client(&state)?;
    if client.work_write_access() == WriteAccess::Granted {
        return Ok(true);
    }
    // Without a usable id and revision there is nothing to probe: show the actions.
    let (Ok(id), Ok(patch)) = (valid_id(id), probe_patch(rev)) else {
        return Ok(true);
    };
    let mut url = client.url(&["_apis", "wit", "workitems", &id]);
    url.query_pairs_mut().append_pair("validateOnly", "true");
    let outcome = client
        .write(Method::PATCH, url, Some((&patch, JSON_PATCH)))
        .await;
    if let Err(err) = &outcome {
        eprintln!("work item write probe: {}", err.kind());
    }
    client.record_work_write_access(remembered_access(client.work_write_access(), &outcome));
    Ok(shows_actions(&outcome))
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
    let project = valid_project(&project)?;
    let work_item_type = validate_type(&work_item_type)?;
    let id = valid_id(id)?;
    let states = type_states(&client, &project, &work_item_type)
        .await
        .map_err(pre_read_error)?;
    let value = validate_state(&value, &states)?;
    let patch = field_patch(rev, "/fields/System.State", &value)?;
    let url = client.url(&[&project, "_apis", "wit", "workitems", &id]);
    send_write(&client, Method::PATCH, url, Some((&patch, JSON_PATCH))).await
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
    let project = valid_project(&project)?;
    let id = valid_id(id)?;
    let members = match assignee.as_deref().map(str::trim) {
        // Unassigning needs no lookup.
        None | Some("") => Vec::new(),
        Some(_) => queries::default_team_members(&client, &project)
            .await
            .map_err(pre_read_error)?,
    };
    let value = validate_assignee(
        assignee.as_deref(),
        &members,
        client.session().email.as_deref(),
    )?;
    let patch = field_patch(rev, "/fields/System.AssignedTo", &value)?;
    let url = client.url(&[&project, "_apis", "wit", "workitems", &id]);
    send_write(&client, Method::PATCH, url, Some((&patch, JSON_PATCH))).await
}

#[tauri::command]
pub async fn add_work_item_comment(
    state: State<'_, AppState>,
    project: String,
    id: i64,
    text: String,
) -> AppResult<()> {
    let client = current_client(&state)?;
    let project = valid_project(&project)?;
    let id = valid_id(id)?;
    let html = comment_html(&text)?;
    let url = client.url_with_version(
        &[&project, "_apis", "wit", "workItems", &id, "comments"],
        COMMENTS_API_VERSION,
    );
    send_write(
        &client,
        Method::POST,
        url,
        Some((&json!({ "text": html }), JSON)),
    )
    .await
}

fn comment_url(client: &AzdoClient, project: &str, id: &str, comment_id: &str) -> url::Url {
    client.url_with_version(
        &[
            project,
            "_apis",
            "wit",
            "workItems",
            id,
            "comments",
            comment_id,
        ],
        COMMENTS_API_VERSION,
    )
}

/// The comment as Azure DevOps has it now (`None` once deleted), mapped exactly as the
/// discussion maps it, so ownership and editability agree with what the user saw.
async fn current_comment(
    client: &AzdoClient,
    project: &str,
    id: &str,
    comment_id: &str,
) -> AppResult<Option<WorkItemComment>> {
    let mut url = comment_url(client, project, id, comment_id);
    url.query_pairs_mut().append_pair("includeDeleted", "true");
    let raw: RawComment = client.get_json(url).await.map_err(pre_read_error)?;
    Ok((!raw.is_deleted)
        .then(|| work_items::map_comment(client.org(), &client.session().user_id, raw)))
}

/// Replaces the text of one of the user's plain comments. The update API has no revision
/// test, so the comment is read first and must still be at `version`.
#[tauri::command]
pub async fn update_work_item_comment(
    state: State<'_, AppState>,
    project: String,
    id: i64,
    comment_id: i64,
    version: i64,
    text: String,
) -> AppResult<()> {
    let client = current_client(&state)?;
    let project = valid_project(&project)?;
    let id = valid_id(id)?;
    let comment_id = valid_comment_id(comment_id)?;
    let html = comment_html(&text)?;
    let current = current_comment(&client, &project, &id, &comment_id).await?;
    check_comment_change(current.as_ref(), version, true)?;
    let url = comment_url(&client, &project, &id, &comment_id);
    send_write(
        &client,
        Method::PATCH,
        url,
        Some((&json!({ "text": html }), JSON)),
    )
    .await
}

/// Deletes one of the user's comments (Azure DevOps keeps it in the History tab).
#[tauri::command]
pub async fn delete_work_item_comment(
    state: State<'_, AppState>,
    project: String,
    id: i64,
    comment_id: i64,
    version: i64,
) -> AppResult<()> {
    let client = current_client(&state)?;
    let project = valid_project(&project)?;
    let id = valid_id(id)?;
    let comment_id = valid_comment_id(comment_id)?;
    let current = current_comment(&client, &project, &id, &comment_id).await?;
    check_comment_change(current.as_ref(), version, false)?;
    let url = comment_url(&client, &project, &id, &comment_id);
    send_write(&client, Method::DELETE, url, None).await
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
    fn editable_comment_text_reverses_comment_html() {
        for input in [
            "Fixed <b>it</b> & \"done\"\nsee 'notes'",
            "one line",
            "a\n\nb",
            "&amp; literally",
            "é ñ 日本",
        ] {
            let stored = comment_html(input).unwrap();
            assert_eq!(
                editable_comment_text(&stored, None).as_deref(),
                Some(input),
                "{stored}"
            );
            assert_eq!(
                editable_comment_text(&stored, Some("html")).as_deref(),
                Some(input)
            );
        }
        assert_eq!(
            editable_comment_text("a<br/>b<BR />c&#x27;&apos;", None).as_deref(),
            Some("a\nb\nc''")
        );
    }

    #[test]
    fn editable_comment_text_refuses_anything_it_would_drop() {
        for stored in [
            "<div>a</div>",
            "a <b>b</b>",
            "<a href=\"#\" data-vss-mention=\"version:2.0,x\">@Ada</a> hi",
            "a&nbsp;b",
            "a & b",
            "a\nb",
            "",
            "<br>",
        ] {
            assert_eq!(editable_comment_text(stored, None), None, "{stored}");
        }
        assert_eq!(editable_comment_text("plain", Some("markdown")), None);
        assert_eq!(editable_comment_text("plain", Some("other")), None);
        assert_eq!(
            editable_comment_text(&"x".repeat(MAX_COMMENT_CHARS + 1), None),
            None
        );
    }

    fn own_comment(version: i64, editable: bool, mine: bool) -> WorkItemComment {
        WorkItemComment {
            id: 1,
            version,
            author: "Ada".into(),
            is_mine: mine,
            editable_text: editable.then(|| "text".to_string()),
            created_at: None,
            modified_at: None,
            is_edited: false,
            html: String::new(),
        }
    }

    #[test]
    fn valid_comment_id_rejects_non_positive_ids() {
        assert_eq!(valid_comment_id(12).unwrap(), "12");
        assert_eq!(rejected(valid_comment_id(0)), "Unknown comment");
        assert_eq!(rejected(valid_comment_id(-3)), "Unknown comment");
    }

    #[test]
    fn check_comment_change_needs_the_users_current_comment() {
        let ok = own_comment(2, true, true);
        assert!(check_comment_change(Some(&ok), 2, true).is_ok());
        assert!(check_comment_change(Some(&ok), 2, false).is_ok());

        let stale = "Reload the work item and try again";
        assert_eq!(rejected(check_comment_change(None, 2, false)), stale);
        assert_eq!(rejected(check_comment_change(Some(&ok), 1, true)), stale);
        assert_eq!(rejected(check_comment_change(Some(&ok), 3, false)), stale);
        assert_eq!(
            rejected(check_comment_change(
                Some(&own_comment(2, true, false)),
                2,
                false
            )),
            "You can only change your own comments"
        );
        let formatted = own_comment(2, false, true);
        assert!(check_comment_change(Some(&formatted), 2, false).is_ok());
        assert_eq!(
            rejected(check_comment_change(Some(&formatted), 2, true)),
            "This comment has formatting; edit it in Azure DevOps"
        );
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
    fn pre_read_error_drops_server_text() {
        let api = pre_read_error(AppError::Api {
            status: 500,
            message: "TF1: secret detail".into(),
        });
        assert_eq!(
            api.to_string(),
            "Azure DevOps returned HTTP 500: could not check the work item before the change"
        );
        let decode = pre_read_error(AppError::Decode("invalid type: string \"x\"".into()));
        assert_eq!(
            decode.to_string(),
            "Unexpected response from Azure DevOps: could not check the work item before the change"
        );
        assert!(matches!(
            pre_read_error(AppError::NotFound),
            AppError::NotFound
        ));
        assert!(matches!(
            pre_read_error(AppError::Forbidden),
            AppError::Forbidden
        ));
    }

    #[test]
    fn remembered_access_grants_on_success_and_forgets_on_denial() {
        use WriteAccess::{Granted, Unknown};
        assert_eq!(remembered_access(Unknown, &Ok(())), Granted);
        assert_eq!(remembered_access(Granted, &Ok(())), Granted);
        // A denial may be specific to one item: never remembered as an organization-wide no.
        assert_eq!(
            remembered_access(Granted, &Err(AppError::WriteDenied)),
            Unknown
        );
        assert_eq!(
            remembered_access(Unknown, &Err(AppError::WriteDenied)),
            Unknown
        );
        for other in [
            AppError::WriteRejected,
            AppError::NotFound,
            AppError::Network("timeout".into()),
        ] {
            assert_eq!(remembered_access(Granted, &Err(other)), Granted);
        }
        assert_eq!(
            remembered_access(Unknown, &Err(AppError::WriteRejected)),
            Unknown
        );
    }

    #[test]
    fn shows_actions_unless_the_probe_was_denied() {
        assert!(shows_actions(&Ok(())));
        assert!(!shows_actions(&Err(AppError::WriteDenied)));
        assert!(shows_actions(&Err(AppError::WriteRejected)));
        assert!(shows_actions(&Err(AppError::NotFound)));
        assert!(shows_actions(&Err(AppError::Network("timeout".into()))));
    }
}

#[cfg(test)]
mod verifier_probes {
    use super::*;
    use crate::azdo::model::build_url;

    fn member(display: &str, unique: &str) -> TeamMember {
        TeamMember {
            display_name: display.into(),
            unique_name: unique.into(),
        }
    }

    #[test]
    fn probe_write_errors_serialize_fixed_text() {
        for (err, kind) in [
            (AppError::WriteDenied, "writeDenied"),
            (AppError::WriteRejected, "writeRejected"),
            (AppError::InvalidChange("Unknown project"), "invalidChange"),
        ] {
            let json = serde_json::to_value(&err).unwrap();
            assert_eq!(json["kind"], kind);
            assert!(!json["message"].as_str().unwrap().is_empty());
        }
    }

    /// No crafted value is echoed back in a validation error.
    #[test]
    fn probe_validation_errors_never_echo_input() {
        let secret = "SECRETPAT0123456789abcdef";
        let members = vec![member("Ana", "ana@contoso.com")];
        let errors = [
            validate_state(secret, &["New".into()]).unwrap_err(),
            validate_assignee(Some(secret), &members, Some("me@contoso.com")).unwrap_err(),
            validate_type(&format!("{secret}\n")).unwrap_err(),
            comment_html(&secret.repeat(400)).unwrap_err(),
            valid_project(&format!("{secret}/x")).unwrap_err(),
        ];
        for err in errors {
            assert!(!err.to_string().contains(secret), "{err}");
            assert!(!serde_json::to_string(&err).unwrap().contains(secret));
        }
    }

    /// Escaped comments never carry markup: only `<br>` tags remain and no raw `<`, `>` or `"`.
    #[test]
    fn probe_comment_html_never_carries_markup() {
        for input in [
            "<script>alert(1)</script>",
            "<img src=x onerror=alert(1)>",
            "&lt;b&gt; already escaped",
            "a\r\nb\rc\u{2028}d\u{0085}e",
            "<a href=\"javascript:alert(1)\">x</a>\n<br>",
            "line1\n\n\nline2",
            "]]><!--",
        ] {
            let html = comment_html(input).unwrap();
            let without_br = html.replace("<br>", "");
            assert!(
                !without_br.contains('<') && !without_br.contains('>'),
                "{input:?} -> {html}"
            );
            assert!(
                !without_br.contains('"') && !without_br.contains('\''),
                "{html}"
            );
        }
        assert_eq!(comment_html("&lt;").unwrap(), "&amp;lt;");
        // CR is dropped and LF kept; LINE SEPARATOR and NEL are removed.
        assert_eq!(
            comment_html("a\r\nb\rc\u{2028}d\u{0085}e").unwrap(),
            "a<br>bcde"
        );
        // Interior blank lines are kept as line breaks.
        assert_eq!(comment_html("a\n\nb").unwrap(), "a<br><br>b");
    }

    /// The length bound counts the cleaned text, not stripped characters.
    #[test]
    fn probe_comment_length_counts_cleaned_text() {
        let padded = format!("{}{}", "x".repeat(MAX_COMMENT_CHARS), "\u{200B}".repeat(10));
        assert!(comment_html(&padded).is_ok());
        let crlf = "x\r\n".repeat(MAX_COMMENT_CHARS / 2);
        assert!(comment_html(&crlf).is_ok(), "CR not counted");
    }

    #[test]
    fn probe_state_comes_from_the_type_list_only() {
        let states = vec!["New".to_string(), "Active".into()];
        assert_eq!(validate_state("Active\n", &states).unwrap(), "Active");
        assert!(validate_state("Active\u{202E}", &states).is_err());
        assert!(validate_state("Active", &[]).is_err());
        assert!(validate_state("<b>Active</b>", &states).is_err());
    }

    #[test]
    fn probe_assignee_comes_from_team_or_me_only() {
        let members = vec![member("Ana", "ana@contoso.com")];
        // Nobody to choose from: only unassigning passes.
        assert!(validate_assignee(Some("ana@contoso.com"), &[], None).is_err());
        assert_eq!(validate_assignee(Some(""), &[], None).unwrap(), "");
        assert!(validate_assignee(Some("ana@contoso.com\u{202E}"), &members, None).is_err());
        assert!(
            validate_assignee(Some("Ana"), &members, None).is_err(),
            "display name is not accepted"
        );
        assert!(validate_assignee(Some("ana@contoso.com.evil"), &members, None).is_err());
    }

    /// State and assignee patches always test the revision first; the probe only tests it.
    #[test]
    fn probe_patches_always_test_the_revision() {
        for rev in [1, 2, i64::MAX] {
            for field in ["/fields/System.State", "/fields/System.AssignedTo"] {
                let patch = field_patch(rev, field, "v").unwrap();
                let ops = patch.as_array().unwrap();
                assert_eq!(ops.len(), 2);
                assert_eq!(
                    ops[0],
                    json!({ "op": "test", "path": "/rev", "value": rev })
                );
                assert_eq!(ops[1]["path"], field);
            }
            let probe = probe_patch(rev).unwrap();
            assert_eq!(
                probe,
                json!([{ "op": "test", "path": "/rev", "value": rev }])
            );
        }
        for rev in [0, -1, i64::MIN] {
            assert!(field_patch(rev, "/fields/System.State", "v").is_err());
            assert!(probe_patch(rev).is_err());
        }
        assert!(valid_id(0).is_err() && valid_id(-5).is_err());
        assert_eq!(valid_id(42).unwrap(), "42");
    }

    #[test]
    fn probe_project_rejects_path_tricks() {
        for bad in [
            "..", ".", "_apis", "a/b", "a\\b", "a?b", "a#b", "", " ", "p\n",
        ] {
            assert!(valid_project(bad).is_err(), "{bad:?}");
        }
    }

    /// A crafted work item type stays inside the active organization and project. `..` passes
    /// validation and the url crate drops it (`.../workitemtypes/states`): a GET in the same
    /// project, never a write, since the PATCH URL does not carry the type.
    #[test]
    fn probe_type_segment_stays_in_org_and_project() {
        let path = |t: &str| {
            let t = validate_type(t).unwrap();
            build_url(&[
                "contoso",
                "proj",
                "_apis",
                "wit",
                "workitemtypes",
                &t,
                "states",
            ])
        };
        for dots in [".", ".."] {
            assert!(matches!(
                validate_type(dots),
                Err(AppError::InvalidChange("Unknown work item type"))
            ));
        }
        for (crafted, segment) in [
            ("%2e%2e", "%252e%252e"),
            ("a/b", "a%2Fb"),
            ("a\\b", "a%5Cb"),
            ("a?b#c", "a%3Fb%23c"),
            ("../../other", "..%2F..%2Fother"),
        ] {
            let url = path(crafted);
            assert_eq!(url.host_str(), Some("dev.azure.com"));
            assert_eq!(
                url.path(),
                format!("/contoso/proj/_apis/wit/workitemtypes/{segment}/states"),
                "{crafted}"
            );
            assert!(url.query().is_none() && url.fragment().is_none());
        }
    }
}

#[cfg(test)]
mod comment_edit_probes {
    use super::*;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) as usize
        }
    }

    const TOKENS: &[&str] = &[
        "<br>", "<BR>", "<br/>", "<Br />", "<br />", "<brx>", "<br", "<b>", "<", ">", "&amp;",
        "&AMP;", "&amp", "&lt;", "&gt;", "&quot;", "&#39;", "&#39", "&#x27;", "&apos;", "&nbsp;",
        "&", "\"", "'", "\n", "\r", "\t", " ", "  ", "a", "Z", "é", "日本", "😀", "\u{200b}",
        "\u{2028}", "\u{feff}", "\u{a0}", "\u{85}", "\u{3000}", "<日", "&日", "<br日", "<div>",
        "</div>",
    ];

    fn clean_trim(x: &str) -> String {
        let cleaned: String = x
            .chars()
            .filter(|c| (*c == '\n' || !c.is_control()) && !is_format_char(*c))
            .collect();
        cleaned.trim().to_string()
    }

    fn strip_edges(html: &str) -> String {
        let mut s = html.trim().to_string();
        loop {
            let before = s.clone();
            for br in ["<br>", "&nbsp;"] {
                if let Some(r) = s.strip_prefix(br) {
                    s = r.trim_start().to_string();
                }
                if let Some(r) = s.strip_suffix(br) {
                    s = r.trim_end().to_string();
                }
            }
            s = s.trim_matches(|c: char| c.is_whitespace()).to_string();
            if s == before {
                return s;
            }
        }
    }

    #[test]
    fn probe_fuzz_editable_round_trip_and_render_equivalence() {
        let mut rng = Lcg(0x5eed);
        let mut some = 0;
        for _ in 0..60_000 {
            let n = rng.next() % 10;
            let s: String = (0..n).map(|_| TOKENS[rng.next() % TOKENS.len()]).collect();
            for fmt in [None, Some("html"), Some("HTML")] {
                if let Some(p) = editable_comment_text(&s, fmt) {
                    some += 1;
                    let re = comment_html(&p).expect("Some implies comment_html accepts");
                    assert_eq!(
                        editable_comment_text(&re, None).as_deref(),
                        Some(p.trim()),
                        "idempotence {s:?}"
                    );
                    let a = crate::rich_text::sanitize_html(&s, "org");
                    let b = crate::rich_text::sanitize_html(&re, "org");
                    assert_eq!(
                        strip_edges(&a),
                        strip_edges(&b),
                        "render differs for stored {s:?}"
                    );
                }
            }
            // Every accepted new comment is editable back to its cleaned, trimmed text.
            if let Ok(html) = comment_html(&s) {
                assert_eq!(
                    editable_comment_text(&html, None).as_deref(),
                    Some(clean_trim(&s).as_str()),
                    "round trip {s:?}"
                );
            }
        }
        assert!(some > 200, "fuzz produced too few editable cases: {some}");
    }

    #[test]
    fn probe_specific_inputs() {
        for stored in [
            "<brx>",
            "a<br",
            "a<br/ >b",
            "a&#39b",
            "a&AMP;b",
            "<日本",
            "&日",
            "a\tb",
            "a\rb",
            "a\u{200b}b",
            "a&nbsp;",
        ] {
            assert_eq!(editable_comment_text(stored, None), None, "{stored:?}");
        }
        assert_eq!(editable_comment_text("plain", Some("")), None);
        assert_eq!(
            editable_comment_text("a > b \" c ' d", None).as_deref(),
            Some("a > b \" c ' d")
        );
        // A resave trims, so text with leading or trailing whitespace is not offered.
        for stored in ["<br>hi", "hi<br>", "\u{a0}hi", "hi ", " hi"] {
            assert_eq!(editable_comment_text(stored, None), None, "{stored:?}");
        }
    }
}
