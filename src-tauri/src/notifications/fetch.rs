//! Fetches what the rules compare: the user's PRs, assigned work items and recent comments of
//! one organization. Comment payloads are reduced to keys and author names here.

use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt, stream};
use serde_json::Value;

use super::rules::{self, Observation, OrgSeen, SeenComment, Signals, TargetComments};
use crate::azdo::AzdoClient;
use crate::azdo::model::{ListResponse, PullRequest, WorkItem};
use crate::error::{AppError, AppResult};
use crate::pull_requests::{self, RawThread};
use crate::queries::{self, PullRequestFilter, PullRequestRoleFilter, PullRequestStatusFilter};
use crate::search::SearchHit;
use crate::work_items::{self, RawCommentList};

/// Concurrent comment requests per organization.
const COMMENT_CONCURRENCY: usize = 4;
/// Newest work item comments read per item.
const WORK_ITEM_COMMENTS: &str = "50";

/// Text comments by someone other than `me` in a PR's threads.
pub fn pr_comments(pr_id: i64, threads: &[RawThread], me: &str) -> Vec<SeenComment> {
    threads
        .iter()
        .filter(|thread| !thread.is_deleted)
        .flat_map(|thread| thread.comments.iter().map(move |comment| (thread.id, comment)))
        .filter(|(_, comment)| {
            !comment.is_deleted && !comment.comment_type.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("system"))
        })
        .filter_map(|(thread, comment)| {
            let author = comment.author.as_ref()?;
            if author.id.eq_ignore_ascii_case(me) {
                return None;
            }
            let name = author.display_name.clone().or_else(|| author.provider_display_name.clone());
            Some(SeenComment {
                key: format!("pr:{pr_id}:{thread}:{}", comment.id),
                author: name.unwrap_or_else(|| "Someone".into()),
            })
        })
        .collect()
}

/// Comments by someone other than `me` on a work item.
pub fn work_item_comments(item_id: i64, list: &RawCommentList, me: &str) -> Vec<SeenComment> {
    list.comments
        .iter()
        .filter(|comment| !comment.is_deleted)
        .filter_map(|comment| {
            let id = comment.id.or(comment.comment_id)?;
            let by = comment.created_by.as_ref()?;
            let by_id = by.get("id").and_then(Value::as_str)?;
            if by_id.eq_ignore_ascii_case(me) {
                return None;
            }
            Some(SeenComment {
                key: format!("wi:{item_id}:{id}"),
                author: work_items::identity_name(by).unwrap_or_else(|| "Someone".into()),
            })
        })
        .collect()
}

async fn pr_target(client: &AzdoClient, pr: &PullRequest) -> AppResult<TargetComments> {
    let url = pull_requests::pr_url(client, &pr.project, &pr.repository_id, pr.id, &["threads"]);
    let threads: ListResponse<RawThread> = client.get_json(url).await?;
    Ok(TargetComments {
        target: SearchHit::PullRequest { organization: client.org().to_string(), item: pr.clone() },
        comments: pr_comments(pr.id, &threads.value, &client.session().user_id),
    })
}

async fn work_item_target(client: &AzdoClient, item: &WorkItem) -> AppResult<TargetComments> {
    let id = item.id.to_string();
    let mut url = client.url_with_version(&[&item.project, "_apis", "wit", "workItems", &id, "comments"], "7.1-preview.4");
    url.query_pairs_mut().append_pair("$top", WORK_ITEM_COMMENTS).append_pair("order", "desc");
    let list: RawCommentList = client.get_json(url).await?;
    Ok(TargetComments {
        target: SearchHit::WorkItem { organization: client.org().to_string(), item: item.clone() },
        comments: work_item_comments(item.id, &list, &client.session().user_id),
    })
}

/// Everything the rules need for one organization. A target whose comments cannot be read is
/// left out (it is simply not seeded or checked this time).
pub async fn observe(client: &AzdoClient, previous: Option<&OrgSeen>, signals: Signals) -> AppResult<Observation> {
    let filter =
        PullRequestFilter { project: None, status: PullRequestStatusFilter::Active, role: PullRequestRoleFilter::Mine };
    let (pull_requests, assigned) = futures::join!(queries::fetch_pull_requests(client, &filter), client.my_work_items());
    let pull_requests = pull_requests.map_err(|err| match err {
        queries::QueryError::App(err) => err,
        other => AppError::Decode(other.to_string()),
    })?;
    let assigned = assigned?;

    let mut comments = Vec::new();
    let mut unchecked_work_items = Vec::new();
    if signals.comments {
        let prs = rules::authored_to_check(&pull_requests);
        let items = rules::work_items_to_check(previous, &assigned);
        // Boxed futures sidestep a rustc limitation with closures inside `Send` streams. Each
        // result carries the work item id (if any) so failed work items are retried next time.
        let fetches: Vec<BoxFuture<'_, (Option<i64>, AppResult<TargetComments>)>> = prs
            .into_iter()
            .map(|pr| pr_target(client, pr).map(|result| (None, result)).boxed())
            .chain(
                items
                    .into_iter()
                    .map(|item| work_item_target(client, item).map(move |result| (Some(item.id), result)).boxed()),
            )
            .collect();
        let results: Vec<(Option<i64>, AppResult<TargetComments>)> =
            stream::iter(fetches).buffer_unordered(COMMENT_CONCURRENCY).collect().await;
        for (work_item, result) in results {
            match (result, work_item) {
                (Ok(target), _) => comments.push(target),
                (Err(_), Some(id)) => unchecked_work_items.push(id),
                (Err(_), None) => {}
            }
        }
    }
    Ok(Observation { pull_requests, assigned, comments, unchecked_work_items })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pr_comments_skip_own_deleted_and_system_comments() {
        let threads: Vec<RawThread> = serde_json::from_value(json!([
            { "id": 7, "comments": [
                { "id": 1, "author": { "id": "me", "displayName": "Me" }, "commentType": "text" },
                { "id": 2, "author": { "id": "bo", "displayName": "Bo" }, "commentType": "text" },
                { "id": 3, "author": { "id": "bo", "displayName": "Bo" }, "commentType": "system" },
                { "id": 4, "author": { "id": "cy", "displayName": "Cy" }, "commentType": "text", "isDeleted": true }
            ]},
            { "id": 8, "isDeleted": true, "comments": [
                { "id": 1, "author": { "id": "di", "displayName": "Di" }, "commentType": "text" }
            ]},
            { "id": 9, "comments": [
                { "id": 1, "author": { "id": "ME", "displayName": "Me" } },
                { "id": 2, "author": { "id": "ed" } }
            ]}
        ]))
        .unwrap();
        let found = pr_comments(55, &threads, "me");
        assert_eq!(
            found,
            vec![
                SeenComment { key: "pr:55:7:2".into(), author: "Bo".into() },
                SeenComment { key: "pr:55:9:2".into(), author: "Someone".into() },
            ]
        );
    }

    #[test]
    fn work_item_comments_skip_own_and_deleted_ones() {
        let list: RawCommentList = serde_json::from_value(json!({ "comments": [
            { "id": 3, "createdBy": { "id": "bo", "displayName": "Bo" } },
            { "commentId": 4, "createdBy": { "id": "me", "displayName": "Me" } },
            { "id": 5, "createdBy": { "id": "cy", "displayName": "Cy" }, "isDeleted": true },
            { "createdBy": { "id": "di", "displayName": "Di" } }
        ]}))
        .unwrap();
        assert_eq!(
            work_item_comments(101, &list, "me"),
            vec![SeenComment { key: "wi:101:3".into(), author: "Bo".into() }]
        );
    }
}
