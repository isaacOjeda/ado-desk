//! One terminal session at a time running `claude` in a PTY. Output streams to the webview
//! through a channel; keystrokes come back through `assistant_write`. Starting a session ends
//! the previous one, so a reloaded webview never leaves an orphan behind.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use super::context::{self, ViewContext};
use super::launch::{self, CLAUDE_NAMES};
use super::server::ServerInfo;
use crate::error::{AppError, AppResult};
use crate::settings::SettingsState;

/// Largest chunk of keystrokes or pasted text accepted in one `assistant_write`.
pub const MAX_WRITE_BYTES: usize = 1 << 20;

/// How long the login shell may take to print its PATH.
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// Variables a parent Claude Code session sets; inherited, they make `claude` refuse to start
/// (this happens when ADO Desk itself was launched from Claude Code during development).
const PARENT_SESSION_VARS: &[&str] = &["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT"];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TerminalEvent {
    Output { data: String },
    /// The process ended; `code` is its exit code when known.
    Exit { code: Option<u32> },
}

struct Session {
    id: u32,
    master: Mutex<Box<dyn MasterPty + Send>>,
    /// Keystrokes go through a writer thread in order: a PTY write blocks while the process
    /// does not read, and must never block the app's main thread.
    input: std::sync::mpsc::Sender<Vec<u8>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

impl Session {
    fn kill(&self) {
        let _ = self.killer.lock().unwrap_or_else(|e| e.into_inner()).kill();
    }
}

#[derive(Default)]
pub struct AssistantState {
    current: Mutex<Option<Arc<Session>>>,
    next_id: AtomicU32,
    /// The login shell's PATH, kept once found.
    login_path: Mutex<Option<String>>,
    /// The local MCP server, started with the first session.
    server: Mutex<Option<ServerInfo>>,
    /// What the main window shows, with the organization it belongs to.
    view: Mutex<Option<(String, ViewContext)>>,
}

impl AssistantState {
    fn session(&self, id: u32) -> AppResult<Arc<Session>> {
        self.current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .filter(|session| session.id == id)
            .ok_or(AppError::Assistant("The assistant session has ended"))
    }

    /// Ends the running session, if any (also on app exit).
    pub fn stop_all(&self) {
        if let Some(session) = self.current.lock().unwrap_or_else(|e| e.into_inner()).take() {
            session.kill();
        }
    }

