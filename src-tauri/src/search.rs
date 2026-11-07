//! Quick search (Cmd+K palette, later the global search bar): open a work item or pull
//! request by ID, or find one by title.
//!
//! The query syntax and the matching rules are pure functions at the top of this module so
//! they are unit tested; free text only reaches WIQL through [`escape_literal`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::State;

use crate::AppState;
use crate::azdo::AzdoClient;
use crate::azdo::model::{PullRequest, PullRequestState, WorkItem};
use crate::commands::connected_clients;
use crate::credentials::OrganizationIndex;
use crate::error::{AppError, AppResult};
use crate::queries::{
    self, PullRequestFilter, PullRequestRoleFilter, PullRequestStatusFilter, QueryError, QueryResult,
    escape_literal,
};

/// Longest free-text query accepted.
pub const MAX_QUERY_CHARS: usize = 100;
/// Shortest free-text query that runs a title search.
pub const MIN_TEXT_CHARS: usize = 2;
/// Results per kind (work items, pull requests) and organization.
pub const MAX_HITS_PER_KIND: usize = 20;
/// How long the user's own pull requests are reused between title searches.
const OWN_PULL_REQUESTS_TTL: Duration = Duration::from_secs(60);

/// What the user typed, once parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickQuery {
    /// `#123`
    WorkItemId(i64),
    /// `!123`
    PullRequestId(i64),
    /// `123`: a work item or a pull request.
    AnyId(i64),
    /// Anything else: a title search.
    Text(String),
}

fn invalid_id() -> QueryError {
    QueryError::InvalidSearch("IDs are whole numbers from 1 to 2147483647")
}

/// Parses the palette input. `Ok(None)` means there is nothing to search yet (empty or
/// too short). Errors are fixed text: the query is never echoed.
pub fn parse_quick_query(raw: &str) -> QueryResult<Option<QuickQuery>> {
    // Checked before trimming so a trailing newline is rejected, not silently dropped.
    if raw.chars().any(char::is_control) {
        return Err(QueryError::InvalidSearch("Search text cannot contain line breaks"));
    }
    let text = raw.trim();
    if text.chars().count() > MAX_QUERY_CHARS {
        return Err(QueryError::InvalidSearch("Search text is limited to 100 characters"));
    }

    let (kind, rest): (fn(i64) -> QuickQuery, &str) = if let Some(rest) = text.strip_prefix('#') {
        (QuickQuery::WorkItemId, rest)
    } else if let Some(rest) = text.strip_prefix('!') {
        (QuickQuery::PullRequestId, rest)
    } else {
        (QuickQuery::AnyId, text)
    };
    let unsigned = rest.strip_prefix('-').unwrap_or(rest);
    if !unsigned.is_empty() && unsigned.chars().all(|c| c.is_ascii_digit()) {
        return match rest.parse::<i64>() {
            Ok(id) if (1..=i64::from(i32::MAX)).contains(&id) => Ok(Some(kind(id))),
            _ => Err(invalid_id()),
        };
    }

    if text.chars().count() < MIN_TEXT_CHARS {
        return Ok(None);
    }
    Ok(Some(QuickQuery::Text(text.to_string())))
}

/// Organization-wide WIQL for work items whose title contains `text`, newest change first.
pub fn title_wiql(text: &str) -> QueryResult<String> {
    Ok(format!(
        "SELECT [System.Id] FROM WorkItems WHERE [System.Title] CONTAINS {} \
         ORDER BY [System.ChangedDate] DESC",
        escape_literal(text)?
    ))
}

/// True when `title` contains every whitespace-separated word of `text`, ignoring case.
pub fn title_matches(title: &str, text: &str) -> bool {
    let title = title.to_lowercase();
    text.split_whitespace().all(|word| title.contains(&word.to_lowercase()))
}

/// Pull requests whose title matches `text`: active ones first, then newest first, capped.
pub fn filter_pull_requests(prs: &[PullRequest], text: &str) -> Vec<PullRequest> {
    let mut found: Vec<PullRequest> = prs.iter().filter(|pr| title_matches(&pr.title, text)).cloned().collect();
    // Pull request IDs grow over time, so a higher ID is a newer pull request.
    found.sort_by_key(|pr| (pr.status != PullRequestState::Active, std::cmp::Reverse(pr.id)));
    found.truncate(MAX_HITS_PER_KIND);
    found
}

/// Work items in the order WIQL returned them, capped.
pub fn cap_work_items(mut items: Vec<WorkItem>) -> Vec<WorkItem> {
    items.truncate(MAX_HITS_PER_KIND);
    items
}

