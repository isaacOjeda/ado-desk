//! Inbox items and per-organization seen state, persisted as JSON in the app config dir.
//! Loading never fails (missing or corrupt files start empty) and saving is atomic.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::rules::{MAX_SEPARATE_NOTIFICATIONS, NewItem, NewKind, OrgSeen};
use crate::search::SearchHit;
use crate::settings::NotificationSettings;

pub const INBOX_FILE: &str = "inbox.json";
pub const STATE_FILE: &str = "notification-state.json";
pub const MAX_INBOX: usize = 100;
/// Longest item title kept in notifications and the Inbox.
const MAX_TITLE_CHARS: usize = 160;

/// One Inbox entry. `target` opens the item like a quick search result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxItem {
    pub id: String,
    pub kind: NewKind,
    pub title: String,
    pub detail: String,
    /// Unix time in milliseconds.
    pub at: u64,
    pub read: bool,
    pub target: SearchHit,
}

/// Seen state of every organization.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeenState {
    pub organizations: BTreeMap<String, OrgSeen>,
}

fn short(text: &str) -> String {
    let clean: String = text.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() <= MAX_TITLE_CHARS {
        clean
    } else {
        format!("{}…", clean.chars().take(MAX_TITLE_CHARS - 1).collect::<String>())
    }
}

fn label(hit: &SearchHit) -> String {
    match hit {
        SearchHit::WorkItem { item, .. } => short(&format!("{} {} {}", item.work_item_type, item.id, item.title)),
        SearchHit::PullRequest { item, .. } => short(&format!("!{} {}", item.id, item.title)),
    }
}

fn organization(hit: &SearchHit) -> &str {
    match hit {
        SearchHit::WorkItem { organization, .. } | SearchHit::PullRequest { organization, .. } => organization,
    }
}

/// Inbox entries for what one check found. `now_ms` stamps them and keeps ids unique.
pub fn to_inbox(new: &[NewItem], now_ms: u64) -> Vec<InboxItem> {
    new.iter()
        .enumerate()
        .map(|(index, found)| {
            let org = organization(&found.target);
            let item = label(&found.target);
            let (title, detail) = match found.kind {
                NewKind::ReviewRequest => {
                    let detail = match &found.target {
                        SearchHit::PullRequest { item, .. } => format!("{} · {} · {org}", item.repository, item.project),
                        SearchHit::WorkItem { item, .. } => format!("{} · {org}", item.project),
                    };
                    (format!("Review requested: {item}"), detail)
                }
                NewKind::Assignment => {
                    let detail = match &found.target {
                        SearchHit::WorkItem { item, .. } => format!("{} · {} · {org}", item.state, item.project),
                        SearchHit::PullRequest { item, .. } => format!("{} · {org}", item.project),
                    };
                    (format!("Assigned to you: {item}"), detail)
                }
                NewKind::Comment => {
                    let title = if found.count == 1 {
                        format!("New comment on {item}")
                    } else {
                        format!("{} new comments on {item}", found.count)
                    };
                    (title, short(&format!("From {} · {org}", found.authors.join(", "))))
                }
            };
            let target = match &found.target {
                SearchHit::WorkItem { item, .. } => format!("wi:{}", item.id),
                SearchHit::PullRequest { item, .. } => format!("pr:{}", item.id),
            };
            InboxItem {
                id: format!("{org}/{target}/{:?}/{now_ms}/{index}", found.kind),
                kind: found.kind,
                title: short(&title),
                detail: short(&detail),
                at: now_ms,
                read: false,
                target: found.target.clone(),
            }
        })
        .collect()
}

/// New items first, then the existing ones, at most [`MAX_INBOX`].
pub fn merge(existing: &[InboxItem], new: Vec<InboxItem>) -> Vec<InboxItem> {
    let mut merged = new;
    merged.extend(existing.iter().cloned());
    merged.truncate(MAX_INBOX);
    merged
}

/// Marks one item (or every item when `id` is `None`) as read.
pub fn mark_read(items: &[InboxItem], id: Option<&str>) -> Vec<InboxItem> {
    items
        .iter()
        .cloned()
        .map(|mut item| {
            if id.is_none_or(|id| id == item.id) {
                item.read = true;
            }
            item
        })
        .collect()
}

pub fn unread(items: &[InboxItem]) -> usize {
    items.iter().filter(|item| !item.read).count()
}

