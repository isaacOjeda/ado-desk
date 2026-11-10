//! Filtered work item and pull request lists, plus the lookups that feed the filter bar
//! (projects, iterations, work item types and team members) for the active organization.
//!
//! Filters arrive typed from the webview. WIQL and PR search criteria are built by pure
//! functions at the top of this module so every rule is unit tested; every user-provided
//! value goes through [`escape_literal`] before it reaches a WIQL string.

use futures::{FutureExt, StreamExt, future::BoxFuture, stream};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use tauri::State;

use crate::AppState;
use crate::azdo::AzdoClient;
use crate::azdo::model::{self, ListResponse, PullRequest, WorkItem};
use crate::commands::current_client;
use crate::error::{AppError, AppResult};

/// Longest value accepted inside a WIQL string literal.
pub const MAX_LITERAL_LEN: usize = 256;
/// Azure DevOps project names are at most 64 characters.
const MAX_PROJECT_LEN: usize = 64;
const MAX_TYPES: usize = 50;
/// Characters Azure DevOps forbids in project names.
const PROJECT_FORBIDDEN: &[char] = &[
    '\\', '/', ':', '*', '?', '"', '<', '>', '|', ';', '#', '$', '{', '}', ',', '+', '=', '[', ']',
];
const CLOSED_STATES: &str = "('Closed', 'Done', 'Removed')";
const ORDER_BY: &str = "ORDER BY [System.ChangedDate] DESC";
/// Per project and per role query; bounds completed/abandoned histories.
const PR_PAGE_SIZE: &str = "100";
const TEAM_MEMBERS_PAGE: &str = "500";

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors of this module: an invalid filter, or any error of the Azure DevOps client.
/// Serialized with the same `{ kind, message }` shape as [`AppError`].
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("Invalid filter: {0}")]
    InvalidFilter(String),
    /// Quick search input (fixed text: the query is never echoed).
    #[error("{0}")]
    InvalidSearch(&'static str),
    #[error(transparent)]
    App(#[from] AppError),
}

impl Serialize for QueryError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            QueryError::App(err) => err.serialize(serializer),
            QueryError::InvalidFilter(_) | QueryError::InvalidSearch(_) => {
                let kind = match self {
                    QueryError::InvalidSearch(_) => "invalidSearch",
                    _ => "invalidFilter",
                };
                let mut state = serializer.serialize_struct("QueryError", 2)?;
                state.serialize_field("kind", kind)?;
                state.serialize_field("message", &self.to_string())?;
                state.end()
            }
        }
    }
}

pub type QueryResult<T> = Result<T, QueryError>;

fn invalid(message: impl Into<String>) -> QueryError {
    QueryError::InvalidFilter(message.into())
}