/// Organizations to search: the active one first, then the others in configured order.
pub fn search_order(index: &OrganizationIndex) -> Vec<String> {
    let active = index.active.as_ref().filter(|active| index.organizations.contains(active));
    active
        .into_iter()
        .chain(index.organizations.iter().filter(|org| Some(*org) != active))
        .cloned()
        .collect()
}

/// Combines per-organization outcomes in search order. An organization that fails is named
/// in `unavailable` and never fails the whole search.
pub fn combine(outcomes: Vec<(String, QueryResult<Vec<SearchHit>>)>) -> SearchResults {
    let mut results = SearchResults::default();
    for (organization, outcome) in outcomes {
        match outcome {
            Ok(hits) => results.hits.extend(hits),
            Err(_) => results.unavailable.push(organization),
        }
    }
    results
}

// ---------------------------------------------------------------------------
// Results and state
// ---------------------------------------------------------------------------

/// One result, with the organization it belongs to. Items have the same shape as the
/// lists, so the workspace opens them without another request.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SearchHit {
    #[serde(rename_all = "camelCase")]
    WorkItem { organization: String, item: WorkItem },
    #[serde(rename_all = "camelCase")]
    PullRequest { organization: String, item: PullRequest },
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Organizations that could not be searched (for example an expired token).
    pub unavailable: Vec<String>,
}

type OwnPullRequests = (Instant, Arc<Vec<PullRequest>>);

/// Pull requests have no server-side title filter, so title searches filter the user's own
/// pull requests, fetched once per organization and user and reused for a short time.
#[derive(Default)]
pub struct SearchState {
    own_pull_requests: Mutex<HashMap<(String, String), OwnPullRequests>>,
}

impl SearchState {
    fn cache(&self) -> MutexGuard<'_, HashMap<(String, String), OwnPullRequests>> {
        self.own_pull_requests.lock().unwrap_or_else(|e| e.into_inner())
    }

    async fn own_pull_requests(&self, client: &AzdoClient) -> QueryResult<Arc<Vec<PullRequest>>> {
        let session = client.session();
        let key = (session.organization.clone(), session.user_id.clone());
        if let Some((fetched, prs)) = self.cache().get(&key)
            && fetched.elapsed() < OWN_PULL_REQUESTS_TTL
        {
            return Ok(prs.clone());
        }
        // The lock is never held across the network call; a concurrent miss fetches twice.
        let filter = PullRequestFilter {
            project: None,
            status: PullRequestStatusFilter::All,
            role: PullRequestRoleFilter::Mine,
        };
        let prs = Arc::new(queries::fetch_pull_requests(client, &filter).await?);
        self.cache().insert(key, (Instant::now(), prs.clone()));
        Ok(prs)
    }
}

async fn work_item_by_id(client: &AzdoClient, id: i64) -> AppResult<Vec<WorkItem>> {
    match client.work_items_by_ids(&[id]).await {
        Err(AppError::NotFound) => Ok(Vec::new()),
        other => other,
    }
}