/// State to keep after the notification settings change from `was` to `now`. Turning
/// notifications (or comment checks) back on starts from a fresh baseline, so whatever happened
/// while they were off is recorded, not reported.
pub fn forget_for(seen: &SeenState, was: Option<&NotificationSettings>, now: &NotificationSettings) -> SeenState {
    let Some(was) = was else { return seen.clone() };
    if now.enabled && !was.enabled {
        return SeenState::default();
    }
    let mut next = seen.clone();
    if now.comments && !was.comments {
        for org in next.organizations.values_mut() {
            org.comment_targets.clear();
            org.comments.clear();
        }
    }
    next
}

/// Native notifications for one check: one per item, or a single summary above
/// [`MAX_SEPARATE_NOTIFICATIONS`]. Returns `(title, body)` pairs.
pub fn notifications(items: &[InboxItem]) -> Vec<(String, String)> {
    if items.len() > MAX_SEPARATE_NOTIFICATIONS {
        return vec![("ADO Desk".to_string(), format!("{} new items in your Inbox", items.len()))];
    }
    items.iter().map(|item| (item.title.clone(), item.detail.clone())).collect()
}

pub fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

pub fn save<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    crate::settings::write_atomic(path, &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azdo::model::{PullRequest, PullRequestRole, PullRequestState, WorkItem};

    fn pr_hit(id: i64, title: &str) -> SearchHit {
        SearchHit::PullRequest {
            organization: "contoso".into(),
            item: PullRequest {
                id,
                title: title.into(),
                project: "Web".into(),
                repository: "site".into(),
                repository_id: "r1".into(),
                source_branch: "feature".into(),
                target_branch: "main".into(),
                author: "Ada".into(),
                created_at: "2026-11-01T00:00:00Z".into(),
                is_draft: false,
                role: PullRequestRole::Reviewer,
                my_vote: None,
                approvals: 0,
                reviewer_count: 1,
                url: String::new(),
                status: PullRequestState::Active,
                closed_at: None,
            },
        }
    }

    fn wi_hit(id: i64, title: &str) -> SearchHit {
        SearchHit::WorkItem {
            organization: "fabrikam".into(),
            item: WorkItem {
                id,
                title: title.into(),
                work_item_type: "Bug".into(),
                state: "Active".into(),
                project: "Ops".into(),
                priority: None,
                changed_at: None,
                url: String::new(),
            },
        }
    }

    fn new(kind: NewKind, target: SearchHit, authors: &[&str], count: usize) -> NewItem {
        NewItem { kind, target, authors: authors.iter().map(|a| a.to_string()).collect(), count }
    }

    #[test]
    fn inbox_text_names_the_item_and_its_organization() {
        let items = to_inbox(
            &[
                new(NewKind::ReviewRequest, pr_hit(55, "Fix login"), &[], 1),
                new(NewKind::Assignment, wi_hit(101, "Crash on save"), &[], 1),
                new(NewKind::Comment, pr_hit(55, "Fix login"), &["Cy", "Di"], 3),
                new(NewKind::Comment, wi_hit(101, "Crash on save"), &["Bo"], 1),
            ],
            1_000,
        );
        let text: Vec<(&str, &str)> = items.iter().map(|i| (i.title.as_str(), i.detail.as_str())).collect();
        assert_eq!(
            text,
            vec![
                ("Review requested: !55 Fix login", "site · Web · contoso"),
                ("Assigned to you: Bug 101 Crash on save", "Active · Ops · fabrikam"),
                ("3 new comments on !55 Fix login", "From Cy, Di · contoso"),
                ("New comment on Bug 101 Crash on save", "From Bo · fabrikam"),
            ]
        );
        assert!(items.iter().all(|i| !i.read && i.at == 1_000));
        let mut ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        ids.dedup();
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn long_or_multiline_titles_are_shortened_to_one_line() {
        let items = to_inbox(&[new(NewKind::ReviewRequest, pr_hit(1, &format!("a\nb{}", "x".repeat(400))), &[], 1)], 1);
        assert!(!items[0].title.contains('\n'));
        assert!(items[0].title.chars().count() <= MAX_TITLE_CHARS);
        assert!(items[0].title.ends_with('…'));
    }

    #[test]
    fn merge_puts_new_items_first_and_caps_the_inbox() {
        let old = to_inbox(&(1..=MAX_INBOX as i64).map(|id| new(NewKind::ReviewRequest, pr_hit(id, "old"), &[], 1)).collect::<Vec<_>>(), 1);
        let fresh = to_inbox(&[new(NewKind::Assignment, wi_hit(7, "new"), &[], 1)], 2);
        let merged = merge(&old, fresh);
        assert_eq!(merged.len(), MAX_INBOX);
        assert_eq!(merged[0].kind, NewKind::Assignment);
        assert_eq!(merged[1], old[0]);
    }

    #[test]
    fn items_are_marked_read_one_by_one_or_all_at_once() {
        let items = to_inbox(
            &[new(NewKind::ReviewRequest, pr_hit(1, "a"), &[], 1), new(NewKind::ReviewRequest, pr_hit(2, "b"), &[], 1)],
            1,
        );
        let one = mark_read(&items, Some(&items[1].id));
        assert_eq!(unread(&one), 1);
        assert!(one[1].read);
        assert_eq!(unread(&mark_read(&items, None)), 0);
        assert_eq!(mark_read(&items, Some("missing")), items);
    }

    #[test]
    fn more_than_three_items_become_one_summary_notification() {
        let few = to_inbox(&[new(NewKind::ReviewRequest, pr_hit(1, "a"), &[], 1)], 1);
        assert_eq!(notifications(&few), vec![("Review requested: !1 a".to_string(), "site · Web · contoso".to_string())]);
        let many = to_inbox(&(1..=4).map(|id| new(NewKind::ReviewRequest, pr_hit(id, "a"), &[], 1)).collect::<Vec<_>>(), 1);
        assert_eq!(notifications(&many), vec![("ADO Desk".to_string(), "4 new items in your Inbox".to_string())]);
        assert!(notifications(&[]).is_empty());
    }

    fn seen_with_comments() -> SeenState {
        let org = OrgSeen {
            review_requests: [1].into(),
            comment_targets: ["wi:10".to_string()].into(),
            comments: ["wi:10:1".to_string()].into(),
            ..OrgSeen::default()
        };
        SeenState { organizations: [("contoso".to_string(), org)].into() }
    }

    #[test]
    fn turning_notifications_back_on_starts_from_a_fresh_baseline() {
        let on = NotificationSettings::default();
        let off = NotificationSettings { enabled: false, ..on.clone() };
        assert_eq!(forget_for(&seen_with_comments(), Some(&off), &on), SeenState::default());
        // Startup and unrelated changes keep the state.
        assert_eq!(forget_for(&seen_with_comments(), None, &on), seen_with_comments());
        let slower = NotificationSettings { interval_minutes: 30, ..on.clone() };
        assert_eq!(forget_for(&seen_with_comments(), Some(&on), &slower), seen_with_comments());
    }

    #[test]
    fn turning_comments_back_on_forgets_only_comment_state() {
        let on = NotificationSettings::default();
        let no_comments = NotificationSettings { comments: false, ..on.clone() };
        let next = forget_for(&seen_with_comments(), Some(&no_comments), &on);
        let org = &next.organizations["contoso"];
        assert!(org.comment_targets.is_empty() && org.comments.is_empty());
        assert_eq!(org.review_requests, [1].into());
    }

    #[test]
    fn inbox_details_are_one_line() {
        let mut hit = pr_hit(1, "a");
        if let SearchHit::PullRequest { item, .. } = &mut hit {
            item.repository = "re\npo".into();
        }
        let items = to_inbox(&[new(NewKind::ReviewRequest, hit, &[], 1)], 1);
        assert!(!items[0].detail.contains('\n'));
    }

    #[test]
    fn missing_or_corrupt_files_load_empty_and_saves_round_trip() {
        let dir = std::env::temp_dir().join(format!("ado-desk-inbox-{}-{}", std::process::id(), line!()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(INBOX_FILE);
        assert_eq!(load::<Vec<InboxItem>>(&path), Vec::new());
        fs::write(&path, "{ nope").unwrap();
        assert_eq!(load::<Vec<InboxItem>>(&path), Vec::new());
        let items = to_inbox(&[new(NewKind::Comment, wi_hit(3, "c"), &["Bo"], 1)], 9);
        save(&path, &items).unwrap();
        assert_eq!(load::<Vec<InboxItem>>(&path), items);
        let _ = fs::remove_dir_all(&dir);
    }
}