// ---------------------------------------------------------------------------
// Filters (received from the frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkItemFilter {
    /// `None` means every project of the organization.
    pub project: Option<String>,
    pub assignee: Assignee,
    pub iteration: IterationFilter,
    /// Empty means every type.
    pub types: Vec<String>,
    pub state: StateFilter,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Assignee {
    #[default]
    Me,
    Anyone,
    Unassigned,
    Person { unique_name: String },
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum IterationFilter {
    #[default]
    Any,
    /// The current sprint of the selected project's default team.
    Current,
    Path { path: String },
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StateFilter {
    #[default]
    Open,
    Closed,
    All,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PullRequestFilter {
    pub project: Option<String>,
    pub status: PullRequestStatusFilter,
    pub role: PullRequestRoleFilter,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestStatusFilter {
    #[default]
    Active,
    Completed,
    Abandoned,
    All,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestRoleFilter {
    /// Created by or reviewed by the signed-in user.
    #[default]
    Mine,
    Created,
    Reviewing,
    /// Every pull request; only allowed within one project.
    Everyone,
}

// ---------------------------------------------------------------------------
// Lookup results (sent to the frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Iteration {
    pub id: String,
    pub name: String,
    /// Iteration path as stored in `System.IterationPath`, e.g. `Core\Sprint 12`.
    pub path: String,
    pub start_date: Option<String>,
    pub finish_date: Option<String>,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeamMember {
    pub display_name: String,
    pub unique_name: String,
}

// ---------------------------------------------------------------------------
// Raw payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct IterationList {
    // The documented sample says `values`; the live API answers `value`.
    #[serde(alias = "values", default)]
    pub value: Vec<RawIteration>,
}

#[derive(Debug, Deserialize)]
pub struct RawIteration {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub attributes: Option<RawIterationAttributes>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawIterationAttributes {
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub finish_date: Option<String>,
    /// `"past" | "current" | "future"` (or the numeric enum 0/1/2).
    #[serde(default)]
    pub time_frame: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawWorkItemType {
    pub name: String,
    #[serde(default)]
    pub is_disabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawProjectDetail {
    #[serde(default)]
    pub default_team: Option<RawTeamRef>,
}

#[derive(Debug, Deserialize)]
pub struct RawTeamRef {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct RawTeamMember {
    pub identity: RawMemberIdentity,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMemberIdentity {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub unique_name: Option<String>,
    #[serde(default)]
    pub is_container: bool,
}

// ---------------------------------------------------------------------------
// Pure rules
// ---------------------------------------------------------------------------

/// Quotes `value` as a WIQL string literal: single quotes doubled, control characters
/// and values longer than [`MAX_LITERAL_LEN`] rejected.
pub fn escape_literal(value: &str) -> QueryResult<String> {
    if value.chars().any(char::is_control) {
        return Err(invalid("values cannot contain control characters"));
    }
    if value.chars().count() > MAX_LITERAL_LEN {
        return Err(invalid(format!("values are limited to {MAX_LITERAL_LEN} characters")));
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}

/// Like [`escape_literal`] for values that must not be blank.
fn required_literal(value: &str, what: &str) -> QueryResult<String> {
    // Checked before trimming so a trailing newline is rejected, not silently dropped.
    if value.chars().any(char::is_control) {
        return Err(invalid("values cannot contain control characters"));
    }
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid(format!("{what} cannot be empty")));
    }
    escape_literal(value)
}

/// Accepts a project name Azure DevOps could have created; returns it trimmed.
pub fn validate_project(project: &str) -> QueryResult<String> {
    let has_control = project.chars().any(char::is_control);
    let project = project.trim();
    let valid = !has_control
        && !project.is_empty()
        && project.chars().count() <= MAX_PROJECT_LEN
        && !project.starts_with(['_', '.'])
        && !project.chars().any(|c| c.is_control() || PROJECT_FORBIDDEN.contains(&c));
    if valid {
        Ok(project.to_string())
    } else {
        Err(invalid("unknown project"))
    }
}

/// An iteration path must be the project's root iteration or a node below it.
fn validate_iteration_path(path: &str, project: &str) -> QueryResult<String> {
    // Checked before trimming so a trailing newline is rejected, not silently dropped.
    if path.chars().any(char::is_control) {
        return Err(invalid("values cannot contain control characters"));
    }
    let path = path.trim();
    let inside = path.eq_ignore_ascii_case(project)
        || path
            .get(..project.len() + 1)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&format!("{project}\\")));
    if inside {
        Ok(path.to_string())
    } else {
        Err(invalid("the sprint does not belong to the selected project"))
    }
}

/// WIQL for `filter`. `current_iteration` is the resolved path of the current sprint and
/// is required when the filter asks for it. Defaults reproduce the historic "my open work
/// items" query exactly.
pub fn build_wiql(filter: &WorkItemFilter, current_iteration: Option<&str>) -> QueryResult<String> {
    let project = filter.project.as_deref().map(validate_project).transpose()?;
    let mut clauses: Vec<String> = Vec::new();

    match &filter.assignee {
        Assignee::Me => clauses.push("[System.AssignedTo] = @Me".into()),
        Assignee::Anyone => {}
        Assignee::Unassigned => clauses.push("[System.AssignedTo] = ''".into()),
        Assignee::Person { unique_name } => clauses.push(format!(
            "[System.AssignedTo] = {}",
            required_literal(unique_name, "the assignee")?
        )),
    }

    match filter.state {
        StateFilter::Open => clauses.push(format!("[System.State] NOT IN {CLOSED_STATES}")),
        StateFilter::Closed => clauses.push(format!("[System.State] IN {CLOSED_STATES}")),
        StateFilter::All => {}
    }

    if let Some(project) = &project {
        clauses.push(format!("[System.TeamProject] = {}", escape_literal(project)?));
    }

    let iteration = match (&filter.iteration, &project) {
        (IterationFilter::Any, _) => None,
        (_, None) => return Err(invalid("choose a project to filter by sprint")),
        (IterationFilter::Current, Some(_)) => Some(
            current_iteration
                .ok_or_else(|| invalid("the current sprint could not be resolved"))?
                .to_string(),
        ),
        (IterationFilter::Path { path }, Some(project)) => {
            Some(validate_iteration_path(path, project)?)
        }
    };
    if let Some(path) = iteration {
        clauses.push(format!("[System.IterationPath] = {}", escape_literal(&path)?));
    }

    if filter.types.len() > MAX_TYPES {
        return Err(invalid(format!("at most {MAX_TYPES} work item types")));
    }
    if !filter.types.is_empty() {
        let types = filter
            .types
            .iter()
            .map(|t| required_literal(t, "a work item type"))
            .collect::<QueryResult<Vec<_>>>()?;
        clauses.push(format!("[System.WorkItemType] IN ({})", types.join(", ")));
    }

    let mut wiql = String::from("SELECT [System.Id] FROM WorkItems ");
    if !clauses.is_empty() {
        wiql.push_str("WHERE ");
        wiql.push_str(&clauses.join(" AND "));
        wiql.push(' ');
    }
    wiql.push_str(ORDER_BY);
    Ok(wiql)
}

/// One query-string set per Azure DevOps call for each queried project.
pub fn pull_request_searches(
    filter: &PullRequestFilter,
    user_id: &str,
) -> QueryResult<Vec<Vec<(String, String)>>> {
    if let Some(project) = &filter.project {
        validate_project(project)?;
    }
    let status = match filter.status {
        PullRequestStatusFilter::Active => "active",
        PullRequestStatusFilter::Completed => "completed",
        PullRequestStatusFilter::Abandoned => "abandoned",
        PullRequestStatusFilter::All => "all",
    };
    let identities: &[Option<&str>] = match filter.role {
        PullRequestRoleFilter::Mine => {
            &[Some("searchCriteria.creatorId"), Some("searchCriteria.reviewerId")]
        }
        PullRequestRoleFilter::Created => &[Some("searchCriteria.creatorId")],
        PullRequestRoleFilter::Reviewing => &[Some("searchCriteria.reviewerId")],
        PullRequestRoleFilter::Everyone if filter.project.is_none() => {
            return Err(invalid("choose a project to see everyone's pull requests"));
        }
        PullRequestRoleFilter::Everyone => &[None],
    };

    Ok(identities
        .iter()
        .map(|identity| {
            let mut params = vec![("searchCriteria.status".to_string(), status.to_string())];
            if let Some(key) = identity {
                params.push((key.to_string(), user_id.to_string()));
            }
            params.push(("$top".to_string(), PR_PAGE_SIZE.to_string()));
            params
        })
        .collect())
}

fn is_current_time_frame(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(s)) => s.eq_ignore_ascii_case("current"),
        Some(Value::Number(n)) => n.as_i64() == Some(1),
        _ => false,
    }
}

pub fn map_iterations(raw: Vec<RawIteration>) -> Vec<Iteration> {
    raw.into_iter()
        .map(|it| {
            let attributes = it.attributes;
            Iteration {
                path: it.path.unwrap_or_else(|| it.name.clone()),
                id: it.id,
                name: it.name,
                is_current: is_current_time_frame(
                    attributes.as_ref().and_then(|a| a.time_frame.as_ref()),
                ),
                start_date: attributes.as_ref().and_then(|a| a.start_date.clone()),
                finish_date: attributes.and_then(|a| a.finish_date),
            }
        })
        .collect()
}

/// Enabled type names, case-insensitively sorted and deduplicated.
pub fn map_work_item_types(raw: impl IntoIterator<Item = RawWorkItemType>) -> Vec<String> {
    let mut names: Vec<String> = raw
        .into_iter()
        .filter(|t| !t.is_disabled)
        .map(|t| t.name)
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}

/// People only (groups dropped), sorted by display name, one entry per unique name.
pub fn map_team_members(raw: Vec<RawTeamMember>) -> Vec<TeamMember> {
    let mut members: Vec<TeamMember> = raw
        .into_iter()
        .filter(|m| !m.identity.is_container)
        .filter_map(|m| {
            let unique_name = m.identity.unique_name.filter(|u| !u.trim().is_empty())?;
            Some(TeamMember {
                display_name: m.identity.display_name.unwrap_or_else(|| unique_name.clone()),
                unique_name,
            })
        })
        .collect();
    members.sort_by(|a, b| {
        a.unique_name
            .to_lowercase()
            .cmp(&b.unique_name.to_lowercase())
    });
    members.dedup_by(|a, b| a.unique_name.eq_ignore_ascii_case(&b.unique_name));
    members.sort_by_key(|m| m.display_name.to_lowercase());
    members
}

// ---------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------

async fn current_iteration_path(client: &AzdoClient, project: &str) -> QueryResult<Option<String>> {
    let mut url = client.url(&[project, "_apis", "work", "teamsettings", "iterations"]);
    url.query_pairs_mut().append_pair("$timeframe", "current");
    let list: IterationList = client.get_json(url).await?;
    Ok(map_iterations(list.value).into_iter().next().map(|it| it.path))
}

async fn project_work_item_types(client: &AzdoClient, project: &str) -> QueryResult<Vec<String>> {
    let types: ListResponse<RawWorkItemType> = client
        .get_json(client.url(&[project, "_apis", "wit", "workitemtypes"]))
        .await?;
    Ok(map_work_item_types(types.value))
}

/// Runs `per_project` for every project with bounded concurrency; projects answering
/// 404 (feature disabled) contribute nothing.
async fn across_projects<'a, T: Send + 'a>(
    projects: &'a [String],
    per_project: impl Fn(&'a str) -> BoxFuture<'a, QueryResult<Vec<T>>>,
) -> QueryResult<Vec<T>> {
    let requests: Vec<BoxFuture<'a, QueryResult<Vec<T>>>> =
        projects.iter().map(|p| per_project(p.as_str())).collect();
    let results: Vec<QueryResult<Vec<T>>> = stream::iter(requests)
        .buffer_unordered(AzdoClient::PROJECT_CONCURRENCY)
        .collect()
        .await;

    let mut all = Vec::new();
    for result in results {
        match result {
            Ok(batch) => all.extend(batch),
            Err(QueryError::App(AppError::NotFound)) => {}
            Err(err) => return Err(err),
        }
    }
    Ok(all)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn query_work_items(
    state: State<'_, AppState>,
    filter: WorkItemFilter,
) -> QueryResult<Vec<WorkItem>> {
    let client = current_client(&state)?;
    let current = match (&filter.iteration, &filter.project) {
        (IterationFilter::Current, Some(project)) => {
            let project = validate_project(project)?;
            match current_iteration_path(&client, &project).await? {
                Some(path) => Some(path),
                // The default team has no current sprint, so nothing can be in it.
                None => return Ok(Vec::new()),
            }
        }
        _ => None,
    };
    let wiql = build_wiql(&filter, current.as_deref())?;
    Ok(client.work_items_by_wiql(&wiql).await?)
}

#[tauri::command]
pub async fn query_pull_requests(
    state: State<'_, AppState>,
    filter: PullRequestFilter,
) -> QueryResult<Vec<PullRequest>> {
    let client = current_client(&state)?;
    fetch_pull_requests(&client, &filter).await
}

/// Pull requests matching `filter` across its project (or every project), merged so each
/// appears once. Shared by the PR list and the quick search.
pub(crate) async fn fetch_pull_requests(
    client: &AzdoClient,
    filter: &PullRequestFilter,
) -> QueryResult<Vec<PullRequest>> {
    let searches = pull_request_searches(filter, &client.session().user_id)?;
    let projects = match &filter.project {
        Some(project) => vec![validate_project(project)?],
        None => client.projects().await?,
    };

    let searches = &searches;
    let prs = across_projects(&projects, |project| {
        async move {
            let mut prs = Vec::new();
            for params in searches {
                prs.extend(client.search_pull_requests(project, params).await?);
            }
            Ok(prs)
        }
        .boxed()
    })
    .await?;
    Ok(model::merge_pull_requests(prs))
}

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> QueryResult<Vec<String>> {
    let mut projects = current_client(&state)?.projects().await?;
    projects.sort_by_key(|p| p.to_lowercase());
    Ok(projects)
}

#[tauri::command]
pub async fn list_iterations(
    state: State<'_, AppState>,
    project: String,
) -> QueryResult<Vec<Iteration>> {
    let client = current_client(&state)?;
    let project = validate_project(&project)?;
    let list: IterationList = client
        .get_json(client.url(&[&project, "_apis", "work", "teamsettings", "iterations"]))
        .await?;
    Ok(map_iterations(list.value))
}

/// Types of one project, or the union over every project when `project` is `None`.
#[tauri::command]
pub async fn list_work_item_types(
    state: State<'_, AppState>,
    project: Option<String>,
) -> QueryResult<Vec<String>> {
    let client = current_client(&state)?;
    let projects = match project {
        Some(project) => vec![validate_project(&project)?],
        None => client.projects().await?,
    };
    let client = &client;
    let types = across_projects(&projects, |project| {
        project_work_item_types(client, project).boxed()
    })
    .await?;
    Ok(map_work_item_types(
        types.into_iter().map(|name| RawWorkItemType { name, is_disabled: false }),
    ))
}

/// Members of the project's default team.
#[tauri::command]
pub async fn list_team_members(
    state: State<'_, AppState>,
    project: String,
) -> QueryResult<Vec<TeamMember>> {
    let client = current_client(&state)?;
    let project = validate_project(&project)?;
    Ok(default_team_members(&client, &project).await?)
}

/// Members of the default team of `project` (already validated). Also the assignee
/// candidates that work item actions accept.
pub(crate) async fn default_team_members(client: &AzdoClient, project: &str) -> AppResult<Vec<TeamMember>> {
    let detail: RawProjectDetail = client
        .get_json(client.url(&["_apis", "projects", project]))
        .await?;
    let Some(team) = detail.default_team else {
        return Ok(Vec::new());
    };

    let mut url = client.url(&["_apis", "projects", project, "teams", &team.id, "members"]);
    url.query_pairs_mut().append_pair("$top", TEAM_MEMBERS_PAGE);
    let members: ListResponse<RawTeamMember> = client.get_json(url).await?;
    Ok(map_team_members(members.value))
}

#[cfg(test)]
mod tests {
    #[test]
    fn iteration_paths_with_control_characters_are_rejected_not_trimmed() {
        assert!(validate_iteration_path("Portal\\Sprint 1\n", "Portal").is_err());
        assert!(validate_iteration_path("Portal\\Sprint 1", "Portal").is_ok());
    }

    use super::*;
    use serde_json::json;

    const ME: &str = "11111111-aaaa-bbbb-cccc-000000000001";

    fn filter(value: Value) -> WorkItemFilter {
        serde_json::from_value(value).unwrap()
    }

    fn wiql(value: Value) -> String {
        build_wiql(&filter(value), None).unwrap()
    }

    fn rejected(result: QueryResult<impl std::fmt::Debug>) -> bool {
        matches!(result, Err(QueryError::InvalidFilter(_)))
    }

    #[test]
    fn default_filter_reproduces_todays_query() {
        assert_eq!(build_wiql(&WorkItemFilter::default(), None).unwrap(), AzdoClient::MY_OPEN_WORK_ITEMS);
        // The frontend sends every field explicitly.
        assert_eq!(
            wiql(json!({
                "project": null,
                "assignee": { "kind": "me" },
                "iteration": { "kind": "any" },
                "types": [],
                "state": "open"
            })),
            AzdoClient::MY_OPEN_WORK_ITEMS
        );
    }

    #[test]
    fn assignee_clauses() {
        assert_eq!(
            wiql(json!({ "assignee": { "kind": "anyone" } })),
            "SELECT [System.Id] FROM WorkItems WHERE [System.State] NOT IN ('Closed', 'Done', 'Removed') \
             ORDER BY [System.ChangedDate] DESC"
        );
        assert!(wiql(json!({ "assignee": { "kind": "unassigned" } }))
            .contains("WHERE [System.AssignedTo] = '' AND"));
        assert!(wiql(json!({ "assignee": { "kind": "person", "uniqueName": "ada@contoso.com" } }))
            .contains("WHERE [System.AssignedTo] = 'ada@contoso.com' AND"));
    }

    #[test]
    fn state_clauses() {
        assert!(wiql(json!({ "state": "closed" }))
            .contains("[System.State] IN ('Closed', 'Done', 'Removed')"));
        assert_eq!(
            wiql(json!({ "state": "all" })),
            "SELECT [System.Id] FROM WorkItems WHERE [System.AssignedTo] = @Me \
             ORDER BY [System.ChangedDate] DESC"
        );
        assert_eq!(
            wiql(json!({ "state": "all", "assignee": { "kind": "anyone" } })),
            "SELECT [System.Id] FROM WorkItems ORDER BY [System.ChangedDate] DESC"
        );
    }

    #[test]
    fn project_iteration_and_type_clauses_in_order() {
        assert_eq!(
            wiql(json!({
                "project": "Core",
                "iteration": { "kind": "path", "path": "Core\\Release 1\\Sprint 2" },
                "types": ["Bug", "User Story"]
            })),
            "SELECT [System.Id] FROM WorkItems WHERE [System.AssignedTo] = @Me \
             AND [System.State] NOT IN ('Closed', 'Done', 'Removed') \
             AND [System.TeamProject] = 'Core' \
             AND [System.IterationPath] = 'Core\\Release 1\\Sprint 2' \
             AND [System.WorkItemType] IN ('Bug', 'User Story') \
             ORDER BY [System.ChangedDate] DESC"
        );
    }

    #[test]
    fn current_iteration_uses_the_resolved_path() {
        let f = filter(json!({ "project": "Core", "iteration": { "kind": "current" } }));
        assert!(build_wiql(&f, Some("Core\\Sprint 7"))
            .unwrap()
            .contains("AND [System.IterationPath] = 'Core\\Sprint 7' ORDER BY"));
        assert!(rejected(build_wiql(&f, None)));
    }

    #[test]
    fn iterations_require_a_project_and_a_path_inside_it() {
        assert!(rejected(build_wiql(&filter(json!({ "iteration": { "kind": "current" } })), Some("Core"))));
        assert!(rejected(build_wiql(
            &filter(json!({ "iteration": { "kind": "path", "path": "Core\\Sprint 1" } })),
            None
        )));
        assert!(rejected(build_wiql(
            &filter(json!({ "project": "Core", "iteration": { "kind": "path", "path": "Other\\Sprint 1" } })),
            None
        )));
        assert!(rejected(build_wiql(
            &filter(json!({ "project": "Core", "iteration": { "kind": "path", "path": "Corex\\Sprint 1" } })),
            None
        )));
        // The root iteration is the project itself.
        assert!(build_wiql(
            &filter(json!({ "project": "Core", "iteration": { "kind": "path", "path": "core" } })),
            None
        )
        .is_ok());
    }

    #[test]
    fn escape_doubles_single_quotes() {
        assert_eq!(escape_literal("O'Brien").unwrap(), "'O''Brien'");
        assert_eq!(escape_literal("x' OR 1=1 --").unwrap(), "'x'' OR 1=1 --'");
        assert_eq!(escape_literal("Core\\Sprint 1").unwrap(), "'Core\\Sprint 1'");
        assert!(wiql(json!({ "types": ["Bug') OR ('1'='1"] }))
            .contains("IN ('Bug'') OR (''1''=''1')"));
        assert!(wiql(json!({ "assignee": { "kind": "person", "uniqueName": "o'brien@contoso.com" } }))
            .contains("= 'o''brien@contoso.com'"));
    }

    #[test]
    fn escape_rejects_control_characters_and_oversize_values() {
        for value in ["a\nb", "a\rb", "tab\there", "nul\u{0}", "esc\u{1b}", "del\u{7f}"] {
            assert!(rejected(escape_literal(value)), "{value:?}");
        }
        assert!(escape_literal(&"a".repeat(MAX_LITERAL_LEN)).is_ok());
        assert!(rejected(escape_literal(&"a".repeat(MAX_LITERAL_LEN + 1))));
        assert!(rejected(build_wiql(
            &filter(json!({ "types": ["Bug\n"] })),
            None
        )));
        assert!(rejected(build_wiql(
            &filter(json!({ "assignee": { "kind": "person", "uniqueName": "a".repeat(300) } })),
            None
        )));
    }

    #[test]
    fn rejects_empty_person_type_and_too_many_types() {
        assert!(rejected(build_wiql(
            &filter(json!({ "assignee": { "kind": "person", "uniqueName": "  " } })),
            None
        )));
        assert!(rejected(build_wiql(&filter(json!({ "types": [""] })), None)));
        let many: Vec<String> = (0..=MAX_TYPES).map(|i| format!("T{i}")).collect();
        assert!(rejected(build_wiql(&filter(json!({ "types": many })), None)));
    }

    #[test]
    fn validates_project_names() {
        assert_eq!(validate_project(" My Project ").unwrap(), "My Project");
        for bad in ["", "  ", "a/b", "a\\b", "_hidden", ".dot", "x; DROP", "tab\t", &"p".repeat(65)] {
            assert!(rejected(validate_project(bad)), "{bad:?}");
        }
        assert!(rejected(build_wiql(&filter(json!({ "project": "a]b" })), None)));
        assert!(wiql(json!({ "project": "Bob's" })).contains("[System.TeamProject] = 'Bob''s'"));
    }

    fn pr_filter(value: Value) -> PullRequestFilter {
        serde_json::from_value(value).unwrap()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn default_pr_filter_matches_todays_queries() {
        assert_eq!(
            pull_request_searches(&PullRequestFilter::default(), ME).unwrap(),
            vec![
                pairs(&[("searchCriteria.status", "active"), ("searchCriteria.creatorId", ME), ("$top", "100")]),
                pairs(&[("searchCriteria.status", "active"), ("searchCriteria.reviewerId", ME), ("$top", "100")]),
            ]
        );
    }

    #[test]
    fn pr_status_maps_to_search_criteria() {
        for (status, expected) in [
            ("active", "active"),
            ("completed", "completed"),
            ("abandoned", "abandoned"),
            ("all", "all"),
        ] {
            let searches =
                pull_request_searches(&pr_filter(json!({ "status": status, "role": "created" })), ME)
                    .unwrap();
            assert_eq!(
                searches,
                vec![pairs(&[
                    ("searchCriteria.status", expected),
                    ("searchCriteria.creatorId", ME),
                    ("$top", "100")
                ])]
            );
        }
    }

    #[test]
    fn pr_role_maps_to_identity_criteria() {
        assert_eq!(
            pull_request_searches(&pr_filter(json!({ "role": "reviewing" })), ME).unwrap(),
            vec![pairs(&[("searchCriteria.status", "active"), ("searchCriteria.reviewerId", ME), ("$top", "100")])]
        );
        assert_eq!(
            pull_request_searches(
                &pr_filter(json!({ "project": "Core", "role": "everyone", "status": "completed" })),
                ME
            )
            .unwrap(),
            vec![pairs(&[("searchCriteria.status", "completed"), ("$top", "100")])]
        );
    }

    #[test]
    fn everyone_requires_a_project() {
        assert!(rejected(pull_request_searches(&pr_filter(json!({ "role": "everyone" })), ME)));
        assert!(rejected(pull_request_searches(
            &pr_filter(json!({ "project": "a/b", "role": "everyone" })),
            ME
        )));
    }

    #[test]
    fn unknown_filter_values_fail_to_deserialize() {
        assert!(serde_json::from_value::<PullRequestFilter>(json!({ "status": "merged" })).is_err());
        assert!(serde_json::from_value::<WorkItemFilter>(json!({ "assignee": { "kind": "boss" } })).is_err());
    }

    #[test]
    fn invalid_filter_serializes_like_app_errors() {
        let value = serde_json::to_value(invalid("nope")).unwrap();
        assert_eq!(value, json!({ "kind": "invalidFilter", "message": "Invalid filter: nope" }));
        let value = serde_json::to_value(QueryError::from(AppError::NotSignedIn)).unwrap();
        assert_eq!(value["kind"], "notSignedIn");
    }

    #[test]
    fn maps_iterations_with_current_flag() {
        let list: IterationList = serde_json::from_value(json!({
            "count": 2,
            "value": [
                { "id": "a", "name": "Sprint 1", "path": "Core\\Sprint 1",
                  "attributes": { "startDate": "2026-10-01T00:00:00Z", "finishDate": "2026-10-14T00:00:00Z", "timeFrame": "past" } },
                { "id": "b", "name": "Sprint 2", "path": "Core\\Sprint 2", "attributes": { "timeFrame": "current" } },
                { "id": "c", "name": "Sprint 3", "attributes": { "timeFrame": 1 } }
            ]
        }))
        .unwrap();

        let iterations = map_iterations(list.value);

        assert_eq!(iterations.iter().map(|i| i.is_current).collect::<Vec<_>>(), [false, true, true]);
        assert_eq!(iterations[0].start_date.as_deref(), Some("2026-10-01T00:00:00Z"));
        assert_eq!(iterations[1].path, "Core\\Sprint 2");
        assert_eq!(iterations[2].path, "Sprint 3");
        // The documented sample uses `values`.
        let sample: IterationList =
            serde_json::from_value(json!({ "values": [{ "id": "x", "name": "S" }] })).unwrap();
        assert_eq!(sample.value.len(), 1);
    }

    #[test]
    fn maps_enabled_work_item_types_sorted_and_unique() {
        let raw: Vec<RawWorkItemType> = serde_json::from_value(json!([
            { "name": "Task" }, { "name": "Bug" }, { "name": "bug" },
            { "name": "Retired", "isDisabled": true }
        ]))
        .unwrap();
        assert_eq!(map_work_item_types(raw), ["Bug", "Task"]);
    }

    #[test]
    fn maps_team_members_people_only() {
        let raw: Vec<RawTeamMember> = serde_json::from_value(json!([
            { "identity": { "displayName": "Zoe", "uniqueName": "zoe@contoso.com" } },
            { "identity": { "displayName": "Ada", "uniqueName": "ada@contoso.com" } },
            { "identity": { "displayName": "Ada (dup)", "uniqueName": "ADA@contoso.com" } },
            { "identity": { "displayName": "[Core]\\Readers", "uniqueName": "vstfs:///x", "isContainer": true } },
            { "identity": { "displayName": "No name" } }
        ]))
        .unwrap();

        let members = map_team_members(raw);

        assert_eq!(
            members.iter().map(|m| m.unique_name.as_str()).collect::<Vec<_>>(),
            ["ada@contoso.com", "zoe@contoso.com"]
        );
        assert_eq!(members[0].display_name, "Ada");
    }
}