/// Every hit for `query` in the organization of `client`: work items first, then pull requests.
async fn search_organization(
    client: &AzdoClient,
    state: &SearchState,
    query: &QuickQuery,
) -> QueryResult<Vec<SearchHit>> {
    let (work_items, pull_requests) = match query {
        QuickQuery::WorkItemId(id) => (work_item_by_id(client, *id).await?, Vec::new()),
        QuickQuery::PullRequestId(id) => {
            (Vec::new(), client.pull_request_by_id(*id).await?.into_iter().collect())
        }
        QuickQuery::AnyId(id) => {
            let (work_items, pull_request) =
                futures::join!(work_item_by_id(client, *id), client.pull_request_by_id(*id));
            (work_items?, pull_request?.into_iter().collect())
        }
        QuickQuery::Text(text) => {
            let wiql = title_wiql(text)?;
            let (work_items, own) = futures::join!(
                client.work_items_by_wiql_top(&wiql, MAX_HITS_PER_KIND),
                state.own_pull_requests(client)
            );
            (cap_work_items(work_items?), filter_pull_requests(&own?, text))
        }
    };

    let organization = client.session().organization.clone();
    Ok(work_items
        .into_iter()
        .map(|item| SearchHit::WorkItem { organization: organization.clone(), item })
        .chain(
            pull_requests
                .into_iter()
                .map(|item| SearchHit::PullRequest { organization: organization.clone(), item }),
        )
        .collect())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Quick search in every configured organization, the active one first. An empty or too
/// short query returns no hits without calling Azure DevOps.
#[tauri::command]
pub async fn quick_search(
    state: State<'_, AppState>,
    search: State<'_, SearchState>,
    query: String,
) -> QueryResult<SearchResults> {
    let Some(query) = parse_quick_query(&query)? else {
        return Ok(SearchResults::default());
    };
    let order = search_order(&state.store.index()?);
    if order.is_empty() {
        return Err(AppError::NotSignedIn.into());
    }
    let clients = connected_clients(&state, &order).await;
    let (search, query) = (&*search, &query);
    let outcomes = futures::future::join_all(clients.into_iter().map(|(organization, client)| async move {
        let outcome = match client {
            Ok(client) => search_organization(&client, search, query).await,
            Err(err) => Err(err.into()),
        };
        (organization, outcome)
    }))
    .await;
    Ok(combine(outcomes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azdo::model::PullRequestRole;

    fn pr(id: i64, title: &str, status: PullRequestState) -> PullRequest {
        PullRequest {
            id,
            title: title.into(),
            project: "Web".into(),
            repository: "site".into(),
            repository_id: "repo-1".into(),
            source_branch: "feature".into(),
            target_branch: "main".into(),
            author: "Ada".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            is_draft: false,
            role: PullRequestRole::Author,
            my_vote: None,
            approvals: 0,
            reviewer_count: 0,
            url: format!("https://dev.azure.com/contoso/Web/_git/site/pullrequest/{id}"),
            status,
            closed_at: None,
        }
    }

    fn wi(id: i64) -> WorkItem {
        WorkItem {
            id,
            title: format!("Item {id}"),
            work_item_type: "Bug".into(),
            state: "Active".into(),
            project: "Web".into(),
            priority: None,
            changed_at: None,
            url: String::new(),
        }
    }

    #[test]
    fn prefixes_select_the_kind_and_bare_numbers_look_up_both() {
        assert_eq!(parse_quick_query("#123").unwrap(), Some(QuickQuery::WorkItemId(123)));
        assert_eq!(parse_quick_query("  !45 ").unwrap(), Some(QuickQuery::PullRequestId(45)));
        assert_eq!(parse_quick_query("678").unwrap(), Some(QuickQuery::AnyId(678)));
        assert_eq!(parse_quick_query("2147483647").unwrap(), Some(QuickQuery::AnyId(2_147_483_647)));
    }

    #[test]
    fn ids_out_of_range_are_rejected_without_echoing_them() {
        for raw in ["#0", "!0", "0", "#2147483648", "99999999999999999999", "!-3"] {
            let err = parse_quick_query(raw).unwrap_err();
            assert!(matches!(err, QueryError::InvalidSearch(_)), "{raw}");
            let json = serde_json::to_value(&err).unwrap();
            assert_eq!(json["kind"], "invalidSearch");
            assert_eq!(json["message"], "IDs are whole numbers from 1 to 2147483647");
            assert!(!err.to_string().contains(raw.trim_start_matches(['#', '!'])), "{raw}");
        }
    }

    #[test]
    fn text_needs_two_characters_and_is_trimmed() {
        assert_eq!(parse_quick_query("").unwrap(), None);
        assert_eq!(parse_quick_query("   ").unwrap(), None);
        assert_eq!(parse_quick_query("a").unwrap(), None);
        assert_eq!(parse_quick_query("#").unwrap(), None);
        assert_eq!(parse_quick_query("  login bug ").unwrap(), Some(QuickQuery::Text("login bug".into())));
        // A prefix followed by text is a title search, not an ID.
        assert_eq!(parse_quick_query("#hotfix").unwrap(), Some(QuickQuery::Text("#hotfix".into())));
        assert_eq!(parse_quick_query("12 monkeys").unwrap(), Some(QuickQuery::Text("12 monkeys".into())));
    }

    #[test]
    fn control_characters_and_long_text_are_rejected() {
        assert!(parse_quick_query("login\nbug").is_err());
        assert!(parse_quick_query("login bug\n").is_err());
        assert!(parse_quick_query(&"a".repeat(MAX_QUERY_CHARS)).is_ok());
        let long = "a".repeat(MAX_QUERY_CHARS + 1);
        let err = parse_quick_query(&long).unwrap_err();
        assert!(!err.to_string().contains(&long));
        assert!(err.to_string().contains(&MAX_QUERY_CHARS.to_string()));
    }

    #[test]
    fn title_wiql_escapes_quotes_and_orders_by_change() {
        assert_eq!(
            title_wiql("Ada's bug").unwrap(),
            "SELECT [System.Id] FROM WorkItems WHERE [System.Title] CONTAINS 'Ada''s bug' \
             ORDER BY [System.ChangedDate] DESC"
        );
    }

    #[test]
    fn title_match_needs_every_word_in_any_case() {
        assert!(title_matches("Fix LOGIN timeout on Safari", "login safari"));
        assert!(title_matches("Fix login", "  LOGIN  "));
        assert!(!title_matches("Fix login", "login safari"));
        assert!(title_matches("Ünicode Größe", "größe"));
    }

    #[test]
    fn pull_requests_are_filtered_active_first_then_newest_and_capped() {
        let prs = vec![
            pr(1, "Login page", PullRequestState::Completed),
            pr(2, "Login API", PullRequestState::Active),
            pr(3, "Unrelated", PullRequestState::Active),
            pr(4, "login retries", PullRequestState::Abandoned),
            pr(5, "Login form", PullRequestState::Active),
        ];
        let ids: Vec<i64> = filter_pull_requests(&prs, "login").iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![5, 2, 4, 1]);

        let many: Vec<PullRequest> =
            (1..=50).map(|id| pr(id, "login", PullRequestState::Active)).collect();
        let found = filter_pull_requests(&many, "login");
        assert_eq!(found.len(), MAX_HITS_PER_KIND);
        assert_eq!(found[0].id, 50);
    }

    #[test]
    fn work_items_keep_wiql_order_and_are_capped() {
        let items: Vec<WorkItem> = (1..=30).rev().map(wi).collect();
        let capped = cap_work_items(items);
        assert_eq!(capped.len(), MAX_HITS_PER_KIND);
        assert_eq!(capped[0].id, 30);
    }

    #[test]
    fn hits_are_tagged_by_kind_with_their_organization() {
        let hit = SearchHit::PullRequest { organization: "contoso".into(), item: pr(7, "Login", PullRequestState::Active) };
        let json = serde_json::to_value(&hit).unwrap();
        assert_eq!(json["kind"], "pullRequest");
        assert_eq!(json["organization"], "contoso");
        assert_eq!(json["item"]["repositoryId"], "repo-1");
        assert_eq!(json["item"]["status"], "active");

        let hit = SearchHit::WorkItem { organization: "contoso".into(), item: wi(3) };
        let json = serde_json::to_value(&hit).unwrap();
        assert_eq!(json["kind"], "workItem");
        assert_eq!(json["item"]["workItemType"], "Bug");
    }

    fn hit(organization: &str, id: i64) -> SearchHit {
        SearchHit::WorkItem { organization: organization.into(), item: wi(id) }
    }

    fn organization_of(hit: &SearchHit) -> &str {
        match hit {
            SearchHit::WorkItem { organization, .. } | SearchHit::PullRequest { organization, .. } => organization,
        }
    }

    #[test]
    fn the_active_organization_is_searched_first() {
        let index = OrganizationIndex {
            active: Some("fabrikam".into()),
            organizations: vec!["contoso".into(), "fabrikam".into(), "tailspin".into()],
        };
        assert_eq!(search_order(&index), vec!["fabrikam", "contoso", "tailspin"]);

        let none_active = OrganizationIndex { active: None, organizations: vec!["contoso".into(), "tailspin".into()] };
        assert_eq!(search_order(&none_active), vec!["contoso", "tailspin"]);

        // An active name missing from the list (stale index) is not searched.
        let stale = OrganizationIndex { active: Some("gone".into()), organizations: vec!["contoso".into()] };
        assert_eq!(search_order(&stale), vec!["contoso"]);
    }

    #[test]
    fn failed_organizations_are_named_and_the_rest_keep_their_order() {
        let results = combine(vec![
            ("fabrikam".into(), Ok(vec![hit("fabrikam", 1), hit("fabrikam", 2)])),
            ("contoso".into(), Err(QueryError::App(AppError::Unauthorized))),
            ("tailspin".into(), Ok(vec![hit("tailspin", 3)])),
            ("woodgrove".into(), Err(QueryError::App(AppError::Forbidden))),
        ]);
        let order: Vec<&str> = results.hits.iter().map(organization_of).collect();
        assert_eq!(order, vec!["fabrikam", "fabrikam", "tailspin"]);
        assert_eq!(results.unavailable, vec!["contoso", "woodgrove"]);
    }
}
