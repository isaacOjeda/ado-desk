//! The assistant's local MCP server: HTTP/1.1 on 127.0.0.1, a random port and a per-launch
//! bearer token. Started with the first assistant session and kept for the app's lifetime; the
//! token reaches `claude` only through an environment variable of its terminal.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming as Body};
use hyper::header::{HeaderValue, ALLOW, AUTHORIZATION, CONTENT_TYPE, HOST, ORIGIN};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use super::confirm;
use super::context;
use super::mcp::{self, Incoming, ToolCall};
use super::AssistantState;
use crate::error::{AppError, AppResult};
use crate::AppState;

/// Where the session finds the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    pub port: u16,
    pub token: String,
}

/// Comments of a work item included in `get_work_item` (newest first).
const MAX_WORK_ITEM_COMMENTS: usize = 20;

pub type ToolFuture = Pin<Box<dyn Future<Output = Value> + Send>>;
/// Runs a validated tool call and returns its MCP tool result.
pub type ToolRunner = Arc<dyn Fn(ToolCall) -> ToolFuture + Send + Sync>;

/// 32 random bytes, hex encoded.
pub fn new_token() -> AppResult<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AppError::Assistant("The assistant server could not start"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Binds a loopback port and serves MCP requests on the async runtime until the app exits.
pub fn spawn(runner: ToolRunner) -> AppResult<ServerInfo> {
    let failed = |_| AppError::Assistant("The assistant server could not start");
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(failed)?;
    listener.set_nonblocking(true).map_err(failed)?;
    let port = listener.local_addr().map_err(failed)?.port();
    let info = ServerInfo { port, token: new_token()? };
    let served = info.clone();
    tauri::async_runtime::spawn(async move { serve(listener, served, runner).await });
    Ok(info)
}

async fn serve(listener: std::net::TcpListener, info: ServerInfo, runner: ToolRunner) {
    let Ok(listener) = tokio::net::TcpListener::from_std(listener) else { return };
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(_) => {
                // Out of file descriptors and the like: back off instead of spinning.
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let info = info.clone();
        let runner = runner.clone();
        tokio::spawn(async move {
            let service = service_fn(move |request| handle(request, info.clone(), runner.clone()));
            let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
        });
    }
}

fn empty(status: StatusCode) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::new()));
    *response.status_mut() = status;
    response
}

fn json_response(status: StatusCode, body: &Value) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(body.to_string())));
    *response.status_mut() = status;
    response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}

async fn handle(request: Request<Body>, info: ServerInfo, runner: ToolRunner) -> Result<Response<Full<Bytes>>, Infallible> {
    let header = |name| request.headers().get(name).and_then(|v: &HeaderValue| v.to_str().ok());
    // Any Origin header counts, even one that is not valid UTF-8.
    let origin = request.headers().contains_key(ORIGIN).then_some("present");
    let checked = mcp::check_request(
        request.method().as_str(),
        request.uri().path(),
        header(HOST),
        origin,
        header(AUTHORIZATION),
        info.port,
        &info.token,
    );
    if let Err(reject) = checked {
        let mut response = empty(StatusCode::from_u16(reject.status()).unwrap_or(StatusCode::FORBIDDEN));
        if reject == mcp::Reject::MethodNotAllowed {
            response.headers_mut().insert(ALLOW, HeaderValue::from_static("POST"));
        }
        return Ok(response);
    }
    let Ok(body) = Limited::new(request.into_body(), mcp::MAX_BODY_BYTES).collect().await else {
        return Ok(empty(StatusCode::PAYLOAD_TOO_LARGE));
    };
    Ok(match mcp::parse(&body.to_bytes()) {
        Incoming::Invalid => json_response(
            StatusCode::BAD_REQUEST,
            &mcp::error(&Value::Null, mcp::PARSE_ERROR, "Invalid JSON-RPC message"),
        ),
        Incoming::NoReply => empty(StatusCode::ACCEPTED),
        Incoming::Request { id, method, params } => {
            json_response(StatusCode::OK, &dispatch(&id, &method, &params, &runner).await)
        }
    })
}

async fn dispatch(id: &Value, method: &str, params: &Value, runner: &ToolRunner) -> Value {
    match method {
        "initialize" => mcp::result(id, mcp::initialize_result(params)),
        "ping" => mcp::result(id, json!({})),
        "tools/list" => mcp::result(id, mcp::tools()),
        "tools/call" => match mcp::parse_tool_call(params) {
            Ok(call) => mcp::result(id, runner(call).await),
            Err(message) if message == mcp::UNKNOWN_TOOL => mcp::error(id, mcp::INVALID_PARAMS, message),
            Err(message) => mcp::result(id, mcp::tool_text(message, true)),
        },
        _ => mcp::error(id, mcp::METHOD_NOT_FOUND, "Method not found"),
    }
}

/// The tools backed by the app: the active organization's client and the recorded view.
pub fn app_runner(app: AppHandle) -> ToolRunner {
    Arc::new(move |call| {
        let app = app.clone();
        Box::pin(async move { run_tool(&app, call).await })
    })
}

