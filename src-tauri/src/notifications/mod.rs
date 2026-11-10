//! Background checks for what needs the user: review requests, assignments and comments.
//! A loop checks every configured organization on the interval from Settings, records what it
//! saw, adds new items to the Inbox and shows native notifications and unread badges.

pub mod fetch;
pub mod rules;
pub mod store;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::FutureExt;
use futures::future::BoxFuture;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Notify;

use crate::AppState;
use crate::azdo::AzdoClient;
use crate::error::AppResult;
use crate::background::{MAIN_WINDOW, TRAY_ID};
use crate::settings::{NotificationSettings, SettingsState};
use rules::Signals;
use store::{InboxItem, SeenState};

/// Sent to every window with the full Inbox after it changes.
pub const INBOX_CHANGED_EVENT: &str = "inbox-changed";
/// Lets the app restore its session before the first check.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(15);
/// Longest one organization's check may take.
const ORGANIZATION_DEADLINE: Duration = Duration::from_secs(60);

pub struct NotificationsState {
    inbox_path: PathBuf,
    state_path: PathBuf,
    inbox: Mutex<Vec<InboxItem>>,
    seen: Mutex<SeenState>,
    /// Wakes the loop early (settings changed, "Check now").
    wake: Notify,
    /// One check at a time.
    checking: tokio::sync::Mutex<()>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl NotificationsState {
    pub fn load(config_dir: &Path) -> Self {
        let inbox_path = config_dir.join(store::INBOX_FILE);
        let state_path = config_dir.join(store::STATE_FILE);
        Self {
            inbox: Mutex::new(store::load(&inbox_path)),
            seen: Mutex::new(store::load(&state_path)),
            inbox_path,
            state_path,
            wake: Notify::new(),
            checking: tokio::sync::Mutex::new(()),
        }
    }

    /// Wakes the loop so it rereads the settings (and checks now when enabled).
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    fn inbox(&self) -> Vec<InboxItem> {
        locked(&self.inbox).clone()
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Broadcasts the Inbox and updates the unread badges: the Dock and menu bar icon on macOS,
/// a taskbar overlay dot on Windows (which supports neither badge counts nor tray titles),
/// and the tray tooltip everywhere.
pub fn publish(app: &AppHandle, inbox: &[InboxItem]) {
    let _ = app.emit(INBOX_CHANGED_EVENT, inbox);
    let unread = store::unread(inbox);
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        #[cfg(windows)]
        let _ = window.set_overlay_icon(
            (unread > 0).then(|| tauri::image::Image::new_owned(overlay_dot(OVERLAY_SIZE), OVERLAY_SIZE, OVERLAY_SIZE)),
        );
        #[cfg(not(windows))]
        let _ = window.set_badge_count((unread > 0).then_some(unread as i64));
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tray_tooltip(unread)));
        #[cfg(not(windows))]
        let _ = tray.set_title((unread > 0).then(|| unread.to_string()));
    }
}

/// Tray tooltip; the only unread hint on the Windows tray, which has no title text.
fn tray_tooltip(unread: usize) -> String {
    match unread {
        0 => "ADO Desk".to_string(),
        1 => "ADO Desk: 1 unread".to_string(),
        n => format!("ADO Desk: {n} unread"),
    }
}

/// Side of the Windows taskbar overlay icon, in pixels.
#[cfg_attr(not(windows), allow(dead_code))]
const OVERLAY_SIZE: u32 = 16;

/// RGBA pixels of a filled red dot (`size` x `size`) with an anti-aliased edge, used as the
/// Windows taskbar overlay that flags unread items.
#[cfg_attr(not(windows), allow(dead_code))]
fn overlay_dot(size: u32) -> Vec<u8> {
    const RED: [u8; 3] = [0xE8, 0x11, 0x23];
    let center = size as f32 / 2.0;
    let radius = center - 1.0;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let coverage = (radius + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
            rgba.extend_from_slice(&RED);
            rgba.push((coverage * 255.0).round() as u8);
        }
    }
    rgba
}

fn signals(settings: &NotificationSettings) -> Signals {
    Signals {
        review_requests: settings.review_requests,
        assignments: settings.assignments,
        comments: settings.comments,
    }
}

type Checked = (String, Option<(rules::OrgSeen, Vec<rules::NewItem>)>);

async fn check_one(
    organization: String,
    client: AppResult<Arc<AzdoClient>>,
    previous: &SeenState,
    signals: Signals,
) -> Checked {
    let Ok(client) = client else { return (organization, None) };
    let seen = previous.organizations.get(&organization);
    let observed = tokio::time::timeout(ORGANIZATION_DEADLINE, fetch::observe(&client, seen, signals)).await;
    let outcome = match observed {
        Ok(Ok(observation)) => Some(rules::diff(&organization, seen, &observation, signals)),
        // Failed or slow: keep the previous state and try again next time.
        _ => None,
    };
    (organization, outcome)
}

