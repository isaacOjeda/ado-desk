//! Every write the assistant asks for waits for the user's confirmation in the main window,
//! whatever permission mode the Claude Code session runs in. A request shows the exact action,
//! target and text; nothing is sent until the user approves it there.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::oneshot;

use crate::error::{AppError, AppResult};

/// Emitted to the main window with a [`WriteRequest`].
pub const CONFIRM_EVENT: &str = "assistant-confirm";
/// Emitted with the request id once it is answered or expired, so the dialog closes.
pub const CONFIRM_CLOSED_EVENT: &str = "assistant-confirm-closed";
/// How long a request waits for the user.
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// What the dialog shows.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WriteRequest {
    pub id: u64,
    /// "Reply to a pull request comment thread", "Comment on a work item".
    pub action: String,
    /// Where it goes, by ids: "Pull request 55 (project Web), thread 7".
    pub target: String,
    pub organization: String,
    /// The exact text that will be posted.
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Approved,
    Declined,
    /// Unanswered in time, or the window went away.
    Expired,
}

#[derive(Default)]
pub struct Confirmations {
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<bool>>>,
}

impl Confirmations {
    /// Registers a request; the receiver resolves when [`Confirmations::answer`] runs.
    pub fn open(&self) -> (u64, oneshot::Receiver<bool>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);
        (id, rx)
    }

    /// Delivers the user's answer; false when the request is unknown or already settled.
    pub fn answer(&self, id: u64, approved: bool) -> bool {
        let sender = self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        sender.is_some_and(|tx| tx.send(approved).is_ok())
    }

    fn forget(&self, id: u64) {
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    }
}

/// Waits for an answer, up to `timeout`.
pub async fn wait(rx: oneshot::Receiver<bool>, timeout: Duration) -> Outcome {
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(true)) => Outcome::Approved,
        Ok(Ok(false)) => Outcome::Declined,
        _ => Outcome::Expired,
    }
}

/// Shows the request in the main window (bringing it forward) and waits for the user.
pub async fn ask(app: &AppHandle, mut request: WriteRequest) -> Outcome {
    let confirmations = app.state::<Confirmations>();
    let (id, rx) = confirmations.open();
    request.id = id;
    crate::background::show_main(app);
    if app.emit_to(crate::background::MAIN_WINDOW, CONFIRM_EVENT, &request).is_err() {
        confirmations.forget(id);
        return Outcome::Expired;
    }
    let outcome = wait(rx, CONFIRM_TIMEOUT).await;
    confirmations.forget(id);
    let _ = app.emit_to(crate::background::MAIN_WINDOW, CONFIRM_CLOSED_EVENT, id);
    outcome
}

/// The user's answer to a write request.
#[tauri::command]
pub fn assistant_confirm(confirmations: State<'_, Confirmations>, id: u64, approved: bool) -> AppResult<()> {
    if confirmations.answer(id, approved) {
        Ok(())
    } else {
        Err(AppError::Assistant("This request is no longer waiting for an answer"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_answer_settles_its_request_once() {
        let confirmations = Confirmations::default();
        let (approve_id, approve_rx) = confirmations.open();
        let (decline_id, decline_rx) = confirmations.open();
        assert_ne!(approve_id, decline_id);

        assert!(confirmations.answer(approve_id, true));
        assert!(confirmations.answer(decline_id, false));
        assert_eq!(wait(approve_rx, Duration::from_secs(1)).await, Outcome::Approved);
        assert_eq!(wait(decline_rx, Duration::from_secs(1)).await, Outcome::Declined);
        // A second answer, or one for a request that never existed, does nothing.
        assert!(!confirmations.answer(approve_id, true));
        assert!(!confirmations.answer(999, true));
    }

    #[tokio::test]
    async fn an_unanswered_request_expires() {
        let confirmations = Confirmations::default();
        let (id, rx) = confirmations.open();
        assert_eq!(wait(rx, Duration::from_millis(20)).await, Outcome::Expired);
        confirmations.forget(id);
        assert!(!confirmations.answer(id, true), "an expired request cannot be approved later");
    }

    #[tokio::test]
    async fn a_dropped_request_expires() {
        let confirmations = Confirmations::default();
        let (id, rx) = confirmations.open();
        confirmations.forget(id);
        assert_eq!(wait(rx, Duration::from_secs(1)).await, Outcome::Expired);
    }
}