fn data(value: Result<Value, AppError>) -> Value {
    match value {
        Ok(value) => mcp::tool_text(&serde_json::to_string_pretty(&value).unwrap_or_default(), false),
        Err(err) => mcp::tool_text(mcp::tool_error(&err), true),
    }
}

fn active_organization(app: &AppHandle) -> Option<String> {
    crate::commands::current_client(&app.state::<AppState>()).ok().map(|c| c.org().to_string())
}

/// Asks the user in the main window, then runs `write` only if they approved it and the same
/// organization is still active.
async fn confirmed_write<F>(app: &AppHandle, action: &str, target: String, text: &str, write: F) -> Value
where
    F: Future<Output = AppResult<()>>,
{
    let Some(organization) = active_organization(app) else {
        return mcp::tool_text(mcp::tool_error(&AppError::NotSignedIn), true);
    };
    let request = confirm::WriteRequest {
        id: 0,
        action: action.to_string(),
        target,
        organization: organization.clone(),
        text: text.to_string(),
    };
    match confirm::ask(app, request).await {
        confirm::Outcome::Approved => {}
        confirm::Outcome::Declined => return mcp::tool_text("The user declined it in ADO Desk; nothing was posted", true),
        confirm::Outcome::Expired => {
            return mcp::tool_text("Nobody confirmed it in ADO Desk in time; nothing was posted", true)
        }
    }
    if active_organization(app).as_deref() != Some(organization.as_str()) {
        return mcp::tool_text("The active organization changed in ADO Desk; nothing was posted", true);
    }
    match write.await {
        Ok(()) => mcp::tool_text("Posted as the user", false),
        Err(err) => mcp::tool_text(mcp::tool_error(&err), true),
    }
}

