//! Build and policy status of pull requests ("checks").
//!
//! Policies come from policy evaluations (Code (Read)); builds come from the Build API, which
//! needs the optional Build (Read) scope. Each half fails on its own into `None`
//! ("unavailable"): a missing scope or a failed call never becomes an error for the caller,
//! because 401-like answers map to `Unauthorized`, which signs the user out.
//!
//! `context` and `settings` of a policy evaluation are undocumented or free-form, so every field
//! read from them is optional. Run links are built here from ids, never taken from payloads.

use std::collections::{HashMap, HashSet};

use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::AppState;
use crate::azdo::AzdoClient;
use crate::azdo::model::{self, ListResponse};
use crate::commands::current_client;
use crate::error::{AppError, AppResult};

const POLICY_API_VERSION: &str = "7.1-preview.1";
/// Pull requests per `pull_request_check_summaries` call.
pub const MAX_SUMMARIES: usize = 50;
/// Concurrent requests per organization, as for notification comments.
const CONCURRENCY: usize = 4;
/// Each list target sends two requests at once (policies and builds).
const TARGETS_AT_ONCE: usize = CONCURRENCY / 2;
/// Builds read per branch query; only the newest per definition is kept.
const BUILDS_PER_QUERY: &str = "50";
const MAX_NAME_CHARS: usize = 100;

// ---------------------------------------------------------------------------
// Types sent to the frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckState {
    Succeeded,
    Failed,
    Running,
    Queued,
    /// A build that partially succeeded.
    Partial,
    Canceled,
}

/// The list dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckSummary {
    Succeeded,
    Failed,
    Running,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PolicyKind {
    /// Build validation.
    Build,
    /// Status check posted by an external service.
    Status,
    /// Reviewers, comments, work items and anything else.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyCheck {
    pub name: String,
    pub kind: PolicyKind,
    /// Required to complete the pull request.
    pub blocking: bool,
    pub state: CheckState,
    /// The build run, for build validation policies that have one.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildSource {
    /// Built `refs/pull/{id}/merge` (pull request trigger or build validation).
    PullRequest,
    /// Built the source branch at the pull request's latest commit (CI).
    Branch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildCheck {
    pub id: i64,
    pub definition: String,
    pub number: String,
    pub state: CheckState,
    pub source: BuildSource,
    pub finished_at: Option<String>,
    pub url: String,
}

/// `None` halves are unavailable (missing scope, failed call).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestChecks {
    pub policies: Option<Vec<PolicyCheck>>,
    pub builds: Option<Vec<BuildCheck>>,
    pub summary: CheckSummary,
}

/// A pull request of the list, as the frontend knows it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckTarget {
    pub id: i64,
    pub project: String,
    pub repository_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckSummaryItem {
    pub id: i64,
    pub repository_id: String,
    pub summary: CheckSummary,
}