    pub fn view(&self) -> Option<(String, ViewContext)> {
        self.view.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn server(&self, app: &AppHandle) -> AppResult<ServerInfo> {
        let mut server = self.server.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(info) = server.as_ref() {
            return Ok(info.clone());
        }
        let info = super::server::spawn(super::server::app_runner(app.clone()))?;
        *server = Some(info.clone());
        Ok(info)
    }

    fn login_path(&self) -> Option<String> {
        let mut cached = self.login_path.lock().unwrap_or_else(|e| e.into_inner());
        if cached.is_none() {
            *cached = read_login_path();
        }
        cached.clone()
    }
}

/// The PATH of the user's login shell. A Finder-launched app gets a minimal PATH that misses
/// `~/.local/bin`, Homebrew and version managers, where `claude` and its tools live.
#[cfg(unix)]
fn read_login_path() -> Option<String> {
    use std::process::{Command, Stdio};
    use std::sync::mpsc;

    let shell = std::env::var("SHELL").ok().filter(|s| Path::new(s).is_absolute()).unwrap_or_else(|| "/bin/zsh".into());
    let marker = launch::PATH_MARKER;
    let script = format!("printf '%s%s%s' '{marker}' \"$PATH\" '{marker}'");
    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    let output = rx.recv_timeout(LOGIN_SHELL_TIMEOUT).ok();
    let _ = child.kill();
    let _ = child.wait();
    launch::parse_login_path(&output?)
}

#[cfg(not(unix))]
fn read_login_path() -> Option<String> {
    None
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// The path set in Settings, else the first `claude` on the login PATH, the app's PATH, or the
/// installers' usual directories.
fn locate_claude(configured: Option<&str>, login_path: Option<&str>) -> AppResult<PathBuf> {
    if let Some(path) = launch::configured_path(configured) {
        return if is_executable(&path) {
            Ok(path)
        } else {
            Err(AppError::Assistant("The Claude Code path set in Settings is not an executable file"))
        };
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(path) = login_path {
        dirs.extend(std::env::split_paths(path));
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(home) = std::env::var_os("HOME") {
        dirs.extend(launch::fallback_dirs(Path::new(&home)));
    }
    launch::find_executable(CLAUDE_NAMES, &dirs, is_executable).ok_or(AppError::AssistantNotFound)
}

/// The folder set in Settings (it must exist), else an empty folder in the app data dir.
fn working_folder(app: &AppHandle, configured: Option<&str>) -> AppResult<PathBuf> {
    if let Some(path) = launch::configured_path(configured) {
        return if path.is_dir() {
            Ok(path)
        } else {
            Err(AppError::Assistant("The assistant folder set in Settings does not exist"))
        };
    }
    let unavailable = || AppError::Assistant("The assistant folder could not be created");
    let dir = app.path().app_data_dir().map_err(|_| unavailable())?.join("assistant");
    std::fs::create_dir_all(&dir).map_err(|_| unavailable())?;
    Ok(dir)
}

fn start(app: &AppHandle, cols: u16, rows: u16, events: Channel<TerminalEvent>) -> AppResult<u32> {
    let state = app.state::<AssistantState>();
    let settings = app.state::<SettingsState>().current().assistant;
    let login_path = state.login_path();
    let claude = locate_claude(settings.claude_path.as_deref(), login_path.as_deref())?;
    let folder = working_folder(app, settings.working_folder.as_deref())?;
    let server = state.server(app)?;
    let failed = |_| AppError::Assistant("The assistant terminal could not start");

    let (cols, rows) = launch::pty_size(cols, rows);
    let pair = native_pty_system()
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(failed)?;
    let mut command = CommandBuilder::new(&claude);
    command.args(launch::claude_args(server.port));
    command.cwd(&folder);
    command.env(launch::TOKEN_ENV, &server.token);
    if let Some(path) = &login_path {
        command.env("PATH", path);
    }
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    if std::env::var_os("LANG").is_none() {
        command.env("LANG", "en_US.UTF-8");
    }
    for var in PARENT_SESSION_VARS {
        command.env_remove(var);
    }
    let child = pair.slave.spawn_command(command).map_err(failed)?;
    // The child holds its own copy; closing ours lets reads end when it exits.
    drop(pair.slave);
    let reader = pair.master.try_clone_reader().map_err(failed)?;
    let writer = pair.master.take_writer().map_err(failed)?;

    let id = state.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let session = Arc::new(Session {
        id,
        master: Mutex::new(pair.master),
        input: spawn_writer(writer),
        killer: Mutex::new(child.clone_killer()),
    });
    if let Some(previous) = state.current.lock().unwrap_or_else(|e| e.into_inner()).replace(session) {
        previous.kill();
    }
    let app = app.clone();
    std::thread::spawn(move || pump(app, id, reader, child, events));
    Ok(id)
}

/// Writes queued input to the PTY until the session is dropped (its sender closes) or the
/// process stops reading for good.
fn spawn_writer(mut writer: Box<dyn Write + Send>) -> std::sync::mpsc::Sender<Vec<u8>> {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        for chunk in rx {
            if writer.write_all(&chunk).and_then(|()| writer.flush()).is_err() {
                break;
            }
        }
    });
    tx
}

/// Streams the terminal output until the process ends, then reports its exit code.
fn pump(
    app: AppHandle,
    id: u32,
    mut reader: Box<dyn Read + Send>,
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
    events: Channel<TerminalEvent>,
) {
    let mut buf = vec![0u8; 16 * 1024];
    let mut pending: Vec<u8> = Vec::new();
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                pending.extend_from_slice(&buf[..n]);
                let cut = launch::complete_utf8_len(&pending);
                if cut == 0 {
                    continue;
                }
                let data = String::from_utf8_lossy(&pending[..cut]).into_owned();
                pending.drain(..cut);
                if events.send(TerminalEvent::Output { data }).is_err() {
                    let _ = child.kill();
                    break;
                }
            }
        }
    }
    let code = child.wait().ok().map(|status| status.exit_code());
    let state = app.state::<AssistantState>();
    let mut current = state.current.lock().unwrap_or_else(|e| e.into_inner());
    if current.as_ref().is_some_and(|session| session.id == id) {
        *current = None;
    }
    drop(current);
    let _ = events.send(TerminalEvent::Exit { code });
}

/// Starts `claude` in a new terminal of `cols` x `rows` and returns the session id. Any
/// previous session ends.
#[tauri::command]
pub async fn assistant_start(app: AppHandle, cols: u16, rows: u16, events: Channel<TerminalEvent>) -> AppResult<u32> {
    tauri::async_runtime::spawn_blocking(move || start(&app, cols, rows, events))
        .await
        .map_err(|_| AppError::Assistant("The assistant terminal could not start"))?
}

/// Sends keystrokes or pasted text to the session.
#[tauri::command]
pub fn assistant_write(state: State<'_, AssistantState>, id: u32, data: String) -> AppResult<()> {
    if data.len() > MAX_WRITE_BYTES {
        return Err(AppError::Assistant("That text is too long to send to the terminal"));
    }
    let session = state.session(id)?;
    session
        .input
        .send(data.into_bytes())
        .map_err(|_| AppError::Assistant("The assistant session has ended"))
}