/// One check of every configured organization.
async fn check(app: &AppHandle, settings: &NotificationSettings) {
    let state = app.state::<NotificationsState>();
    let _running = state.checking.lock().await;
    let app_state = app.state::<AppState>();
    let Ok(index) = app_state.store.index() else { return };
    let order = crate::search::search_order(&index);
    let signals = signals(settings);
    let previous = locked(&state.seen).clone();

    // Connecting has no timeout of its own; a stuck connection must not stall every later check.
    let Ok(clients) =
        tokio::time::timeout(ORGANIZATION_DEADLINE, crate::commands::connected_clients(&app_state, &order)).await
    else {
        return;
    };
    // Boxed futures sidestep a rustc limitation with closures inside `Send` futures.
    let checks: Vec<BoxFuture<'_, Checked>> =
        clients.into_iter().map(|(organization, client)| check_one(organization, client, &previous, signals).boxed()).collect();
    let outcomes = futures::future::join_all(checks).await;

    // Organizations that are no longer configured drop out; failed ones keep their state.
    let mut next = SeenState::default();
    for organization in &order {
        if let Some(seen) = previous.organizations.get(organization) {
            next.organizations.insert(organization.clone(), seen.clone());
        }
    }
    let mut found = Vec::new();
    for (organization, outcome) in outcomes {
        if let Some((seen, new)) = outcome {
            next.organizations.insert(organization, seen);
            found.extend(new);
        }
    }
    if next != previous {
        let mut seen = locked(&state.seen);
        let _ = store::save(&state.state_path, &next);
        *seen = next;
    }
    if found.is_empty() {
        return;
    }

    let items = store::to_inbox(&found, now_ms());
    let inbox = {
        let mut inbox = locked(&state.inbox);
        *inbox = store::merge(&inbox, items.clone());
        let _ = store::save(&state.inbox_path, &*inbox);
        inbox.clone()
    };
    publish(app, &inbox);
    for (title, body) in store::notifications(&items) {
        if let Err(err) = app.notification().builder().title(title).body(body).show() {
            eprintln!("notification not shown: {err}");
        }
    }
}

/// Re-enabling notifications (or comment checks) starts from a fresh baseline.
fn forget_after_change(app: &AppHandle, was: Option<&NotificationSettings>, now: &NotificationSettings) {
    let state = app.state::<NotificationsState>();
    let mut seen = locked(&state.seen);
    let next = store::forget_for(&seen, was, now);
    if next != *seen {
        let _ = store::save(&state.state_path, &next);
        *seen = next;
    }
}

/// Starts the background loop. It rereads the settings before each check, so turning
/// notifications off or changing the interval applies at the next wake.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        let mut last: Option<NotificationSettings> = None;
        loop {
            let settings = app.state::<SettingsState>().current().notifications;
            forget_after_change(&app, last.as_ref(), &settings);
            last = Some(settings.clone());
            if settings.enabled {
                check(&app, &settings).await;
            }
            let wait = Duration::from_secs(u64::from(settings.interval_minutes.max(1)) * 60);
            let state = app.state::<NotificationsState>();
            let sleep = std::pin::pin!(tokio::time::sleep(wait));
            let woken = std::pin::pin!(state.wake.notified());
            futures::future::select(sleep, woken).await;
        }
    });
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_inbox(state: State<'_, NotificationsState>) -> Vec<InboxItem> {
    state.inbox()
}

/// Marks one Inbox item read, or all of them when `id` is omitted; returns the Inbox.
#[tauri::command]
pub fn mark_inbox_read(app: AppHandle, state: State<'_, NotificationsState>, id: Option<String>) -> Vec<InboxItem> {
    let inbox = {
        let mut inbox = locked(&state.inbox);
        *inbox = store::mark_read(&inbox, id.as_deref());
        let _ = store::save(&state.inbox_path, &*inbox);
        inbox.clone()
    };
    publish(&app, &inbox);
    inbox
}

/// Asks the loop to check now (when notifications are on).
#[tauri::command]
pub fn check_notifications_now(state: State<'_, NotificationsState>) {
    state.wake();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_tooltip_shows_the_unread_count() {
        assert_eq!(tray_tooltip(0), "ADO Desk");
        assert_eq!(tray_tooltip(1), "ADO Desk: 1 unread");
        assert_eq!(tray_tooltip(12), "ADO Desk: 12 unread");
    }

    #[test]
    fn overlay_dot_is_an_opaque_disc_with_transparent_corners() {
        let size = OVERLAY_SIZE;
        let pixels = overlay_dot(size);
        assert_eq!(pixels.len(), (size * size * 4) as usize);
        let alpha = |x: u32, y: u32| pixels[((y * size + x) * 4 + 3) as usize];
        assert_eq!(alpha(size / 2, size / 2), 255);
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(size - 1, size - 1), 0);
        assert_eq!(&pixels[0..3], &[0xE8, 0x11, 0x23]);
    }
}