// ---------------------------------------------------------------------------
// Raw payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawEvaluation {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub configuration: Option<RawPolicyConfiguration>,
    /// Undocumented; build policies carry `buildId` here.
    #[serde(default)]
    pub context: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPolicyConfiguration {
    #[serde(default)]
    pub is_blocking: Option<bool>,
    #[serde(default)]
    pub settings: Option<Value>,
    #[serde(default, rename = "type")]
    pub policy_type: Option<RawPolicyType>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPolicyType {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawBuild {
    pub id: i64,
    #[serde(default)]
    pub build_number: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub definition: Option<RawDefinition>,
    #[serde(default)]
    pub source_version: Option<String>,
    #[serde(default)]
    pub finish_time: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDefinition {
    pub id: i64,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCheckPullRequest {
    source_ref_name: String,
    #[serde(default)]
    last_merge_source_commit: Option<RawCommitRef>,
    repository: RawCheckRepository,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCommitRef {
    commit_id: String,
}

#[derive(Debug, Deserialize)]
struct RawCheckRepository {
    project: RawProjectId,
}

#[derive(Debug, Deserialize)]
struct RawProjectId {
    id: String,
}

// ---------------------------------------------------------------------------
// Pure rules
// ---------------------------------------------------------------------------

/// State of a policy evaluation; `None` for policies that do not apply or unknown values.
pub fn policy_state(status: &str) -> Option<CheckState> {
    match status {
        "approved" => Some(CheckState::Succeeded),
        "rejected" | "broken" => Some(CheckState::Failed),
        "running" => Some(CheckState::Running),
        "queued" => Some(CheckState::Queued),
        _ => None,
    }
}

/// State of a build from its `status` and, once completed, its `result`.
pub fn build_state(status: Option<&str>, result: Option<&str>) -> CheckState {
    match (status, result) {
        (Some("inProgress" | "cancelling"), _) => CheckState::Running,
        (Some("completed"), Some("succeeded")) => CheckState::Succeeded,
        (Some("completed"), Some("partiallySucceeded")) => CheckState::Partial,
        (Some("completed"), Some("canceled")) => CheckState::Canceled,
        (Some("completed"), _) => CheckState::Failed,
        _ => CheckState::Queued,
    }
}

/// `https://dev.azure.com/{org}/{project}/_build/results?buildId={id}`.
pub fn build_run_url(organization: &str, project: &str, build_id: i64) -> String {
    let mut url = model::build_url(&[organization, project, "_build", "results"]);
    url.query_pairs_mut().append_pair("buildId", &build_id.to_string());
    url.to_string()
}

/// Applicable policies, required ones first; names are plain bounded text.
pub fn map_policies(organization: &str, project: &str, evaluations: Vec<RawEvaluation>) -> Vec<PolicyCheck> {
    let mut policies: Vec<PolicyCheck> = evaluations
        .into_iter()
        .filter_map(|evaluation| {
            let state = policy_state(evaluation.status.as_deref()?)?;
            let configuration = evaluation.configuration;
            let settings = configuration.as_ref().and_then(|c| c.settings.as_ref());
            let setting = |key: &str| settings.and_then(|s| s.get(key));
            let text = |key: &str| setting(key).and_then(Value::as_str).filter(|v| !v.trim().is_empty());
            let kind = if setting("buildDefinitionId").is_some_and(Value::is_number) {
                PolicyKind::Build
            } else if text("statusName").is_some() {
                PolicyKind::Status
            } else {
                PolicyKind::Other
            };
            let type_name = configuration
                .as_ref()
                .and_then(|c| c.policy_type.as_ref())
                .and_then(|t| t.display_name.as_deref())
                .filter(|v| !v.trim().is_empty());
            let name = match (kind, text("displayName")) {
                (_, Some(display_name)) => display_name.to_string(),
                (PolicyKind::Status, None) => match text("statusGenre") {
                    Some(genre) => format!("{genre}/{}", text("statusName").unwrap_or_default()),
                    None => text("statusName").unwrap_or_default().to_string(),
                },
                _ => type_name.unwrap_or("Policy").to_string(),
            };
            let url = (kind == PolicyKind::Build)
                .then(|| evaluation.context.as_ref()?.get("buildId")?.as_i64())
                .flatten()
                .filter(|id| *id > 0)
                .map(|id| build_run_url(organization, project, id));
            Some(PolicyCheck {
                name: clean_name(&name, "Policy"),
                kind,
                blocking: configuration.and_then(|c| c.is_blocking).unwrap_or(false),
                state,
                url,
            })
        })
        .collect();
    // Stable: keeps Azure DevOps' order within required and optional policies.
    policies.sort_by_key(|p| !p.blocking);
    policies
}

/// Plain text without control or bidi characters, at most `MAX_NAME_CHARS`.
fn clean_name(name: &str, fallback: &str) -> String {
    let cleaned = crate::themes::clean_text(name, MAX_NAME_CHARS);
    if cleaned.is_empty() { fallback.to_string() } else { cleaned }
}

/// Newest build per definition, newest first. `builds` may come from several queries.
pub fn latest_per_definition(organization: &str, project: &str, mut builds: Vec<(BuildSource, RawBuild)>) -> Vec<BuildCheck> {
    // Build ids grow in queue order; `queueTime` text has a variable precision.
    builds.sort_by_key(|(_, b)| std::cmp::Reverse(b.id));
    let mut seen_definitions = HashSet::new();
    let mut seen_builds = HashSet::new();
    builds
        .into_iter()
        .filter(|(_, b)| seen_builds.insert(b.id))
        .filter(|(_, b)| b.definition.as_ref().is_none_or(|d| seen_definitions.insert(d.id)))
        .map(|(source, b)| BuildCheck {
            id: b.id,
            definition: clean_name(b.definition.as_ref().and_then(|d| d.name.as_deref()).unwrap_or_default(), "Build"),
            number: crate::themes::clean_text(b.build_number.as_deref().unwrap_or_default(), MAX_NAME_CHARS),
            state: build_state(b.status.as_deref(), b.result.as_deref()),
            source,
            finished_at: b.finish_time,
            url: build_run_url(organization, project, b.id),
        })
        .collect()
}

/// The list dot: build-type policies and builds only; failed, then running, then succeeded.
pub fn summarize(policies: Option<&[PolicyCheck]>, builds: Option<&[BuildCheck]>) -> CheckSummary {
    let policy_states = policies
        .unwrap_or_default()
        .iter()
        .filter(|p| p.kind != PolicyKind::Other)
        .map(|p| p.state);
    let build_states = builds.unwrap_or_default().iter().map(|b| b.state);
    let mut summary = CheckSummary::None;
    for state in policy_states.chain(build_states) {
        summary = match (summary, state) {
            (_, CheckState::Failed | CheckState::Partial) | (CheckSummary::Failed, _) => CheckSummary::Failed,
            (_, CheckState::Running | CheckState::Queued) | (CheckSummary::Running, _) => CheckSummary::Running,
            (_, CheckState::Succeeded) => CheckSummary::Succeeded,
            (current, CheckState::Canceled) => current,
        };
    }
    summary
}

/// Builds of the detail: pull request builds plus source-branch builds at `commit`. A failed
/// merge-ref query (likely no Build (Read)) makes builds unavailable; a failed branch query only
/// drops the branch builds.
pub fn select_builds(
    organization: &str,
    project: &str,
    merge: Option<Vec<RawBuild>>,
    branch: Option<Vec<RawBuild>>,
    commit: Option<&str>,
) -> Option<Vec<BuildCheck>> {
    let merge = merge?.into_iter().map(|b| (BuildSource::PullRequest, b));
    let at_commit = branch
        .unwrap_or_default()
        .into_iter()
        .filter(|b| commit.is_some() && b.source_version.as_deref() == commit)
        .map(|b| (BuildSource::Branch, b));
    Some(latest_per_definition(organization, project, merge.chain(at_commit).collect()))
}

/// A pull request reference the frontend may send: positive id, bounded project, GUID-like
/// repository id.
pub fn is_valid_target(target: &CheckTarget) -> bool {
    let repository = &target.repository_id;
    target.id > 0
        && !target.project.trim().is_empty()
        && target.project.len() <= 256
        && !repository.is_empty()
        && repository.len() <= 64
        && repository.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Keeps targets that can name a repository, at most `MAX_SUMMARIES`.
pub fn valid_targets(targets: Vec<CheckTarget>) -> Vec<CheckTarget> {
    targets.into_iter().filter(is_valid_target).take(MAX_SUMMARIES).collect()
}

/// `Ok` value, or `None` with a log line naming only the error kind (never URLs or messages).
fn available<T>(what: &str, result: AppResult<T>) -> Option<T> {
    result.map_err(|err: AppError| eprintln!("checks: {what} unavailable ({})", err.kind())).ok()
}

// ---------------------------------------------------------------------------
// HTTP orchestration
// ---------------------------------------------------------------------------

async fn evaluations(client: &AzdoClient, project: &str, project_id: &str, id: i64) -> AppResult<Vec<RawEvaluation>> {
    let mut url = client.url_with_version(&[project, "_apis", "policy", "evaluations"], POLICY_API_VERSION);
    url.query_pairs_mut()
        .append_pair("artifactId", &format!("vstfs:///CodeReview/CodeReviewId/{project_id}/{id}"));
    let list: ListResponse<RawEvaluation> = client.get_json(url).await?;
    Ok(list.value)
}

async fn builds_of_branch(client: &AzdoClient, project: &str, repository_id: &str, branch: &str) -> AppResult<Vec<RawBuild>> {
    let mut url = client.url(&[project, "_apis", "build", "builds"]);
    url.query_pairs_mut()
        .append_pair("branchName", branch)
        .append_pair("repositoryId", repository_id)
        .append_pair("repositoryType", "TfsGit")
        .append_pair("queryOrder", "queueTimeDescending")
        .append_pair("$top", BUILDS_PER_QUERY);
    let list: ListResponse<RawBuild> = client.get_json(url).await?;
    Ok(list.value)
}

fn merge_ref(id: i64) -> String {
    format!("refs/pull/{id}/merge")
}

async fn repository_project_id(client: &AzdoClient, project: &str, repository_id: &str) -> AppResult<String> {
    #[derive(Deserialize)]
    struct RawRepository {
        project: RawProjectId,
    }
    let url = client.url(&[project, "_apis", "git", "repositories", repository_id]);
    let raw: RawRepository = client.get_json(url).await?;
    Ok(raw.project.id)
}

/// Policies and builds of one pull request for the detail view.
async fn checks_of(client: &AzdoClient, project: &str, repository_id: &str, id: i64) -> PullRequestChecks {
    const UNAVAILABLE: PullRequestChecks = PullRequestChecks { policies: None, builds: None, summary: CheckSummary::None };
    let org = client.org();
    let pr_url = crate::pull_requests::pr_url(client, project, repository_id, id, &[]);
    let Some(pr) = available("pull request", client.get_json::<RawCheckPullRequest>(pr_url).await) else {
        return UNAVAILABLE;
    };
    let commit = pr.last_merge_source_commit.map(|c| c.commit_id);
    let merge = merge_ref(id);
    let (evaluations, merge_builds, branch_builds) = futures::join!(
        evaluations(client, project, &pr.repository.project.id, id),
        builds_of_branch(client, project, repository_id, &merge),
        builds_of_branch(client, project, repository_id, &pr.source_ref_name),
    );
    let policies = available("policies", evaluations).map(|list| map_policies(org, project, list));
    let builds = select_builds(
        org,
        project,
        available("pull request builds", merge_builds),
        available("branch builds", branch_builds),
        commit.as_deref(),
    );
    let summary = summarize(policies.as_deref(), builds.as_deref());
    PullRequestChecks { policies, builds, summary }
}

/// The list dot of one pull request; `None` when nothing could be read.
async fn summary_of(client: &AzdoClient, target: &CheckTarget, project_id: Option<&str>) -> CheckSummary {
    let org = client.org();
    let policies = async {
        match project_id {
            Some(project_id) => available("policies", evaluations(client, &target.project, project_id, target.id).await)
                .map(|list| map_policies(org, &target.project, list)),
            None => None,
        }
    };
    let merge = merge_ref(target.id);
    let builds = async {
        available("pull request builds", builds_of_branch(client, &target.project, &target.repository_id, &merge).await)
            .map(|list| {
                let tagged = list.into_iter().map(|b| (BuildSource::PullRequest, b)).collect();
                latest_per_definition(org, &target.project, tagged)
            })
    };
    let (policies, builds) = futures::join!(policies, builds);
    summarize(policies.as_deref(), builds.as_deref())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Checks of one pull request. Never fails for missing scopes or failed calls: those halves
/// come back as `null`.
#[tauri::command]
pub async fn get_pull_request_checks(
    state: State<'_, AppState>,
    project: String,
    repository_id: String,
    id: i64,
) -> AppResult<PullRequestChecks> {
    let client = current_client(&state)?;
    let target = CheckTarget { id, project, repository_id };
    if !is_valid_target(&target) {
        return Ok(PullRequestChecks { policies: None, builds: None, summary: CheckSummary::None });
    }
    Ok(checks_of(&client, &target.project, &target.repository_id, target.id).await)
}

/// The list dot of up to `MAX_SUMMARIES` pull requests of the active organization. Project ids
/// are resolved once per repository; failures yield `none`.
#[tauri::command]
pub async fn pull_request_check_summaries(
    state: State<'_, AppState>,
    items: Vec<CheckTarget>,
) -> AppResult<Vec<CheckSummaryItem>> {
    let client = current_client(&state)?;
    let targets = valid_targets(items);

    let mut repositories: Vec<(String, String)> = Vec::new();
    let mut seen = HashSet::new();
    for t in &targets {
        if seen.insert(t.repository_id.clone()) {
            repositories.push((t.project.clone(), t.repository_id.clone()));
        }
    }
    let project_ids: HashMap<String, String> = stream::iter(repositories)
        .map(|(project, repository_id)| {
            let client = &client;
            async move {
                let id = available("repository", repository_project_id(client, &project, &repository_id).await);
                (repository_id, id)
            }
        })
        .buffer_unordered(CONCURRENCY)
        .filter_map(|(repository_id, id)| async move { id.map(|id| (repository_id, id)) })
        .collect()
        .await;

    let summaries = stream::iter(targets)
        .map(|target| {
            let client = &client;
            let project_id = project_ids.get(&target.repository_id).cloned();
            async move {
                let summary = summary_of(client, &target, project_id.as_deref()).await;
                CheckSummaryItem { id: target.id, repository_id: target.repository_id, summary }
            }
        })
        .buffer_unordered(TARGETS_AT_ONCE)
        .collect()
        .await;
    Ok(summaries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn evaluation(value: Value) -> RawEvaluation {
        serde_json::from_value(value).unwrap()
    }

    fn build(value: Value) -> RawBuild {
        serde_json::from_value(value).unwrap()
    }

    fn policy(kind: PolicyKind, blocking: bool, state: CheckState) -> PolicyCheck {
        PolicyCheck { name: "p".into(), kind, blocking, state, url: None }
    }

    fn build_check(state: CheckState) -> BuildCheck {
        BuildCheck {
            id: 1,
            definition: "ci".into(),
            number: "1".into(),
            state,
            source: BuildSource::PullRequest,
            finished_at: None,
            url: String::new(),
        }
    }

    #[test]
    fn policy_states_follow_the_documented_values() {
        assert_eq!(policy_state("approved"), Some(CheckState::Succeeded));
        assert_eq!(policy_state("rejected"), Some(CheckState::Failed));
        assert_eq!(policy_state("broken"), Some(CheckState::Failed));
        assert_eq!(policy_state("running"), Some(CheckState::Running));
        assert_eq!(policy_state("queued"), Some(CheckState::Queued));
        assert_eq!(policy_state("notApplicable"), None);
        assert_eq!(policy_state("somethingNew"), None);
    }

    #[test]
    fn build_states_follow_status_and_result() {
        assert_eq!(build_state(Some("inProgress"), None), CheckState::Running);
        assert_eq!(build_state(Some("cancelling"), None), CheckState::Running);
        assert_eq!(build_state(Some("notStarted"), None), CheckState::Queued);
        assert_eq!(build_state(Some("postponed"), None), CheckState::Queued);
        assert_eq!(build_state(Some("completed"), Some("succeeded")), CheckState::Succeeded);
        assert_eq!(build_state(Some("completed"), Some("partiallySucceeded")), CheckState::Partial);
        assert_eq!(build_state(Some("completed"), Some("failed")), CheckState::Failed);
        assert_eq!(build_state(Some("completed"), Some("canceled")), CheckState::Canceled);
        assert_eq!(build_state(None, None), CheckState::Queued);
    }

    #[test]
    fn build_run_links_are_built_from_ids() {
        assert_eq!(
            build_run_url("contoso", "My Web", 42),
            "https://dev.azure.com/contoso/My%20Web/_build/results?buildId=42"
        );
    }

    #[test]
    fn policies_are_classified_by_settings_named_cleanly_and_required_first() {
        let list = vec![
            evaluation(json!({
                "status": "approved",
                "configuration": { "isBlocking": false, "type": { "displayName": "Minimum number of reviewers" }, "settings": { "minimumApproverCount": 2 } }
            })),
            evaluation(json!({
                "status": "rejected",
                "configuration": { "isBlocking": true, "type": { "displayName": "Build" }, "settings": { "buildDefinitionId": 7, "displayName": "CI \u{202e}gate" } },
                "context": { "buildId": 99 }
            })),
            evaluation(json!({ "status": "notApplicable", "configuration": { "isBlocking": true, "type": { "displayName": "Work item linking" } } })),
            evaluation(json!({
                "status": "running",
                "configuration": { "isBlocking": true, "type": { "displayName": "Status" }, "settings": { "statusName": "sonar", "statusGenre": "qa" } }
            })),
            evaluation(json!({ "status": "queued" })),
        ];

        let policies = map_policies("contoso", "Web", list);

        let names: Vec<&str> = policies.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["CI gate", "qa/sonar", "Minimum number of reviewers", "Policy"]);
        assert_eq!(policies[0].kind, PolicyKind::Build);
        assert!(policies[0].blocking);
        assert_eq!(policies[0].state, CheckState::Failed);
        assert_eq!(policies[0].url.as_deref(), Some("https://dev.azure.com/contoso/Web/_build/results?buildId=99"));
        assert_eq!(policies[1].kind, PolicyKind::Status);
        assert_eq!(policies[1].url, None);
        assert_eq!(policies[2].kind, PolicyKind::Other);
        assert!(!policies[2].blocking);
        assert_eq!(policies[3].state, CheckState::Queued);
    }

    #[test]
    fn long_policy_names_are_bounded() {
        let list = vec![evaluation(json!({
            "status": "approved",
            "configuration": { "type": { "displayName": "x".repeat(500) } }
        }))];
        assert_eq!(map_policies("contoso", "Web", list)[0].name.chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn only_the_newest_build_of_each_definition_is_kept() {
        let builds = vec![
            (BuildSource::PullRequest, build(json!({ "id": 3, "buildNumber": "20261110.3", "status": "inProgress", "definition": { "id": 1, "name": "ci" }, "queueTime": "2026-11-10T10:00:00Z" }))),
            (BuildSource::PullRequest, build(json!({ "id": 2, "buildNumber": "20261110.2", "status": "completed", "result": "failed", "definition": { "id": 1, "name": "ci" }, "queueTime": "2026-11-10T10:00:00.5Z" }))),
            (BuildSource::Branch, build(json!({ "id": 5, "buildNumber": "7", "status": "completed", "result": "succeeded", "definition": { "id": 2, "name": "lint" }, "finishTime": "2026-11-10T11:05:00Z" }))),
            (BuildSource::Branch, build(json!({ "id": 4, "status": "completed", "result": "succeeded" }))),
            (BuildSource::Branch, build(json!({ "id": 5, "status": "completed", "result": "failed", "definition": { "id": 2, "name": "lint" } }))),
        ];

        let latest = latest_per_definition("contoso", "Web", builds);

        // Build ids grow with queue order, so the highest id per definition is the newest,
        // whatever the precision of `queueTime`.
        let ids: Vec<i64> = latest.iter().map(|b| b.id).collect();
        assert_eq!(ids, vec![5, 4, 3]);
        assert_eq!(latest[0].definition, "lint");
        assert_eq!(latest[0].source, BuildSource::Branch);
        assert_eq!(latest[0].state, CheckState::Succeeded);
        assert_eq!(latest[0].finished_at.as_deref(), Some("2026-11-10T11:05:00Z"));
        assert_eq!(latest[1].definition, "Build");
        assert_eq!(latest[1].number, "");
        assert_eq!(latest[2].state, CheckState::Running);
        assert_eq!(latest[2].url, "https://dev.azure.com/contoso/Web/_build/results?buildId=3");
    }

    #[test]
    fn detail_builds_keep_branch_ci_at_the_latest_commit_and_degrade_per_query() {
        let merge = || vec![build(json!({ "id": 10, "status": "completed", "result": "failed", "definition": { "id": 1, "name": "validation" } }))];
        let branch = || {
            vec![
                build(json!({ "id": 12, "status": "completed", "result": "succeeded", "definition": { "id": 2, "name": "ci" }, "sourceVersion": "abc" })),
                build(json!({ "id": 11, "status": "completed", "result": "succeeded", "definition": { "id": 3, "name": "old" }, "sourceVersion": "old" })),
            ]
        };
        let ids = |builds: Option<Vec<BuildCheck>>| builds.map(|list| list.iter().map(|b| b.id).collect::<Vec<_>>());

        assert_eq!(ids(select_builds("o", "p", Some(merge()), Some(branch()), Some("abc"))), Some(vec![12, 10]));
        assert_eq!(ids(select_builds("o", "p", Some(merge()), Some(branch()), None)), Some(vec![10]));
        assert_eq!(ids(select_builds("o", "p", Some(merge()), None, Some("abc"))), Some(vec![10]));
        assert_eq!(ids(select_builds("o", "p", None, Some(branch()), Some("abc"))), None);
        let sources: Vec<BuildSource> = select_builds("o", "p", Some(merge()), Some(branch()), Some("abc"))
            .unwrap()
            .iter()
            .map(|b| b.source)
            .collect();
        assert_eq!(sources, vec![BuildSource::Branch, BuildSource::PullRequest]);
    }

    #[test]
    fn detail_and_list_targets_share_one_validation() {
        let target = |id: i64, project: &str, repository_id: &str| CheckTarget { id, project: project.into(), repository_id: repository_id.into() };
        let repo = "3411ebc1-d5aa-464f-9615-0b527bc66719";
        assert!(is_valid_target(&target(1, "Web", repo)));
        assert!(!is_valid_target(&target(0, "Web", repo)));
        assert!(!is_valid_target(&target(1, "  ", repo)));
        assert!(!is_valid_target(&target(1, &"p".repeat(257), repo)));
        assert!(!is_valid_target(&target(1, "Web", "../_apis")));
        assert!(!is_valid_target(&target(1, "Web", &"a".repeat(65))));
    }

    #[test]
    fn the_dot_uses_build_type_checks_and_the_worst_state_wins() {
        let reviewers_failed = policy(PolicyKind::Other, true, CheckState::Failed);
        let build_ok = policy(PolicyKind::Build, true, CheckState::Succeeded);
        let status_queued = policy(PolicyKind::Status, false, CheckState::Queued);

        assert_eq!(summarize(Some(std::slice::from_ref(&reviewers_failed)), Some(&[])), CheckSummary::None);
        assert_eq!(summarize(Some(&[reviewers_failed.clone(), build_ok.clone()]), None), CheckSummary::Succeeded);
        assert_eq!(summarize(Some(&[build_ok.clone(), status_queued.clone()]), None), CheckSummary::Running);
        assert_eq!(
            summarize(Some(std::slice::from_ref(&build_ok)), Some(&[build_check(CheckState::Partial)])),
            CheckSummary::Failed
        );
        assert_eq!(
            summarize(Some(&[status_queued]), Some(&[build_check(CheckState::Failed)])),
            CheckSummary::Failed
        );
        assert_eq!(summarize(None, Some(&[build_check(CheckState::Canceled)])), CheckSummary::None);
        assert_eq!(summarize(None, Some(&[build_check(CheckState::Running)])), CheckSummary::Running);
        assert_eq!(summarize(None, None), CheckSummary::None);
    }

    #[test]
    fn targets_are_validated_and_capped() {
        let target = |id: i64, project: &str, repository_id: &str| CheckTarget { id, project: project.into(), repository_id: repository_id.into() };
        let repo = "3411ebc1-d5aa-464f-9615-0b527bc66719";
        let mut list = vec![
            target(1, "Web", repo),
            target(0, "Web", repo),
            target(2, "", repo),
            target(3, "Web", "../../_apis"),
            target(4, "Web", ""),
        ];
        list.extend((10..10 + MAX_SUMMARIES as i64 + 5).map(|i| target(i, "Web", repo)));

        let valid = valid_targets(list);

        assert_eq!(valid.len(), MAX_SUMMARIES);
        assert_eq!(valid[0].id, 1);
        assert!(valid.iter().all(|t| t.id > 0 && !t.project.is_empty() && t.repository_id == repo));
    }

    #[test]
    fn summaries_serialize_for_the_frontend() {
        let item = CheckSummaryItem { id: 5, repository_id: "r".into(), summary: CheckSummary::Running };
        assert_eq!(serde_json::to_value(item).unwrap(), json!({ "id": 5, "repositoryId": "r", "summary": "running" }));
        let checks = PullRequestChecks { policies: None, builds: Some(vec![]), summary: CheckSummary::None };
        assert_eq!(serde_json::to_value(checks).unwrap(), json!({ "policies": null, "builds": [], "summary": "none" }));
    }
}
