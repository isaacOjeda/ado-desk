//! Work item detail (fields, rich text, relations) and its discussion comments.
//! Read-only. Mapping is pure and unit tested; HTTP lives in the commands below.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::State;
use url::Url;

use crate::AppState;
use crate::azdo::AzdoClient;
use crate::azdo::model::{self, ListResponse, RawWorkItem};
use crate::commands::current_client;
use crate::error::AppResult;
use crate::rich_text;

// ---------------------------------------------------------------------------
// Domain types (sent to the frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemDetail {
    pub id: i64,
    /// Revision loaded; writes test it so a change made elsewhere is not overwritten.
    pub rev: i64,
    pub title: String,
    pub work_item_type: String,
    pub state: String,
    pub reason: Option<String>,
    pub project: String,
    pub url: String,
    pub assigned_to: Option<String>,
    pub created_by: Option<String>,
    pub created_at: Option<String>,
    pub changed_by: Option<String>,
    pub changed_at: Option<String>,
    pub priority: Option<i64>,
    pub severity: Option<String>,
    pub area_path: Option<String>,
    pub iteration_path: Option<String>,
    pub tags: Vec<String>,
    pub story_points: Option<f64>,
    pub effort: Option<f64>,
    pub original_estimate: Option<f64>,
    pub remaining_work: Option<f64>,
    pub completed_work: Option<f64>,
    pub comment_count: Option<i64>,
    /// Sanitized HTML (see `rich_text::sanitize_html`); `None` when empty.
    pub description_html: Option<String>,
    pub repro_steps_html: Option<String>,
    pub system_info_html: Option<String>,
    pub acceptance_criteria_html: Option<String>,
    pub parent: Option<LinkedWorkItem>,
    pub children: Vec<LinkedWorkItem>,
    pub related: Vec<LinkedWorkItem>,
    pub development: Vec<DevelopmentLink>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LinkedWorkItem {
    pub id: i64,
    /// Link type name as shown in the web, e.g. "Related", "Successor".
    pub link: String,
    pub title: String,
    pub work_item_type: String,
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DevelopmentKind {
    PullRequest,
    Commit,
    Branch,
    Build,
    Hyperlink,
    Other,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentLink {
    pub kind: DevelopmentKind,
    pub label: String,
    /// Link type name, e.g. "Fixed in Commit".
    pub link: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemComment {
    pub id: i64,
    pub author: String,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
    pub is_edited: bool,
    /// Sanitized HTML.
    pub html: String,
}

// ---------------------------------------------------------------------------
// Raw Azure DevOps payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawWorkItemDetail {
    pub id: i64,
    #[serde(default)]
    pub rev: i64,
    #[serde(default)]
    pub fields: Map<String, Value>,
    #[serde(default)]
    pub relations: Vec<RawRelation>,
    /// Present when some long text fields are stored as Markdown.
    #[serde(default)]
    pub multiline_fields_format: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct RawRelation {
    pub rel: String,
    pub url: String,
    #[serde(default)]
    pub attributes: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCommentList {
    #[serde(default)]
    pub comments: Vec<RawComment>,
    #[serde(default)]
    pub continuation_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawComment {
    // The docs disagree on the key (`id` in the schema, `commentId` in samples).
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub comment_id: Option<i64>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub rendered_text: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub created_by: Option<Value>,
    #[serde(default)]
    pub created_date: Option<String>,
    #[serde(default)]
    pub modified_date: Option<String>,
    #[serde(default)]
    pub version: Option<i64>,
    #[serde(default)]
    pub is_deleted: bool,
}

// ---------------------------------------------------------------------------
// Pure mapping
// ---------------------------------------------------------------------------

const HIERARCHY_REVERSE: &str = "System.LinkTypes.Hierarchy-Reverse";
const HIERARCHY_FORWARD: &str = "System.LinkTypes.Hierarchy-Forward";

/// Display name of an identity field: an object with `displayName`, or the
/// legacy `"Name <user@domain>"` string form.
pub fn identity_name(value: &Value) -> Option<String> {
    let name = match value {
        Value::Object(map) => ["displayName", "uniqueName"]
            .iter()
            .find_map(|key| map.get(*key).and_then(Value::as_str).filter(|s| !s.trim().is_empty()))?
            .to_string(),
        Value::String(text) => text.split('<').next().unwrap_or_default().to_string(),
        _ => return None,
    };
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn text_field(fields: &Map<String, Value>, name: &str) -> Option<String> {
    fields
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn number_field(fields: &Map<String, Value>, name: &str) -> Option<f64> {
    fields.get(name).and_then(Value::as_f64)
}

fn integer_field(fields: &Map<String, Value>, name: &str) -> Option<i64> {
    fields
        .get(name)
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
}

/// Sanitized HTML of a long text field, rendering Markdown when the item says so.
fn rich_field(
    organization: &str,
    fields: &Map<String, Value>,
    formats: &Map<String, Value>,
    name: &str,
) -> Option<String> {
    let raw = text_field(fields, name)?;
    let is_markdown = formats
        .get(name)
        .and_then(Value::as_str)
        .is_some_and(|f| f.eq_ignore_ascii_case("markdown"));
    let html = if is_markdown {
        rich_text::markdown_to_html(&raw, organization)
    } else {
        rich_text::sanitize_html(&raw, organization)
    };
    (!html.trim().is_empty()).then_some(html)
}

/// Id of a work item link (`https://dev.azure.com/{org}/_apis/wit/workItems/{id}`).
/// Remote links (other organizations) and non work item URLs yield `None`.
fn work_item_link_id(relation: &RawRelation) -> Option<i64> {
    // Work link types are not all `System.LinkTypes.*` (e.g. `Microsoft.VSTS.Common.TestedBy-Forward`),
    // so exclude the known non work item relations and let the URL decide.
    if matches!(relation.rel.as_str(), "ArtifactLink" | "Hyperlink" | "AttachedFile")
        || relation.rel.contains(".Remote.")
    {
        return None;
    }
    let url = Url::parse(&relation.url).ok()?;
    let segments: Vec<&str> = url.path_segments()?.collect();
    let [.., kind, id] = segments.as_slice() else {
        return None;
    };
    if !kind.eq_ignore_ascii_case("workItems") {
        return None;
    }
    id.parse().ok()
}

/// Ids of the work items linked from `relations` (parent, children, related), deduplicated.
pub fn linked_ids(relations: &[RawRelation]) -> Vec<i64> {
    let mut ids: Vec<i64> = Vec::new();
    for id in relations.iter().filter_map(work_item_link_id) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

fn attribute(relation: &RawRelation, key: &str) -> Option<String> {
    relation
        .attributes
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn linked_work_item(organization: &str, id: i64, link: String, linked: &[RawWorkItem]) -> LinkedWorkItem {
    match linked.iter().find(|item| item.id == id) {
        Some(raw) => {
            // Reuse the list mapping so titles, states and web URLs stay consistent.
            let item = model::map_work_item(
                organization,
                RawWorkItem {
                    id: raw.id,
                    fields: raw.fields.clone(),
                },
            );
            LinkedWorkItem {
                id,
                link,
                title: item.title,
                work_item_type: item.work_item_type,
                state: item.state,
                url: item.url,
            }
        }
        None => LinkedWorkItem {
            id,
            link,
            title: format!("Work item {id}"),
            work_item_type: String::new(),
            state: String::new(),
            url: model::build_url(&[organization, "_workitems", "edit", &id.to_string()]).to_string(),
        },
    }
}

/// Decodes `%XX` escapes; invalid escapes are kept verbatim.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = input
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Maps an `ArtifactLink` (`vstfs:///{tool}/{type}/{id}`) to a readable label and,
/// when the artifact is understood, its web URL. Unknown artifacts keep only the label.
fn development_link(organization: &str, project: &str, relation: &RawRelation) -> DevelopmentLink {
    let link = attribute(relation, "name").unwrap_or_else(|| "Link".into());
    let other = DevelopmentLink {
        kind: DevelopmentKind::Other,
        label: link.clone(),
        link: link.clone(),
        url: None,
    };

    let Some((tool, rest)) = relation
        .url
        .strip_prefix("vstfs:///")
        .and_then(|rest| rest.split_once('/'))
    else {
        return other;
    };
    let Some((artifact, id)) = rest.split_once('/') else {
        return other;
    };
    let id = percent_decode(id);
    // Git artifact ids are `{projectId}/{repositoryId}/{value}`.
    let git_parts = || {
        let mut parts = id.splitn(3, '/');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(p), Some(r), Some(v)) if !p.is_empty() && !r.is_empty() && !v.is_empty() => Some((p, r, v)),
            _ => None,
        }
    };

    let (kind, label, url) = match (tool.to_ascii_lowercase().as_str(), artifact.to_ascii_lowercase().as_str()) {
        ("git", "pullrequestid") => {
            let Some((p, r, pr)) = git_parts() else { return other };
            let url = model::build_url(&[organization, p, "_git", r, "pullrequest", pr]);
            (DevelopmentKind::PullRequest, format!("Pull request !{pr}"), url)
        }
        ("git", "commit") => {
            let Some((p, r, sha)) = git_parts() else { return other };
            let url = model::build_url(&[organization, p, "_git", r, "commit", sha]);
            let short: String = sha.chars().take(7).collect();
            (DevelopmentKind::Commit, format!("Commit {short}"), url)
        }
        ("git", "ref") => {
            let Some((p, r, reference)) = git_parts() else { return other };
            let mut url = model::build_url(&[organization, p, "_git", r]);
            url.query_pairs_mut().append_pair("version", reference);
            let name = reference.strip_prefix("GB").unwrap_or(reference);
            (DevelopmentKind::Branch, format!("Branch {name}"), url)
        }
        ("build", "build") if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) && !project.is_empty() => {
            let mut url = model::build_url(&[organization, project, "_build", "results"]);
            url.query_pairs_mut().append_pair("buildId", &id);
            (DevelopmentKind::Build, format!("Build {id}"), url)
        }
        _ => return other,
    };

    DevelopmentLink {
        kind,
        label,
        link,
        url: Some(url.to_string()),
    }
}

/// Web hyperlinks only; other schemes (`javascript:`, `file:`, UNC paths) are dropped.
fn hyperlink(relation: &RawRelation) -> Option<DevelopmentLink> {
    let url = Url::parse(&relation.url).ok()?;
    if !matches!(url.scheme(), "https" | "http") {
        return None;
    }
    Some(DevelopmentLink {
        kind: DevelopmentKind::Hyperlink,
        label: attribute(relation, "comment").unwrap_or_else(|| url.to_string()),
        link: "Hyperlink".into(),
        url: Some(url.to_string()),
    })
}

/// Maps the work item, resolving linked work items against `linked` (the batch
/// lookup result; missing entries become placeholders).
pub fn map_detail(organization: &str, raw: RawWorkItemDetail, linked: &[RawWorkItem]) -> WorkItemDetail {
    let fields = &raw.fields;
    let formats = &raw.multiline_fields_format;
    let text = |name: &str| text_field(fields, name);
    let identity = |name: &str| fields.get(name).and_then(identity_name);
    let rich = |name: &str| rich_field(organization, fields, formats, name);

    let project = text("System.TeamProject").unwrap_or_default();
    let url = model::build_url(&[organization, &project, "_workitems", "edit", &raw.id.to_string()]);

    let mut parent = None;
    let mut children = Vec::new();
    let mut related = Vec::new();
    let mut development = Vec::new();
    let mut seen: Vec<(&str, i64)> = Vec::new();

    for relation in &raw.relations {
        if let Some(id) = work_item_link_id(relation) {
            let key = (relation.rel.as_str(), id);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let link = attribute(relation, "name").unwrap_or_else(|| "Related".into());
            let item = linked_work_item(organization, id, link, linked);
            match relation.rel.as_str() {
                HIERARCHY_REVERSE => parent = parent.or(Some(item)),
                HIERARCHY_FORWARD => children.push(item),
                _ => related.push(item),
            }
        } else if relation.rel == "ArtifactLink" {
            development.push(development_link(organization, &project, relation));
        } else if relation.rel == "Hyperlink" {
            development.extend(hyperlink(relation));
        }
    }

    WorkItemDetail {
        id: raw.id,
        rev: raw.rev,
        title: text("System.Title").unwrap_or_default(),
        work_item_type: text("System.WorkItemType").unwrap_or_default(),
        state: text("System.State").unwrap_or_default(),
        reason: text("System.Reason"),
        url: url.to_string(),
        assigned_to: identity("System.AssignedTo"),
        created_by: identity("System.CreatedBy"),
        created_at: text("System.CreatedDate"),
        changed_by: identity("System.ChangedBy"),
        changed_at: text("System.ChangedDate"),
        priority: integer_field(fields, "Microsoft.VSTS.Common.Priority"),
        severity: text("Microsoft.VSTS.Common.Severity"),
        area_path: text("System.AreaPath"),
        iteration_path: text("System.IterationPath"),
        tags: text("System.Tags")
            .map(|tags| {
                tags.split(';')
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        story_points: number_field(fields, "Microsoft.VSTS.Scheduling.StoryPoints"),
        effort: number_field(fields, "Microsoft.VSTS.Scheduling.Effort"),
        original_estimate: number_field(fields, "Microsoft.VSTS.Scheduling.OriginalEstimate"),
        remaining_work: number_field(fields, "Microsoft.VSTS.Scheduling.RemainingWork"),
        completed_work: number_field(fields, "Microsoft.VSTS.Scheduling.CompletedWork"),
        comment_count: integer_field(fields, "System.CommentCount"),
        description_html: rich("System.Description"),
        repro_steps_html: rich("Microsoft.VSTS.TCM.ReproSteps"),
        system_info_html: rich("Microsoft.VSTS.TCM.SystemInfo"),
        acceptance_criteria_html: rich("Microsoft.VSTS.Common.AcceptanceCriteria"),
        project,
        parent,
        children,
        related,
        development,
    }
}

fn map_comment(organization: &str, raw: RawComment) -> WorkItemComment {
    let rendered = raw.rendered_text.filter(|t| !t.trim().is_empty());
    let text = raw.text.unwrap_or_default();
    let is_markdown = raw
        .format
        .as_deref()
        .is_some_and(|f| f.eq_ignore_ascii_case("markdown"));
    let html = match rendered {
        Some(html) => rich_text::sanitize_html(&html, organization),
        None if is_markdown => rich_text::markdown_to_html(&text, organization),
        None => rich_text::sanitize_html(&text, organization),
    };
    let is_edited = match raw.version {
        Some(version) => version > 1,
        None => matches!((&raw.created_date, &raw.modified_date), (Some(c), Some(m)) if c != m),
    };

    WorkItemComment {
        id: raw.id.or(raw.comment_id).unwrap_or_default(),
        author: raw
            .created_by
            .as_ref()
            .and_then(identity_name)
            .unwrap_or_else(|| "Unknown user".into()),
        created_at: raw.created_date,
        modified_at: raw.modified_date,
        is_edited,
        html,
    }
}

/// Drops deleted comments, sanitizes their HTML and sorts newest first.
pub fn map_comments(organization: &str, raw: Vec<RawComment>) -> Vec<WorkItemComment> {
    let mut comments: Vec<WorkItemComment> = raw
        .into_iter()
        .filter(|c| !c.is_deleted)
        .map(|c| map_comment(organization, c))
        .collect();
    comments.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    comments
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// The batch endpoint accepts at most 200 ids.
const MAX_LINKED: usize = 200;
const COMMENTS_PAGE_SIZE: &str = "200";
const MAX_COMMENT_PAGES: usize = 10;
const LINKED_FIELDS: &str = "System.Title,System.WorkItemType,System.State,System.TeamProject";

async fn fetch_linked(client: &AzdoClient, ids: &[i64]) -> AppResult<Vec<RawWorkItem>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = ids.iter().take(MAX_LINKED).map(i64::to_string).collect();
    let mut url = client.url(&["_apis", "wit", "workitems"]);
    url.query_pairs_mut()
        .append_pair("ids", &ids.join(","))
        .append_pair("fields", LINKED_FIELDS)
        .append_pair("errorPolicy", "Omit");
    let list: ListResponse<Option<RawWorkItem>> = client.get_json(url).await?;
    Ok(list.value.into_iter().flatten().collect())
}

#[tauri::command]
pub async fn get_work_item(state: State<'_, AppState>, id: i64) -> AppResult<WorkItemDetail> {
    let client = current_client(&state)?;

    let mut url = client.url(&["_apis", "wit", "workitems", &id.to_string()]);
    url.query_pairs_mut().append_pair("$expand", "relations");
    let raw: RawWorkItemDetail = client.get_json(url).await?;

    // Linked titles are a nicety: if the lookup fails, rows fall back to placeholders.
    let linked = fetch_linked(&client, &linked_ids(&raw.relations))
        .await
        .unwrap_or_default();

    Ok(map_detail(client.org(), raw, &linked))
}

#[tauri::command]
pub async fn list_work_item_comments(
    state: State<'_, AppState>,
    project: String,
    id: i64,
) -> AppResult<Vec<WorkItemComment>> {
    let client = current_client(&state)?;
    let id = id.to_string();
    let mut comments = Vec::new();
    let mut continuation: Option<String> = None;

    for _ in 0..MAX_COMMENT_PAGES {
        let mut url = client.url_with_version(
            &[&project, "_apis", "wit", "workItems", &id, "comments"],
            "7.1-preview.4",
        );
        url.query_pairs_mut()
            .append_pair("$top", COMMENTS_PAGE_SIZE)
            .append_pair("$expand", "renderedText")
            .append_pair("order", "desc");
        if let Some(token) = &continuation {
            url.query_pairs_mut().append_pair("continuationToken", token);
        }

        let page: RawCommentList = client.get_json(url).await?;
        comments.extend(page.comments);
        continuation = page.continuation_token.filter(|t| !t.is_empty());
        if continuation.is_none() {
            break;
        }
    }

    Ok(map_comments(client.org(), comments))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ORG: &str = "contoso";

    fn raw_detail(value: Value) -> RawWorkItemDetail {
        serde_json::from_value(value).unwrap()
    }

    fn linked(id: i64, title: &str, kind: &str, state: &str) -> RawWorkItem {
        serde_json::from_value(json!({
            "id": id,
            "fields": {
                "System.Title": title,
                "System.WorkItemType": kind,
                "System.State": state,
                "System.TeamProject": "Core"
            }
        }))
        .unwrap()
    }

    fn relation(rel: &str, url: &str, name: &str) -> Value {
        json!({ "rel": rel, "url": url, "attributes": { "name": name } })
    }

    fn wi_url(id: i64) -> String {
        format!("https://dev.azure.com/{ORG}/_apis/wit/workItems/{id}")
    }

    #[test]
    fn identity_name_accepts_objects_and_legacy_strings() {
        assert_eq!(
            identity_name(&json!({ "displayName": "Ada Lovelace", "uniqueName": "ada@x.com" })),
            Some("Ada Lovelace".into())
        );
        assert_eq!(identity_name(&json!("Grace Hopper <grace@x.com>")), Some("Grace Hopper".into()));
        assert_eq!(identity_name(&json!("Plain Name")), Some("Plain Name".into()));
        assert_eq!(identity_name(&json!({ "uniqueName": "only@x.com" })), Some("only@x.com".into()));
        assert_eq!(identity_name(&json!("")), None);
        assert_eq!(identity_name(&Value::Null), None);
    }

    #[test]
    fn maps_scalar_fields_identities_and_tags() {
        let raw = raw_detail(json!({
            "id": 42,
            "rev": 9,
            "fields": {
                "System.Title": "Login fails",
                "System.WorkItemType": "Bug",
                "System.State": "Active",
                "System.Reason": "Approved",
                "System.TeamProject": "My Project",
                "System.AreaPath": "My Project\\Web",
                "System.IterationPath": "My Project\\Sprint 4",
                "System.AssignedTo": { "displayName": "Ada Lovelace", "uniqueName": "ada@x.com" },
                "System.CreatedBy": { "displayName": "Grace Hopper" },
                "System.CreatedDate": "2026-10-01T10:00:00Z",
                "System.ChangedBy": "Linus T <linus@x.com>",
                "System.ChangedDate": "2026-11-01T10:00:00Z",
                "System.Tags": "frontend; urgent ;  ",
                "System.CommentCount": 3,
                "Microsoft.VSTS.Common.Priority": 2,
                "Microsoft.VSTS.Common.Severity": "2 - High",
                "Microsoft.VSTS.Scheduling.StoryPoints": 5.5,
                "Microsoft.VSTS.Scheduling.RemainingWork": 3
            }
        }));

        let item = map_detail(ORG, raw, &[]);
        assert_eq!(item.rev, 9);

        assert_eq!(item.id, 42);
        assert_eq!(item.title, "Login fails");
        assert_eq!(item.work_item_type, "Bug");
        assert_eq!(item.state, "Active");
        assert_eq!(item.reason.as_deref(), Some("Approved"));
        assert_eq!(item.project, "My Project");
        assert_eq!(item.url, "https://dev.azure.com/contoso/My%20Project/_workitems/edit/42");
        assert_eq!(item.area_path.as_deref(), Some("My Project\\Web"));
        assert_eq!(item.iteration_path.as_deref(), Some("My Project\\Sprint 4"));
        assert_eq!(item.assigned_to.as_deref(), Some("Ada Lovelace"));
        assert_eq!(item.created_by.as_deref(), Some("Grace Hopper"));
        assert_eq!(item.created_at.as_deref(), Some("2026-10-01T10:00:00Z"));
        assert_eq!(item.changed_by.as_deref(), Some("Linus T"));
        assert_eq!(item.changed_at.as_deref(), Some("2026-11-01T10:00:00Z"));
        assert_eq!(item.tags, vec!["frontend", "urgent"]);
        assert_eq!(item.comment_count, Some(3));
        assert_eq!(item.priority, Some(2));
        assert_eq!(item.severity.as_deref(), Some("2 - High"));
        assert_eq!(item.story_points, Some(5.5));
        assert_eq!(item.remaining_work, Some(3.0));
        assert_eq!(item.effort, None);
        assert_eq!(item.description_html, None);
        assert!(item.parent.is_none() && item.children.is_empty() && item.development.is_empty());
    }

    #[test]
    fn sanitizes_every_rich_text_field_and_drops_empty_ones() {
        let raw = raw_detail(json!({
            "id": 7,
            "fields": {
                "System.TeamProject": "Core",
                "System.Description": "<p onclick=\"x()\">Hello<script>alert(1)</script></p>",
                "Microsoft.VSTS.TCM.ReproSteps": "<ol><li>Open</li></ol><img src=\"https://dev.azure.com/contoso/_apis/wit/attachments/abc?fileName=a.png\">",
                "Microsoft.VSTS.TCM.SystemInfo": "<iframe src=\"https://evil.example\"></iframe>macOS",
                "Microsoft.VSTS.Common.AcceptanceCriteria": "   "
            }
        }));

        let item = map_detail(ORG, raw, &[]);

        let description = item.description_html.unwrap();
        assert!(description.contains("Hello"));
        assert!(!description.contains("script") && !description.contains("onclick"));
        let repro = item.repro_steps_html.unwrap();
        assert!(repro.contains("<li>Open</li>"));
        assert!(repro.contains("localhost/contoso/_apis/wit/attachments/abc"), "image proxied: {repro}");
        assert!(!repro.contains("src=\"https://dev.azure.com"));
        let system_info = item.system_info_html.unwrap();
        assert!(!system_info.contains("iframe") && system_info.contains("macOS"));
        assert_eq!(item.acceptance_criteria_html, None);
    }

    #[test]
    fn renders_markdown_multiline_fields() {
        let raw = raw_detail(json!({
            "id": 8,
            "fields": { "System.TeamProject": "Core", "System.Description": "**Bold** <script>x</script>" },
            "multilineFieldsFormat": { "System.Description": "markdown" }
        }));

        let description = map_detail(ORG, raw, &[]).description_html.unwrap();

        assert!(description.contains("<strong>Bold</strong>"));
        assert!(!description.contains("<script"));
    }

    #[test]
    fn linked_ids_collects_work_item_links_only() {
        let raw = raw_detail(json!({
            "id": 1,
            "relations": [
                relation("System.LinkTypes.Hierarchy-Reverse", &wi_url(10), "Parent"),
                relation("System.LinkTypes.Hierarchy-Forward", &wi_url(11), "Child"),
                relation("System.LinkTypes.Hierarchy-Forward", &wi_url(11), "Child"),
                relation("System.LinkTypes.Related", &wi_url(12), "Related"),
                relation("ArtifactLink", "vstfs:///Git/Commit/p%2Fr%2Fabc", "Fixed in Commit"),
                relation("AttachedFile", "https://dev.azure.com/contoso/_apis/wit/attachments/x", "AttachedFile"),
                relation("System.LinkTypes.Remote.Related", "https://dev.azure.com/other/_apis/wit/workItems/99", "Remote Related")
            ]
        }));

        assert_eq!(linked_ids(&raw.relations), vec![10, 11, 12]);
    }

    #[test]
    fn groups_relations_into_parent_children_related_and_development() {
        let raw = raw_detail(json!({
            "id": 1,
            "fields": { "System.TeamProject": "Core" },
            "relations": [
                relation("System.LinkTypes.Hierarchy-Reverse", &wi_url(10), "Parent"),
                relation("System.LinkTypes.Hierarchy-Forward", &wi_url(11), "Child"),
                relation("System.LinkTypes.Hierarchy-Forward", &wi_url(13), "Child"),
                relation("System.LinkTypes.Related", &wi_url(12), "Related"),
                relation("System.LinkTypes.Dependency-Forward", &wi_url(14), "Successor"),
                relation("Microsoft.VSTS.Common.TestedBy-Forward", &wi_url(15), "Tested By"),
                relation("ArtifactLink", "vstfs:///Git/PullRequestId/proj-guid%2Frepo-guid%2F57", "Pull Request"),
                relation("ArtifactLink", "vstfs:///Git/Commit/proj-guid%2Frepo-guid%2Fa1b2c3d4e5f6", "Fixed in Commit"),
                relation("ArtifactLink", "vstfs:///Git/Ref/proj-guid%2Frepo-guid%2FGBfeature%2Flogin", "Branch"),
                relation("ArtifactLink", "vstfs:///Build/Build/321", "Found in build"),
                relation("ArtifactLink", "vstfs:///Wiki/WikiPage/whatever", "Wiki Page"),
                json!({ "rel": "Hyperlink", "url": "https://example.com/spec", "attributes": { "comment": "Spec" } }),
                json!({ "rel": "Hyperlink", "url": "javascript:alert(1)", "attributes": {} }),
                relation("AttachedFile", "https://dev.azure.com/contoso/_apis/wit/attachments/x", "AttachedFile")
            ]
        }));
        let batch = vec![
            linked(10, "Epic thing", "Epic", "Active"),
            linked(11, "Child one", "Task", "Done"),
            linked(12, "Sibling", "Bug", "New"),
            linked(14, "Next", "Task", "New"),
        ];

        let item = map_detail(ORG, raw, &batch);

        let parent = item.parent.unwrap();
        assert_eq!((parent.id, parent.title.as_str(), parent.work_item_type.as_str()), (10, "Epic thing", "Epic"));
        assert_eq!(parent.url, "https://dev.azure.com/contoso/Core/_workitems/edit/10");
        assert_eq!(item.children.iter().map(|c| c.id).collect::<Vec<_>>(), vec![11, 13]);
        assert_eq!(item.children[0].state, "Done");
        // Omitted by `errorPolicy=Omit` (deleted or no access): placeholder with an org-level URL.
        assert_eq!(item.children[1].title, "Work item 13");
        assert_eq!(item.children[1].url, "https://dev.azure.com/contoso/_workitems/edit/13");
        assert_eq!(
            item.related.iter().map(|r| (r.id, r.link.as_str())).collect::<Vec<_>>(),
            vec![(12, "Related"), (14, "Successor"), (15, "Tested By")]
        );

        let dev = &item.development;
        assert_eq!(dev.len(), 6, "{dev:#?}");
        assert_eq!(dev[0].kind, DevelopmentKind::PullRequest);
        assert_eq!(dev[0].label, "Pull request !57");
        assert_eq!(
            dev[0].url.as_deref(),
            Some("https://dev.azure.com/contoso/proj-guid/_git/repo-guid/pullrequest/57")
        );
        assert_eq!(dev[1].kind, DevelopmentKind::Commit);
        assert_eq!(dev[1].label, "Commit a1b2c3d");
        assert_eq!(dev[1].link, "Fixed in Commit");
        assert_eq!(
            dev[1].url.as_deref(),
            Some("https://dev.azure.com/contoso/proj-guid/_git/repo-guid/commit/a1b2c3d4e5f6")
        );
        assert_eq!(dev[2].kind, DevelopmentKind::Branch);
        assert_eq!(dev[2].label, "Branch feature/login");
        assert_eq!(
            dev[2].url.as_deref(),
            Some("https://dev.azure.com/contoso/proj-guid/_git/repo-guid?version=GBfeature%2Flogin")
        );
        assert_eq!(dev[3].kind, DevelopmentKind::Build);
        assert_eq!(dev[3].label, "Build 321");
        assert_eq!(
            dev[3].url.as_deref(),
            Some("https://dev.azure.com/contoso/Core/_build/results?buildId=321")
        );
        assert_eq!(dev[4].kind, DevelopmentKind::Other);
        assert_eq!(dev[4].label, "Wiki Page");
        assert_eq!(dev[4].url, None);
        assert_eq!(dev[5].kind, DevelopmentKind::Hyperlink);
        assert_eq!(dev[5].label, "Spec");
        assert_eq!(dev[5].url.as_deref(), Some("https://example.com/spec"));
    }

    #[test]
    fn maps_comments_skipping_deleted_marking_edits_newest_first() {
        let raw: RawCommentList = serde_json::from_value(json!({
            "totalCount": 4,
            "count": 4,
            "comments": [
                {
                    "commentId": 1, "version": 1, "text": "<p>First<script>x</script></p>",
                    "createdBy": { "displayName": "Ada" },
                    "createdDate": "2026-10-01T10:00:00Z", "modifiedDate": "2026-10-01T10:00:00Z"
                },
                {
                    "id": 2, "version": 3, "text": "<p>Second</p>",
                    "createdBy": { "displayName": "Grace" },
                    "createdDate": "2026-10-03T10:00:00Z", "modifiedDate": "2026-10-04T10:00:00Z"
                },
                {
                    "id": 3, "text": "gone", "isDeleted": true,
                    "createdBy": { "displayName": "Linus" }, "createdDate": "2026-10-05T10:00:00Z"
                },
                {
                    "id": 4, "version": 1, "format": "markdown", "text": "**md** text",
                    "createdBy": { "displayName": "Ken" }, "createdDate": "2026-10-02T10:00:00Z"
                },
                {
                    "id": 5, "format": "markdown", "text": "raw md",
                    "renderedText": "<p>rendered <b>html</b><img src=x onerror=alert(1)></p>",
                    "createdDate": "2026-09-01T10:00:00Z", "modifiedDate": "2026-09-02T10:00:00Z"
                }
            ],
            "continuationToken": "next"
        }))
        .unwrap();
        assert_eq!(raw.continuation_token.as_deref(), Some("next"));

        let comments = map_comments(ORG, raw.comments);

        assert_eq!(comments.iter().map(|c| c.id).collect::<Vec<_>>(), vec![2, 4, 1, 5]);
        assert_eq!(comments[0].author, "Grace");
        assert!(comments[0].is_edited);
        assert_eq!(comments[0].modified_at.as_deref(), Some("2026-10-04T10:00:00Z"));
        assert!(comments[1].html.contains("<strong>md</strong>"));
        assert!(!comments[1].is_edited);
        assert!(comments[2].html.contains("First") && !comments[2].html.contains("script"));
        assert!(!comments[2].is_edited);
        // No version: edited when the dates differ. Rendered HTML wins over raw Markdown.
        assert!(comments[3].is_edited);
        assert_eq!(comments[3].author, "Unknown user");
        assert!(comments[3].html.contains("<b>html</b>"));
        assert!(!comments[3].html.contains("onerror"));
    }
}
