//! Pure rules for notifications: what counts as new between two checks of one organization.
//! The poller fetches; these functions decide, so every rule is unit tested.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::azdo::model::{PullRequest, PullRequestRole, PullRequestState, WorkItem};
use crate::search::SearchHit;

/// Most PRs and work items whose comments are fetched in one check.
pub const MAX_COMMENT_TARGETS: usize = 25;
/// Change marker for work items whose comments must be fetched again.
const RETRY_MARKER: &str = "retry";
/// More new items than this in one check become a single summary notification.
pub const MAX_SEPARATE_NOTIFICATIONS: usize = 3;

/// What one organization looked like at the previous check (persisted).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OrgSeen {
    /// PRs waiting for the user's review.
    pub review_requests: BTreeSet<i64>,
    /// Open work items assigned to the user, with their last change.
    pub assigned: BTreeMap<i64, String>,
    /// Targets (`pr:55`, `wi:101`) whose comments were fetched at least once.
    pub comment_targets: BTreeSet<String>,
    /// Comments already seen (`pr:55:7:1`, `wi:101:3`).
    pub comments: BTreeSet<String>,
}

/// A comment by someone else, as fetched by the poller.
#[derive(Debug, Clone, PartialEq)]
pub struct SeenComment {
    /// Unique key, prefixed by its target key (`pr:55:7:1`).
    pub key: String,
    pub author: String,
}

/// Comments fetched for one target in this check.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetComments {
    pub target: SearchHit,
    pub comments: Vec<SeenComment>,
}

/// Everything fetched for one organization in this check.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Observation {
    /// Active PRs the user created or reviews (from the PR list query).
    pub pull_requests: Vec<PullRequest>,
    /// Open work items assigned to the user.
    pub assigned: Vec<WorkItem>,
    pub comments: Vec<TargetComments>,
    /// Assigned work items whose comments should have been fetched but could not be.
    pub unchecked_work_items: Vec<i64>,
}

/// Which signals the user wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signals {
    pub review_requests: bool,
    pub assignments: bool,
    pub comments: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NewKind {
    ReviewRequest,
    Assignment,
    Comment,
}

/// Something new to tell the user about.
#[derive(Debug, Clone, PartialEq)]
pub struct NewItem {
    pub kind: NewKind,
    pub target: SearchHit,
    /// Comment authors (deduplicated, in order), empty for other kinds.
    pub authors: Vec<String>,
    pub count: usize,
}

pub fn pr_target(id: i64) -> String {
    format!("pr:{id}")
}

pub fn wi_target(id: i64) -> String {
    format!("wi:{id}")
}

/// An active, non-draft PR the user reviews (not authors) and has not voted on.
pub fn is_review_request(pr: &PullRequest) -> bool {
    pr.role == PullRequestRole::Reviewer
        && pr.my_vote.unwrap_or(0) == 0
        && pr.status == PullRequestState::Active
        && !pr.is_draft
}

fn is_authored(pr: &PullRequest) -> bool {
    pr.role == PullRequestRole::Author && pr.status == PullRequestState::Active
}

/// `wi:10` for `wi:10:3`: the target a comment key belongs to.
fn target_of(key: &str) -> &str {
    key.match_indices(':').nth(1).map_or(key, |(i, _)| &key[..i])
}

fn target_key(hit: &SearchHit) -> String {
    match hit {
        SearchHit::WorkItem { item, .. } => wi_target(item.id),
        SearchHit::PullRequest { item, .. } => pr_target(item.id),
    }
}

/// Active PRs the user created, whose threads are checked for comments (capped, newest first).
pub fn authored_to_check(pull_requests: &[PullRequest]) -> Vec<&PullRequest> {
    let mut authored: Vec<&PullRequest> = pull_requests.iter().filter(|pr| is_authored(pr)).collect();
    // Higher IDs are newer pull requests.
    authored.sort_by_key(|pr| std::cmp::Reverse(pr.id));
    authored.truncate(MAX_COMMENT_TARGETS);
    authored
}

/// Assigned work items whose comments should be fetched: changed since the previous check or
/// not tracked yet; on the first check (`previous` is `None`) the most recent ones, to seed.
pub fn work_items_to_check<'a>(previous: Option<&OrgSeen>, assigned: &'a [WorkItem]) -> Vec<&'a WorkItem> {
    // `assigned` comes newest change first, so the cap keeps the most active items.
    assigned
        .iter()
        .filter(|item| {
            previous.is_none_or(|seen| seen.assigned.get(&item.id) != Some(&changed(item)))
        })
        .take(MAX_COMMENT_TARGETS)
        .collect()
}