#[tauri::command]
pub fn assistant_resize(state: State<'_, AssistantState>, id: u32, cols: u16, rows: u16) -> AppResult<()> {
    let session = state.session(id)?;
    let (cols, rows) = launch::pty_size(cols, rows);
    let master = session.master.lock().unwrap_or_else(|e| e.into_inner());
    master
        .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|_| AppError::Assistant("The assistant terminal could not be resized"))
}

#[tauri::command]
pub fn assistant_stop(state: State<'_, AssistantState>, id: u32) -> AppResult<()> {
    let mut current = state.current.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(session) = current.take_if(|session| session.id == id) {
        session.kill();
    }
    Ok(())
}

/// Records what the main window shows, for the `current_view` tool. `None` clears it.
#[tauri::command]
pub fn assistant_set_view(
    state: State<'_, AssistantState>,
    app_state: State<'_, crate::AppState>,
    view: Option<ViewContext>,
) -> AppResult<()> {
    let organization = crate::commands::current_client(&app_state).ok().map(|client| client.org().to_string());
    let recorded = match (view, organization) {
        (Some(view), Some(organization)) => Some((organization, context::validate(view)?)),
        _ => None,
    };
    *state.view.lock().unwrap_or_else(|e| e.into_inner()) = recorded;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests_support {
    /// The real lookup, for opt-in tests that run the installed `claude`.
    pub fn login_path() -> Option<String> {
        super::read_login_path()
    }

    pub fn locate(login_path: Option<&str>) -> Option<std::path::PathBuf> {
        super::locate_claude(None, login_path).ok()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let dir = std::env::temp_dir().join(format!("ado-desk-assistant-{name}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn file(path: &Path, mode: u32) {
        std::fs::write(path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn a_configured_path_must_be_an_executable_file() {
        let scratch = Scratch::new("configured");
        let exe = scratch.0.join("claude");
        let plain = scratch.0.join("notes");
        file(&exe, 0o755);
        file(&plain, 0o644);

        assert_eq!(locate_claude(exe.to_str(), None).unwrap(), exe);
        for bad in [plain.to_str().unwrap(), scratch.0.to_str().unwrap(), "/no/such/claude"] {
            let err = locate_claude(Some(bad), None).unwrap_err();
            assert_eq!(err.kind(), "assistant");
            assert!(!err.to_string().contains(bad), "the path is never echoed");
        }
    }

    #[test]
    fn the_login_path_is_searched_and_non_executables_are_skipped() {
        let scratch = Scratch::new("login");
        let first = scratch.0.join("first");
        let second = scratch.0.join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        file(&first.join("claude"), 0o644);
        file(&second.join("claude"), 0o755);
        let login = std::env::join_paths([&first, &second]).unwrap();

        assert_eq!(locate_claude(None, login.to_str()).unwrap(), second.join("claude"));
    }

    #[test]
    fn symlinks_to_executables_are_accepted_and_dangling_ones_rejected_without_echo() {
        let scratch = Scratch::new("symlink");
        let exe = scratch.0.join("real");
        file(&exe, 0o700);
        let link = scratch.0.join("claude");
        std::os::unix::fs::symlink(&exe, &link).unwrap();
        assert_eq!(locate_claude(link.to_str(), None).unwrap(), link);
        let dangling = scratch.0.join("dangling");
        std::os::unix::fs::symlink(scratch.0.join("nope"), &dangling).unwrap();
        let err = locate_claude(dangling.to_str(), None).unwrap_err();
        assert!(!err.to_string().contains(dangling.to_str().unwrap()));
    }

    #[test]
    fn queued_input_reaches_the_process_in_order() {
        let pair = native_pty_system().openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }).unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        // Echo off, read one line, print it back once.
        command.args(["-c", "stty -echo; read line; printf '<%s>' \"$line\""]);
        let mut child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let input = spawn_writer(pair.master.take_writer().unwrap());
        for chunk in ["a", "b", "c", "\r"] {
            input.send(chunk.as_bytes().to_vec()).unwrap();
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let mut buf = [0u8; 256];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..n]);
            }
            let _ = tx.send(out);
        });
        let out = rx.recv_timeout(Duration::from_secs(10)).expect("reads ended");
        assert!(String::from_utf8_lossy(&out).contains("<abc>"), "{:?}", String::from_utf8_lossy(&out));
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn pty_reads_end_when_the_process_exits() {
        // The pump loop relies on this: once the child exits and our slave copy is closed, reads
        // stop, so the exit event is always sent.
        let pair = native_pty_system().openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }).unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "printf 'a\u{f1}o'"]);
        let mut child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let mut buf = [0u8; 1024];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..n]);
            }
            let _ = tx.send(out);
        });
        let out = rx.recv_timeout(Duration::from_secs(10)).expect("reads ended");
        assert_eq!(String::from_utf8_lossy(&out), "a\u{f1}o");
        assert!(child.wait().unwrap().success());
    }
}