async fn run_tool(app: &AppHandle, call: ToolCall) -> Value {
    let organization = active_organization(app);
    match call {
        ToolCall::ReplyToThread { project, repository_id, id, thread_id, parent_comment_id, content } => {
            let target = format!("Pull request {id} (project {project}), thread {thread_id}");
            let write = crate::pull_request_actions::reply_to_pull_request_thread(
                app.state(),
                project,
                repository_id,
                id,
                thread_id,
                parent_comment_id,
                content.clone(),
            );
            confirmed_write(app, "Reply to a pull request comment thread", target, &content, write).await
        }
        ToolCall::CommentOnWorkItem { project, id, text } => {
            let target = format!("Work item {id} (project {project})");
            let write = crate::work_item_actions::add_work_item_comment(app.state(), project, id, text.clone());
            confirmed_write(app, "Comment on a work item", target, &text, write).await
        }
        ToolCall::CurrentView => {
            let view = app.state::<AssistantState>().view();
            data(Ok(context::describe(view.as_ref(), organization.as_deref())))
        }
        ToolCall::GetWorkItem { id } => {
            let result = async {
                let detail = crate::work_items::get_work_item(app.state(), id).await?;
                // Comments are secondary: the work item still answers without them.
                let comments = crate::work_items::list_work_item_comments(app.state(), detail.project.clone(), id)
                    .await
                    .map(|mut list| {
                        list.truncate(MAX_WORK_ITEM_COMMENTS);
                        list
                    })
                    .ok();
                Ok(json!({ "organization": organization, "workItem": detail, "recentComments": comments }))
            };
            data(result.await)
        }
        ToolCall::GetPullRequest { project, repository_id, id } => {
            let result = async {
                let (detail, threads, files) = futures::join!(
                    crate::pull_requests::get_pull_request(app.state(), project.clone(), repository_id.clone(), id),
                    crate::pull_requests::list_pull_request_threads(app.state(), project.clone(), repository_id.clone(), id),
                    crate::pull_requests::list_pull_request_files(app.state(), project.clone(), repository_id.clone(), id),
                );
                Ok(json!({
                    "organization": organization,
                    "project": project,
                    "repositoryId": repository_id,
                    "pullRequest": detail?,
                    "threads": threads.ok(),
                    "changedFiles": files.ok(),
                }))
            };
            data(result.await)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server on the test's own runtime with a stub tool runner.
    async fn start_stub() -> ServerInfo {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let info = ServerInfo { port: listener.local_addr().unwrap().port(), token: new_token().unwrap() };
        let runner: ToolRunner = Arc::new(|call| Box::pin(async move { mcp::tool_text(&format!("{call:?}"), false) }));
        let served = info.clone();
        tokio::spawn(async move { serve(listener, served, runner).await });
        info
    }

    fn post(info: &ServerInfo, body: &str) -> reqwest::RequestBuilder {
        reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/mcp", info.port))
            .bearer_auth(&info.token)
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_string())
    }

    #[test]
    fn tokens_are_64_hex_characters_and_differ() {
        let a = new_token().unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, new_token().unwrap());
    }

    #[tokio::test]
    async fn a_session_initializes_lists_and_calls_tools() {
        let info = start_stub().await;
        let init: Value = post(&info, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}"#)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-11-25");

        let notified = post(&info, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).send().await.unwrap();
        assert_eq!(notified.status(), 202);

        let listed: Value = post(&info, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#).send().await.unwrap().json().await.unwrap();
        assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 5);

        let called: Value = post(&info, r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_work_item","arguments":{"id":42}}}"#)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(called["result"]["content"][0]["text"], "GetWorkItem { id: 42 }");

        let bad_args: Value = post(&info, r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_work_item","arguments":{"id":-1}}}"#)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(bad_args["result"]["isError"], true);

        let unknown: Value = post(&info, r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope"}}"#)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(unknown["error"]["code"], mcp::INVALID_PARAMS);

        // Claude Code first probes a newer discovery method, then falls back to initialize.
        let discover: Value = post(&info, r#"{"jsonrpc":"2.0","id":6,"method":"server/discover"}"#).send().await.unwrap().json().await.unwrap();
        assert_eq!(discover["error"]["code"], mcp::METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn requests_without_the_token_or_from_a_browser_are_refused() {
        let info = start_stub().await;
        let url = format!("http://127.0.0.1:{}/mcp", info.port);
        let client = reqwest::Client::new();
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;

        let no_token = client.post(&url).body(body).send().await.unwrap();
        assert_eq!(no_token.status(), 401);
        let wrong = client.post(&url).bearer_auth("x".repeat(64)).body(body).send().await.unwrap();
        assert_eq!(wrong.status(), 401);
        let browser = post(&info, body).header(ORIGIN, "http://evil.example").send().await.unwrap();
        assert_eq!(browser.status(), 403);
        let rebinding = post(&info, body).header(HOST, format!("evil.example:{}", info.port)).send().await.unwrap();
        assert_eq!(rebinding.status(), 403);
        let get = client.get(&url).bearer_auth(&info.token).send().await.unwrap();
        assert_eq!(get.status(), 405);
        assert_eq!(get.headers().get(ALLOW).unwrap(), "POST");
        let other_path = client.post(format!("http://127.0.0.1:{}/", info.port)).bearer_auth(&info.token).body(body).send().await.unwrap();
        assert_eq!(other_path.status(), 404);
    }

    /// Runs the installed `claude` (signed in with the developer's own account) against the
    /// server with the exact panel arguments. Uses the account's quota, so it is opt-in:
    /// `cargo test live_claude -- --ignored`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_claude_calls_current_view_with_the_panel_arguments() {
        let info = start_stub().await;
        let login = super::super::terminal::tests_support::login_path();
        let claude = super::super::terminal::tests_support::locate(login.as_deref()).expect("claude installed");
        let mut command = std::process::Command::new(claude);
        command
            .args(super::super::launch::claude_args(info.port))
            .args(["-p", "Call the current_view tool of the ado-desk server and reply with its text only.", "--max-turns", "3"])
            .env(super::super::launch::TOKEN_ENV, &info.token)
            .env_remove("CLAUDECODE")
            .env_remove("CLAUDE_CODE_ENTRYPOINT")
            .current_dir(std::env::temp_dir());
        if let Some(path) = &login {
            command.env("PATH", path);
        }
        let output = tokio::task::spawn_blocking(move || command.output()).await.unwrap().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("CurrentView"), "stdout: {stdout}\nstderr: {}", String::from_utf8_lossy(&output.stderr));
    }

    #[tokio::test]
    async fn a_tool_call_notification_runs_nothing_and_null_origins_are_refused() {
        let info = start_stub().await;
        let note = r#"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"add_work_item_comment","arguments":{"project":"Web","id":9,"text":"x"}}}"#;
        assert_eq!(post(&info, note).send().await.unwrap().status(), 202);
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        assert_eq!(post(&info, body).header(ORIGIN, "null").send().await.unwrap().status(), 403);
        // Browsers cannot send one, but a non-UTF-8 Origin is still an Origin.
        let raw = HeaderValue::from_bytes(b"http://\xffevil").unwrap();
        assert_eq!(post(&info, body).header(ORIGIN, raw).send().await.unwrap().status(), 403);
        let ping = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let at_limit = format!("{ping}{}", " ".repeat(mcp::MAX_BODY_BYTES - ping.len()));
        assert_eq!(post(&info, &at_limit).send().await.unwrap().status(), 200);
    }

    #[tokio::test]
    async fn oversized_and_malformed_bodies_are_rejected() {
        let info = start_stub().await;
        let huge = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"ping","params":"{}"}}"#, "a".repeat(mcp::MAX_BODY_BYTES));
        assert_eq!(post(&info, &huge).send().await.unwrap().status(), 413);
        let malformed = post(&info, "{nope").send().await.unwrap();
        assert_eq!(malformed.status(), 400);
        let reply: Value = malformed.json().await.unwrap();
        assert_eq!(reply["error"]["code"], mcp::PARSE_ERROR);
    }
}