fn changed(item: &WorkItem) -> String {
    item.changed_at.clone().unwrap_or_default()
}

/// Compares this check with the previous one. Returns the state to persist and what is new.
/// The first check of an organization (`previous` is `None`) only records state.
pub fn diff(
    organization: &str,
    previous: Option<&OrgSeen>,
    observed: &Observation,
    signals: Signals,
) -> (OrgSeen, Vec<NewItem>) {
    let reviews: Vec<&PullRequest> = observed.pull_requests.iter().filter(|pr| is_review_request(pr)).collect();
    let current_targets: BTreeSet<String> = observed
        .pull_requests
        .iter()
        .filter(|pr| is_authored(pr))
        .map(|pr| pr_target(pr.id))
        .chain(observed.assigned.iter().map(|item| wi_target(item.id)))
        .collect();

    let mut next = OrgSeen {
        review_requests: reviews.iter().map(|pr| pr.id).collect(),
        assigned: observed.assigned.iter().map(|item| (item.id, changed(item))).collect(),
        comment_targets: BTreeSet::new(),
        comments: BTreeSet::new(),
    };
    // Work items whose comments could not be read keep their previous change marker (or a
    // marker that never matches), so the next check fetches them again.
    for id in &observed.unchecked_work_items {
        if next.assigned.contains_key(id) {
            let before = previous.and_then(|seen| seen.assigned.get(id).cloned());
            next.assigned.insert(*id, before.unwrap_or_else(|| RETRY_MARKER.to_string()));
        }
    }
    // Comment state only survives for targets that are still current.
    if let Some(seen) = previous {
        next.comment_targets = seen.comment_targets.intersection(&current_targets).cloned().collect();
        next.comments = seen
            .comments
            .iter()
            .filter(|key| current_targets.contains(target_of(key)))
            .cloned()
            .collect();
    }

    let mut new = Vec::new();
    let pr_hit = |pr: &PullRequest| SearchHit::PullRequest { organization: organization.to_string(), item: pr.clone() };
    let wi_hit = |item: &WorkItem| SearchHit::WorkItem { organization: organization.to_string(), item: item.clone() };

    if let Some(seen) = previous {
        if signals.review_requests {
            new.extend(reviews.iter().filter(|pr| !seen.review_requests.contains(&pr.id)).map(|pr| NewItem {
                kind: NewKind::ReviewRequest,
                target: pr_hit(pr),
                authors: Vec::new(),
                count: 1,
            }));
        }
        if signals.assignments {
            new.extend(observed.assigned.iter().filter(|item| !seen.assigned.contains_key(&item.id)).map(|item| {
                NewItem { kind: NewKind::Assignment, target: wi_hit(item), authors: Vec::new(), count: 1 }
            }));
        }
    }

    for fetched in &observed.comments {
        let target = target_key(&fetched.target);
        if !current_targets.contains(&target) {
            continue;
        }
        // Only targets whose comments were fetched before can have *new* comments; the first
        // fetch of a target just records what is already there.
        let known = previous.is_some_and(|seen| seen.comment_targets.contains(&target));
        let fresh: Vec<&SeenComment> = fetched
            .comments
            .iter()
            .filter(|c| previous.is_none_or(|seen| !seen.comments.contains(&c.key)))
            .collect();
        if known && signals.comments && !fresh.is_empty() {
            let mut authors: Vec<String> = Vec::new();
            for comment in &fresh {
                if !authors.contains(&comment.author) {
                    authors.push(comment.author.clone());
                }
            }
            new.push(NewItem { kind: NewKind::Comment, target: fetched.target.clone(), authors, count: fresh.len() });
        }
        next.comments.extend(fetched.comments.iter().map(|c| c.key.clone()));
        next.comment_targets.insert(target);
    }

    (next, new)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: Signals = Signals { review_requests: true, assignments: true, comments: true };

    fn pr(id: i64, role: PullRequestRole, vote: Option<i64>) -> PullRequest {
        PullRequest {
            id,
            title: format!("PR {id}"),
            project: "Web".into(),
            repository: "site".into(),
            repository_id: "r1".into(),
            source_branch: "feature".into(),
            target_branch: "main".into(),
            author: "Ada".into(),
            created_at: "2026-11-01T00:00:00Z".into(),
            is_draft: false,
            role,
            my_vote: vote,
            approvals: 0,
            reviewer_count: 1,
            url: format!("https://dev.azure.com/contoso/Web/_git/site/pullrequest/{id}"),
            status: PullRequestState::Active,
            closed_at: None,
        }
    }

    fn wi(id: i64, changed: &str) -> WorkItem {
        WorkItem {
            id,
            title: format!("Item {id}"),
            work_item_type: "Bug".into(),
            state: "Active".into(),
            project: "Web".into(),
            priority: None,
            changed_at: Some(changed.into()),
            url: format!("https://dev.azure.com/contoso/Web/_workitems/edit/{id}"),
        }
    }

    fn wi_hit(item: WorkItem) -> SearchHit {
        SearchHit::WorkItem { organization: "contoso".into(), item }
    }

    fn pr_hit(item: PullRequest) -> SearchHit {
        SearchHit::PullRequest { organization: "contoso".into(), item }
    }

    fn comment(key: &str, author: &str) -> SeenComment {
        SeenComment { key: key.into(), author: author.into() }
    }

    fn ids(items: &[NewItem], kind: NewKind) -> Vec<i64> {
        items
            .iter()
            .filter(|i| i.kind == kind)
            .map(|i| match &i.target {
                SearchHit::WorkItem { item, .. } => item.id,
                SearchHit::PullRequest { item, .. } => item.id,
            })
            .collect()
    }

    #[test]
    fn review_requests_exclude_own_drafts_voted_and_closed_prs() {
        assert!(is_review_request(&pr(1, PullRequestRole::Reviewer, None)));
        assert!(is_review_request(&pr(2, PullRequestRole::Reviewer, Some(0))));
        assert!(!is_review_request(&pr(3, PullRequestRole::Author, None)));
        assert!(!is_review_request(&pr(4, PullRequestRole::Reviewer, Some(10))));
        assert!(!is_review_request(&pr(5, PullRequestRole::Reviewer, Some(-5))));
        let mut draft = pr(6, PullRequestRole::Reviewer, None);
        draft.is_draft = true;
        assert!(!is_review_request(&draft));
        let mut done = pr(7, PullRequestRole::Reviewer, None);
        done.status = PullRequestState::Completed;
        assert!(!is_review_request(&done));
    }

    #[test]
    fn the_first_check_records_state_without_notifying() {
        let observed = Observation {
            pull_requests: vec![pr(1, PullRequestRole::Reviewer, None), pr(2, PullRequestRole::Author, None)],
            assigned: vec![wi(10, "a")],
            comments: vec![TargetComments { target: wi_hit(wi(10, "a")), comments: vec![comment("wi:10:1", "Bo")] }],
            ..Observation::default()
        };
        let (seen, new) = diff("contoso", None, &observed, ON);
        assert!(new.is_empty());
        assert_eq!(seen.review_requests, BTreeSet::from([1]));
        assert_eq!(seen.assigned, BTreeMap::from([(10, "a".to_string())]));
        assert_eq!(seen.comment_targets, BTreeSet::from(["wi:10".to_string()]));
        assert_eq!(seen.comments, BTreeSet::from(["wi:10:1".to_string()]));
    }

    #[test]
    fn new_review_requests_and_assignments_are_reported_once() {
        let first = Observation {
            pull_requests: vec![pr(1, PullRequestRole::Reviewer, None)],
            assigned: vec![wi(10, "a")],
            comments: vec![],
            ..Observation::default()
        };
        let (seen, _) = diff("contoso", None, &first, ON);
        let second = Observation {
            pull_requests: vec![pr(1, PullRequestRole::Reviewer, None), pr(2, PullRequestRole::Reviewer, None)],
            assigned: vec![wi(10, "a"), wi(11, "b")],
            comments: vec![],
            ..Observation::default()
        };
        let (seen, new) = diff("contoso", Some(&seen), &second, ON);
        assert_eq!(ids(&new, NewKind::ReviewRequest), vec![2]);
        assert_eq!(ids(&new, NewKind::Assignment), vec![11]);
        assert!(new.iter().all(|i| i.count == 1));

        let (_, again) = diff("contoso", Some(&seen), &second, ON);
        assert!(again.is_empty());
    }

    #[test]
    fn disabled_signals_still_update_state_but_report_nothing() {
        let (seen, _) = diff("contoso", None, &Observation::default(), ON);
        let observed = Observation {
            pull_requests: vec![pr(2, PullRequestRole::Reviewer, None)],
            assigned: vec![wi(11, "b")],
            comments: vec![],
            ..Observation::default()
        };
        let off = Signals { review_requests: false, assignments: false, comments: false };
        let (next, new) = diff("contoso", Some(&seen), &observed, off);
        assert!(new.is_empty());
        assert!(next.review_requests.contains(&2));
        assert!(next.assigned.contains_key(&11));
    }

    #[test]
    fn comments_on_known_targets_are_grouped_and_first_fetches_only_seed() {
        let item = wi(10, "a");
        let mine = pr(55, PullRequestRole::Author, None);
        let first = Observation {
            pull_requests: vec![mine.clone()],
            assigned: vec![item.clone()],
            comments: vec![TargetComments { target: wi_hit(item.clone()), comments: vec![comment("wi:10:1", "Bo")] }],
            ..Observation::default()
        };
        let (seen, _) = diff("contoso", None, &first, ON);
        let second = Observation {
            pull_requests: vec![mine.clone()],
            assigned: vec![wi(10, "b")],
            comments: vec![
                TargetComments {
                    target: wi_hit(wi(10, "b")),
                    comments: vec![comment("wi:10:1", "Bo"), comment("wi:10:2", "Cy"), comment("wi:10:3", "Cy")],
                },
                // First fetch of this PR's threads: seeds, does not notify.
                TargetComments { target: pr_hit(mine.clone()), comments: vec![comment("pr:55:1:1", "Di")] },
            ],
            ..Observation::default()
        };
        let (seen, new) = diff("contoso", Some(&seen), &second, ON);
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].kind, NewKind::Comment);
        assert_eq!(new[0].count, 2);
        assert_eq!(new[0].authors, vec!["Cy".to_string()]);
        assert!(seen.comment_targets.contains("pr:55"));

        let third = Observation {
            pull_requests: vec![mine.clone()],
            assigned: vec![wi(10, "b")],
            comments: vec![TargetComments {
                target: pr_hit(mine),
                comments: vec![comment("pr:55:1:1", "Di"), comment("pr:55:1:2", "Di"), comment("pr:55:2:1", "Ed")],
            }],
            ..Observation::default()
        };
        let (_, new) = diff("contoso", Some(&seen), &third, ON);
        assert_eq!(ids(&new, NewKind::Comment), vec![55]);
        assert_eq!(new[0].authors, vec!["Di".to_string(), "Ed".to_string()]);
        assert_eq!(new[0].count, 2);
    }

    #[test]
    fn work_items_whose_comments_could_not_be_read_are_retried() {
        let (seen, _) = diff("contoso", None, &Observation { assigned: vec![wi(10, "a")], ..Observation::default() }, ON);
        let changed = vec![wi(10, "b"), wi(11, "x")];
        let failed = Observation { assigned: changed.clone(), unchecked_work_items: vec![10, 11], ..Observation::default() };
        let (seen, new) = diff("contoso", Some(&seen), &failed, ON);
        // Item 11 is still reported as newly assigned, once.
        assert_eq!(ids(&new, NewKind::Assignment), vec![11]);
        let retry: Vec<i64> = work_items_to_check(Some(&seen), &changed).iter().map(|w| w.id).collect();
        assert_eq!(retry, vec![10, 11]);
        let (_, again) = diff("contoso", Some(&seen), &Observation { assigned: changed, ..Observation::default() }, ON);
        assert!(again.is_empty());
    }

    #[test]
    fn state_of_items_no_longer_current_is_dropped() {
        let first = Observation {
            pull_requests: vec![pr(55, PullRequestRole::Author, None)],
            assigned: vec![wi(10, "a")],
            comments: vec![
                TargetComments { target: wi_hit(wi(10, "a")), comments: vec![comment("wi:10:1", "Bo")] },
                TargetComments { target: pr_hit(pr(55, PullRequestRole::Author, None)), comments: vec![comment("pr:55:1:1", "Bo")] },
            ],
            ..Observation::default()
        };
        let (seen, _) = diff("contoso", None, &first, ON);
        let (next, _) = diff("contoso", Some(&seen), &Observation::default(), ON);
        assert_eq!(next, OrgSeen::default());
    }

    #[test]
    fn comment_checks_cover_changed_or_new_items_and_seed_the_first_time() {
        let assigned: Vec<WorkItem> = (1..=30).map(|id| wi(id, "a")).collect();
        assert_eq!(work_items_to_check(None, &assigned).len(), MAX_COMMENT_TARGETS);

        let seen = OrgSeen { assigned: BTreeMap::from([(1, "a".into()), (2, "a".into())]), ..OrgSeen::default() };
        let now = vec![wi(1, "a"), wi(2, "b"), wi(3, "a")];
        let checked: Vec<i64> = work_items_to_check(Some(&seen), &now).iter().map(|w| w.id).collect();
        assert_eq!(checked, vec![2, 3]);

        let prs: Vec<PullRequest> = (1..=30)
            .map(|id| pr(id, if id % 2 == 0 { PullRequestRole::Author } else { PullRequestRole::Reviewer }, None))
            .collect();
        let authored: Vec<i64> = authored_to_check(&prs).iter().map(|p| p.id).collect();
        assert_eq!(authored.len(), 15);
        assert_eq!(authored[0], 30);
    }
}
