//! Domain types exposed to the UI and pure mapping from Azure DevOps payloads.
//! Everything here is free of I/O so it can be unit tested.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use url::Url;

use crate::error::{AppError, AppResult};

pub const HOST: &str = "https://dev.azure.com";

// ---------------------------------------------------------------------------
// Domain types (sent to the frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub organization: String,
    pub user_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub id: i64,
    pub title: String,
    pub work_item_type: String,
    pub state: String,
    pub project: String,
    pub priority: Option<i64>,
    pub changed_at: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestRole {
    Author,
    Reviewer,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub id: i64,
    pub title: String,
    pub project: String,
    pub repository: String,
    pub repository_id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub author: String,
    pub created_at: String,
    pub is_draft: bool,
    pub role: PullRequestRole,
    /// The signed-in user's vote: 10 approved, 5 approved with suggestions,
    /// 0 no vote, -5 waiting for author, -10 rejected.
    pub my_vote: Option<i64>,
    pub approvals: usize,
    pub reviewer_count: usize,
    pub url: String,
}

// ---------------------------------------------------------------------------
// Raw Azure DevOps payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionData {
    pub authenticated_user: RawIdentity,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawIdentity {
    pub id: String,
    #[serde(default)]
    pub provider_display_name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListResponse<T> {
    pub value: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WiqlResponse {
    #[serde(default)]
    pub work_items: Vec<WiqlReference>,
}

#[derive(Debug, Deserialize)]
pub struct WiqlReference {
    pub id: i64,
}

#[derive(Debug, Deserialize)]
pub struct RawWorkItem {
    pub id: i64,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct RawProject {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPullRequest {
    pub pull_request_id: i64,
    pub title: String,
    #[serde(default)]
    pub is_draft: bool,
    pub creation_date: String,
    pub created_by: RawIdentity,
    pub repository: RawRepository,
    pub source_ref_name: String,
    pub target_ref_name: String,
    #[serde(default)]
    pub reviewers: Vec<RawReviewer>,
}

#[derive(Debug, Deserialize)]
pub struct RawRepository {
    pub id: String,
    pub name: String,
    pub project: RawProject,
}

#[derive(Debug, Deserialize)]
pub struct RawReviewer {
    pub id: String,
    #[serde(default)]
    pub vote: i64,
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Accepts `contoso`, `https://dev.azure.com/contoso[/...]` or
/// `https://contoso.visualstudio.com` and returns the bare organization name.
pub fn parse_organization(input: &str) -> AppResult<String> {
    let trimmed = input.trim().trim_end_matches('/');
    let invalid = || AppError::InvalidOrganization;

    let candidate = if trimmed.contains("://") {
        let url = Url::parse(trimmed).map_err(|_| invalid())?;
        let host = url.host_str().ok_or_else(invalid)?;
        if host.eq_ignore_ascii_case("dev.azure.com") {
            url.path_segments()
                .and_then(|mut segments| segments.next())
                .unwrap_or_default()
                .to_string()
        } else if let Some(org) = host.strip_suffix(".visualstudio.com") {
            org.to_string()
        } else {
            return Err(invalid());
        }
    } else {
        trimmed.to_string()
    };

    let valid = !candidate.is_empty()
        && candidate.len() <= 50
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && candidate.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());

    if valid { Ok(candidate) } else { Err(invalid()) }
}

/// Builds `https://dev.azure.com/{segments...}` with every segment percent-encoded.
pub fn build_url(segments: &[&str]) -> Url {
    let mut url = Url::parse(HOST).expect("HOST is a valid URL");
    url.path_segments_mut()
        .expect("HOST can be a base")
        .extend(segments);
    url
}

pub fn session_from(organization: &str, data: ConnectionData) -> Session {
    let user = data.authenticated_user;
    let display_name = user
        .provider_display_name
        .or(user.display_name)
        .unwrap_or_else(|| "Unknown user".into());
    Session {
        organization: organization.to_string(),
        user_id: user.id,
        display_name,
    }
}

pub fn map_work_item(organization: &str, raw: RawWorkItem) -> WorkItem {
    let text = |field: &str| {
        raw.fields
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let project = text("System.TeamProject");
    let url = build_url(&[organization, &project, "_workitems", "edit", &raw.id.to_string()]);

    WorkItem {
        id: raw.id,
        title: text("System.Title"),
        work_item_type: text("System.WorkItemType"),
        state: text("System.State"),
        priority: raw
            .fields
            .get("Microsoft.VSTS.Common.Priority")
            .and_then(Value::as_i64),
        changed_at: raw
            .fields
            .get("System.ChangedDate")
            .and_then(Value::as_str)
            .map(str::to_string),
        project,
        url: url.to_string(),
    }
}

fn short_branch(reference: &str) -> String {
    reference
        .strip_prefix("refs/heads/")
        .unwrap_or(reference)
        .to_string()
}

pub fn map_pull_request(organization: &str, user_id: &str, raw: RawPullRequest) -> PullRequest {
    let is_author = raw.created_by.id.eq_ignore_ascii_case(user_id);
    let my_vote = raw
        .reviewers
        .iter()
        .find(|r| r.id.eq_ignore_ascii_case(user_id))
        .map(|r| r.vote);
    let approvals = raw.reviewers.iter().filter(|r| r.vote >= 5).count();
    let project = raw.repository.project.name;
    let repository = raw.repository.name;
    let repository_id = raw.repository.id;
    let url = build_url(&[
        organization,
        &project,
        "_git",
        &repository,
        "pullrequest",
        &raw.pull_request_id.to_string(),
    ]);

    PullRequest {
        id: raw.pull_request_id,
        title: raw.title,
        source_branch: short_branch(&raw.source_ref_name),
        target_branch: short_branch(&raw.target_ref_name),
        author: raw
            .created_by
            .display_name
            .or(raw.created_by.provider_display_name)
            .unwrap_or_default(),
        created_at: raw.creation_date,
        is_draft: raw.is_draft,
        role: if is_author {
            PullRequestRole::Author
        } else {
            PullRequestRole::Reviewer
        },
        my_vote,
        approvals,
        reviewer_count: raw.reviewers.len(),
        project,
        repository,
        repository_id,
        url: url.to_string(),
    }
}

/// Removes duplicates (a PR can match both the author and reviewer queries)
/// and sorts newest first.
pub fn merge_pull_requests(mut prs: Vec<PullRequest>) -> Vec<PullRequest> {
    // Authors first so `dedup_by_key` keeps the author entry for the same PR.
    prs.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| (a.role != PullRequestRole::Author).cmp(&(b.role != PullRequestRole::Author)))
    });
    prs.dedup_by_key(|pr| pr.id);
    prs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    prs
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ME: &str = "11111111-aaaa-bbbb-cccc-000000000001";

    #[test]
    fn parses_plain_and_url_organizations() {
        assert_eq!(parse_organization("contoso").unwrap(), "contoso");
        assert_eq!(parse_organization("  smc-devs  ").unwrap(), "smc-devs");
        assert_eq!(parse_organization("https://dev.azure.com/smc-devs").unwrap(), "smc-devs");
        assert_eq!(
            parse_organization("https://dev.azure.com/smc-devs/Project/_boards/").unwrap(),
            "smc-devs"
        );
        assert_eq!(parse_organization("https://contoso.visualstudio.com/").unwrap(), "contoso");
    }

    #[test]
    fn rejects_unsafe_or_foreign_organizations() {
        for input in ["", "  ", "con/toso", "../etc", "-contoso", "https://evil.example.com/x", "a b"] {
            assert!(
                matches!(parse_organization(input), Err(AppError::InvalidOrganization)),
                "expected {input:?} to be rejected"
            );
        }
    }

    #[test]
    fn build_url_percent_encodes_segments() {
        let url = build_url(&["contoso", "My Project", "_git", "repo"]);
        assert_eq!(url.as_str(), "https://dev.azure.com/contoso/My%20Project/_git/repo");
    }

    #[test]
    fn session_prefers_provider_display_name() {
        let data: ConnectionData = serde_json::from_value(json!({
            "authenticatedUser": { "id": ME, "providerDisplayName": "Ada Lovelace" }
        }))
        .unwrap();

        let session = session_from("contoso", data);

        assert_eq!(session.user_id, ME);
        assert_eq!(session.display_name, "Ada Lovelace");
        assert_eq!(session.organization, "contoso");
    }

    #[test]
    fn maps_work_item_fields_and_web_url() {
        let raw: RawWorkItem = serde_json::from_value(json!({
            "id": 42,
            "fields": {
                "System.Title": "Fix login",
                "System.WorkItemType": "Bug",
                "System.State": "Active",
                "System.TeamProject": "My Project",
                "System.ChangedDate": "2026-11-01T10:00:00Z",
                "Microsoft.VSTS.Common.Priority": 2
            }
        }))
        .unwrap();

        let item = map_work_item("contoso", raw);

        assert_eq!(item.title, "Fix login");
        assert_eq!(item.work_item_type, "Bug");
        assert_eq!(item.state, "Active");
        assert_eq!(item.priority, Some(2));
        assert_eq!(item.url, "https://dev.azure.com/contoso/My%20Project/_workitems/edit/42");
    }

    fn raw_pr(id: i64, author_id: &str, reviewers: serde_json::Value, created: &str) -> RawPullRequest {
        serde_json::from_value(json!({
            "pullRequestId": id,
            "title": format!("PR {id}"),
            "isDraft": false,
            "creationDate": created,
            "createdBy": { "id": author_id, "displayName": "Someone" },
            "repository": { "id": "repo-guid", "name": "api", "project": { "name": "Core" } },
            "sourceRefName": "refs/heads/feature/x",
            "targetRefName": "refs/heads/main",
            "reviewers": reviewers
        }))
        .unwrap()
    }

    #[test]
    fn maps_reviewer_role_vote_and_branches() {
        let raw = raw_pr(
            7,
            "someone-else",
            json!([{ "id": ME.to_uppercase(), "vote": -5 }, { "id": "x", "vote": 10 }]),
            "2026-11-02T00:00:00Z",
        );

        let pr = map_pull_request("contoso", ME, raw);

        assert_eq!(pr.role, PullRequestRole::Reviewer);
        assert_eq!(pr.my_vote, Some(-5));
        assert_eq!(pr.approvals, 1);
        assert_eq!(pr.reviewer_count, 2);
        assert_eq!(pr.source_branch, "feature/x");
        assert_eq!(pr.target_branch, "main");
        assert_eq!(pr.url, "https://dev.azure.com/contoso/Core/_git/api/pullrequest/7");
        assert_eq!(pr.repository_id, "repo-guid");
    }

    #[test]
    fn merge_dedupes_keeping_author_role_and_sorts_newest_first() {
        let as_reviewer = map_pull_request(
            "contoso",
            ME,
            raw_pr(1, "other", json!([{ "id": ME, "vote": 0 }]), "2026-10-01T00:00:00Z"),
        );
        let mut as_author = as_reviewer.clone();
        as_author.role = PullRequestRole::Author;
        let newer = map_pull_request("contoso", ME, raw_pr(2, ME, json!([]), "2026-11-01T00:00:00Z"));

        let merged = merge_pull_requests(vec![as_reviewer, newer, as_author]);

        assert_eq!(merged.iter().map(|p| p.id).collect::<Vec<_>>(), vec![2, 1]);
        assert_eq!(merged[1].role, PullRequestRole::Author);
    }
}
