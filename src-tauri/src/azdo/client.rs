//! Thin HTTP adapter over the Azure DevOps REST API (api-version 7.1).

use futures::{FutureExt, StreamExt, future::BoxFuture, stream};
use std::sync::Mutex;

use reqwest::{Method, RequestBuilder, StatusCode, header::{CONTENT_TYPE, HeaderMap}, redirect};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use url::Url;

use super::model::{
    self, ConnectionData, ListResponse, PullRequest, RawProject, RawPullRequest, RawWorkItem,
    Session, WiqlResponse, WorkItem,
};
use crate::error::{AppError, AppResult};

const API_VERSION: &str = "7.1";
const MAX_WORK_ITEMS: usize = 200;
const PROJECT_CONCURRENCY: usize = 6;

const MY_OPEN_WORK_ITEMS: &str = "SELECT [System.Id] FROM WorkItems \
     WHERE [System.AssignedTo] = @Me \
     AND [System.State] NOT IN ('Closed', 'Done', 'Removed') \
     ORDER BY [System.ChangedDate] DESC";

const WORK_ITEM_FIELDS: &str = "System.Id,System.Title,System.WorkItemType,System.State,\
     System.TeamProject,System.ChangedDate,Microsoft.VSTS.Common.Priority";

pub struct AzdoClient {
    http: reqwest::Client,
    pat: String,
    session: Session,
    /// Whether this token is known to write work items; reset by a new token (client).
    work_write: Mutex<WriteAccess>,
}

/// Whether the token can write, as learned from Azure DevOps answers during this session.
/// There is no remembered "denied": a refusal may concern one item only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteAccess {
    Unknown,
    Granted,
}

impl AzdoClient {
    /// The historic "my open work items" WIQL; filtered queries must reproduce it by default.
    #[cfg(test)]
    pub(crate) const MY_OPEN_WORK_ITEMS: &'static str = MY_OPEN_WORK_ITEMS;
    /// Concurrent per-project requests when a query fans out over every project.
    pub(crate) const PROJECT_CONCURRENCY: usize = PROJECT_CONCURRENCY;

    /// Validates the token against `connectionData` and resolves the signed-in identity.
    pub async fn connect(organization: &str, pat: &str) -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("ado-desk/", env!("CARGO_PKG_VERSION")))
            // Azure DevOps redirects unauthenticated calls to a sign-in page; treat it as 401.
            .redirect(redirect::Policy::none())
            .build()?;

        let mut url = model::build_url(&[organization, "_apis", "connectionData"]);
        url.query_pairs_mut().append_pair("api-version", "7.1-preview");

        let (data, _) =
            send::<ConnectionData>(http.get(url).basic_auth("", Some(pat))).await?;

        Ok(Self {
            http,
            pat: pat.to_string(),
            session: model::session_from(organization, data),
            work_write: Mutex::new(WriteAccess::Unknown),
        })
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub(crate) fn org(&self) -> &str {
        &self.session.organization
    }

    /// `https://dev.azure.com/{org}/{segments...}?api-version=7.1`, segments percent-encoded.
    pub(crate) fn url(&self, segments: &[&str]) -> Url {
        self.url_with_version(segments, API_VERSION)
    }

    /// Same as [`Self::url`] for endpoints that only exist as preview versions.
    pub(crate) fn url_with_version(&self, segments: &[&str], api_version: &str) -> Url {
        let mut full = vec![self.org()];
        full.extend_from_slice(segments);
        let mut url = model::build_url(&full);
        url.query_pairs_mut().append_pair("api-version", api_version);
        url
    }

    fn get(&self, url: Url) -> RequestBuilder {
        self.http.get(url).basic_auth("", Some(&self.pat))
    }

    /// Authenticated GET that decodes a JSON body. Shared by every feature module.
    pub(crate) async fn get_json<T: DeserializeOwned>(&self, url: Url) -> AppResult<T> {
        send::<T>(self.get(url)).await.map(|(value, _)| value)
    }

    /// Authenticated POST with a JSON body that decodes a JSON response.
    pub(crate) async fn post_json<T: DeserializeOwned>(&self, url: Url, body: &Value) -> AppResult<T> {
        let request = self.http.post(url).basic_auth("", Some(&self.pat)).json(body);
        send::<T>(request).await.map(|(value, _)| value)
    }

    pub(crate) fn work_write_access(&self) -> WriteAccess {
        *self.work_write.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn record_work_write_access(&self, access: WriteAccess) {
        *self.work_write.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = access;
    }

    /// Authenticated write (POST, PATCH) with a JSON body. The response body is not decoded:
    /// once Azure DevOps accepted a change, a surprising body must not report it as failed.
    /// Failures map through [`write_status_error`], never to `Unauthorized`.
    pub(crate) async fn write(
        &self,
        method: Method,
        url: Url,
        body: &Value,
        content_type: &'static str,
    ) -> AppResult<()> {
        let body = serde_json::to_vec(body).map_err(|err| AppError::Decode(err.to_string()))?;
        let response = self
            .http
            .request(method, url)
            .basic_auth("", Some(&self.pat))
            .header(CONTENT_TYPE, content_type)
            .body(body)
            .send()
            .await?;
        match write_status_error(response.status()) {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Authenticated GET returning the raw body and its content type (attachments, file blobs).
    /// Only URLs on `dev.azure.com` for the signed-in organization are accepted.
    pub(crate) async fn get_bytes(&self, url: Url) -> AppResult<(Vec<u8>, String)> {
        let same_org = url.host_str() == Some("dev.azure.com")
            && url
                .path_segments()
                .and_then(|mut s| s.next())
                .is_some_and(|org| org.eq_ignore_ascii_case(self.org()));
        if url.scheme() != "https" || !same_org {
            return Err(AppError::NotFound);
        }

        let response = self.get(url).send().await?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED
            || status == StatusCode::NON_AUTHORITATIVE_INFORMATION
            || status.is_redirection()
        {
            return Err(AppError::Unauthorized);
        }
        if !status.is_success() {
            return Err(AppError::Api {
                status: status.as_u16(),
                message: "download failed".into(),
            });
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        Ok((response.bytes().await?.to_vec(), content_type))
    }

    pub async fn my_work_items(&self) -> AppResult<Vec<WorkItem>> {
        let mut wiql_url = self.url(&["_apis", "wit", "wiql"]);
        wiql_url
            .query_pairs_mut()
            .append_pair("$top", &MAX_WORK_ITEMS.to_string());

        let request = self
            .http
            .post(wiql_url)
            .basic_auth("", Some(&self.pat))
            .json(&json!({ "query": MY_OPEN_WORK_ITEMS }));
        let (wiql, _) = send::<WiqlResponse>(request).await?;

        let ids: Vec<String> = wiql
            .work_items
            .iter()
            .take(MAX_WORK_ITEMS)
            .map(|r| r.id.to_string())
            .collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut items_url = self.url(&["_apis", "wit", "workitems"]);
        items_url
            .query_pairs_mut()
            .append_pair("ids", &ids.join(","))
            .append_pair("fields", WORK_ITEM_FIELDS)
            .append_pair("errorPolicy", "Omit");

        let (items, _) = send::<ListResponse<Option<RawWorkItem>>>(self.get(items_url)).await?;

        Ok(items
            .value
            .into_iter()
            .flatten()
            .map(|raw| model::map_work_item(self.org(), raw))
            .collect())
    }

    pub(crate) async fn projects(&self) -> AppResult<Vec<String>> {
        let mut names = Vec::new();
        let mut continuation: Option<String> = None;

        loop {
            let mut url = self.url(&["_apis", "projects"]);
            url.query_pairs_mut().append_pair("$top", "500");
            if let Some(token) = &continuation {
                url.query_pairs_mut().append_pair("continuationToken", token);
            }

            let (page, headers) = send::<ListResponse<RawProject>>(self.get(url)).await?;
            names.extend(page.value.into_iter().map(|p| p.name));

            continuation = headers
                .get("x-ms-continuationtoken")
                .and_then(|v| v.to_str().ok())
                .filter(|v| !v.is_empty())
                .map(str::to_string);
            if continuation.is_none() {
                return Ok(names);
            }
        }
    }

    /// Runs a WIQL query (organization scope) and fetches up to [`MAX_WORK_ITEMS`] results.
    pub(crate) async fn work_items_by_wiql(&self, wiql: &str) -> AppResult<Vec<WorkItem>> {
        self.work_items_by_wiql_top(wiql, MAX_WORK_ITEMS).await
    }

    /// Like [`Self::work_items_by_wiql`] with a smaller cap (at most [`MAX_WORK_ITEMS`]).
    pub(crate) async fn work_items_by_wiql_top(&self, wiql: &str, top: usize) -> AppResult<Vec<WorkItem>> {
        let top = top.min(MAX_WORK_ITEMS);
        let mut wiql_url = self.url(&["_apis", "wit", "wiql"]);
        wiql_url.query_pairs_mut().append_pair("$top", &top.to_string());
        let wiql: WiqlResponse = self.post_json(wiql_url, &json!({ "query": wiql })).await?;
        let ids: Vec<i64> = wiql.work_items.iter().take(top).map(|r| r.id).collect();
        self.work_items_by_ids(&ids).await
    }

    /// Work items by ID, in the order given; IDs that do not exist (or are not visible) are
    /// skipped.
    pub(crate) async fn work_items_by_ids(&self, ids: &[i64]) -> AppResult<Vec<WorkItem>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = ids.iter().take(MAX_WORK_ITEMS).map(i64::to_string).collect();
        let mut items_url = self.url(&["_apis", "wit", "workitems"]);
        items_url
            .query_pairs_mut()
            .append_pair("ids", &ids.join(","))
            .append_pair("fields", WORK_ITEM_FIELDS)
            .append_pair("errorPolicy", "Omit");
        let items: ListResponse<Option<RawWorkItem>> = self.get_json(items_url).await?;

        Ok(items
            .value
            .into_iter()
            .flatten()
            .map(|raw| model::map_work_item(self.org(), raw))
            .collect())
    }

    /// One pull request by ID, in any project of the organization; `None` if it does not
    /// exist or is not visible.
    pub(crate) async fn pull_request_by_id(&self, id: i64) -> AppResult<Option<PullRequest>> {
        let url = self.url(&["_apis", "git", "pullrequests", &id.to_string()]);
        match self.get_json::<RawPullRequest>(url).await {
            Ok(raw) => Ok(Some(model::map_pull_request(self.org(), &self.session.user_id, raw))),
            Err(AppError::NotFound) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Pull requests of one project matching the given `searchCriteria.*` / `$top` pairs.
    pub(crate) async fn search_pull_requests(
        &self,
        project: &str,
        params: &[(String, String)],
    ) -> AppResult<Vec<PullRequest>> {
        let mut url = self.url(&[project, "_apis", "git", "pullrequests"]);
        url.query_pairs_mut().extend_pairs(params);
        let prs: ListResponse<RawPullRequest> = self.get_json(url).await?;
        Ok(prs
            .value
            .into_iter()
            .map(|raw| model::map_pull_request(self.org(), &self.session.user_id, raw))
            .collect())
    }

    async fn project_pull_requests(
        &self,
        project: &str,
        criteria: &str,
    ) -> AppResult<Vec<RawPullRequest>> {
        let mut url = self.url(&[project, "_apis", "git", "pullrequests"]);
        url.query_pairs_mut()
            .append_pair("searchCriteria.status", "active")
            .append_pair(criteria, &self.session.user_id)
            .append_pair("$top", "100");

        let (prs, _) = send::<ListResponse<RawPullRequest>>(self.get(url)).await?;
        Ok(prs.value)
    }

    /// Active PRs across every project where the user is the author or a reviewer.
    pub async fn my_pull_requests(&self) -> AppResult<Vec<PullRequest>> {
        let projects = self.projects().await?;
        // Boxed futures sidestep a rustc limitation with closures inside `Send` streams.
        let requests: Vec<BoxFuture<'_, AppResult<Vec<RawPullRequest>>>> = projects
            .iter()
            .flat_map(|project| {
                ["searchCriteria.creatorId", "searchCriteria.reviewerId"]
                    .map(|criteria| self.project_pull_requests(project, criteria).boxed())
            })
            .collect();

        let results: Vec<AppResult<Vec<RawPullRequest>>> = stream::iter(requests)
            .buffer_unordered(PROJECT_CONCURRENCY)
            .collect()
            .await;

        let mut prs = Vec::new();
        for result in results {
            match result {
                Ok(batch) => prs.extend(
                    batch
                        .into_iter()
                        .map(|raw| model::map_pull_request(self.org(), &self.session.user_id, raw)),
                ),
                // Projects without Repos enabled answer 404; they simply have no PRs.
                Err(AppError::NotFound) => {}
                Err(err) => return Err(err),
            }
        }

        Ok(model::merge_pull_requests(prs))
    }
}

async fn send<T: DeserializeOwned>(request: RequestBuilder) -> AppResult<(T, HeaderMap)> {
    let response = request.send().await?;
    let status = response.status();
    let headers = response.headers().clone();

    if status == StatusCode::UNAUTHORIZED
        || status == StatusCode::NON_AUTHORITATIVE_INFORMATION
        || status.is_redirection()
    {
        return Err(AppError::Unauthorized);
    }
    if status == StatusCode::FORBIDDEN {
        return Err(AppError::Forbidden);
    }
    if status == StatusCode::NOT_FOUND {
        return Err(AppError::NotFound);
    }

    let body = response.text().await?;
    if !status.is_success() {
        return Err(AppError::Api {
            status: status.as_u16(),
            message: api_message(&body),
        });
    }

    serde_json::from_str(&body)
        .map(|value| (value, headers))
        .map_err(|err| AppError::Decode(err.to_string()))
}

/// The error of a write answered with `status`, or `None` when the write was accepted.
/// A token without a write scope may be answered 401 (or 203, or a redirect to sign-in) as
/// well as 403; all of them mean "no write access" here, so a write never signs the user out.
/// Server text is dropped: it may quote the rejected value.
pub(crate) fn write_status_error(status: StatusCode) -> Option<AppError> {
    let denied = status == StatusCode::UNAUTHORIZED
        || status == StatusCode::FORBIDDEN
        || status == StatusCode::NON_AUTHORITATIVE_INFORMATION
        || status.is_redirection();
    if denied {
        return Some(AppError::WriteDenied);
    }
    if status.is_success() {
        return None;
    }
    Some(match status {
        StatusCode::NOT_FOUND => AppError::NotFound,
        StatusCode::BAD_REQUEST | StatusCode::CONFLICT | StatusCode::PRECONDITION_FAILED => {
            AppError::WriteRejected
        }
        _ => AppError::Api {
            status: status.as_u16(),
            message: "the change was not saved".into(),
        },
    })
}

/// Extracts `message` from an Azure DevOps error body, or a short excerpt otherwise.
fn api_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| body.chars().take(200).collect())
}

#[cfg(test)]
mod tests {
    use super::{StatusCode, api_message, write_status_error};
    use crate::error::AppError;

    #[test]
    fn write_status_error_never_signs_out() {
        for code in [401, 403, 203, 302, 307] {
            let status = StatusCode::from_u16(code).unwrap();
            assert!(matches!(write_status_error(status), Some(AppError::WriteDenied)), "{code}");
        }
    }

    #[test]
    fn write_status_error_maps_rejections_to_fixed_text() {
        for code in [400, 409, 412] {
            let status = StatusCode::from_u16(code).unwrap();
            assert!(matches!(write_status_error(status), Some(AppError::WriteRejected)), "{code}");
        }
        assert!(matches!(write_status_error(StatusCode::NOT_FOUND), Some(AppError::NotFound)));
        let server = write_status_error(StatusCode::INTERNAL_SERVER_ERROR).unwrap();
        assert_eq!(server.to_string(), "Azure DevOps returned HTTP 500: the change was not saved");
        for code in [200, 201, 204] {
            assert!(write_status_error(StatusCode::from_u16(code).unwrap()).is_none(), "{code}");
        }
    }

    /// Every status a write can get: never `Unauthorized`/`Forbidden` (no sign-out), and only
    /// fixed text in the message.
    #[test]
    fn write_status_error_sweep_never_signs_out_or_echoes() {
        for code in 100u16..600 {
            let Ok(status) = StatusCode::from_u16(code) else { continue };
            match write_status_error(status) {
                None => assert!((200..300).contains(&code) && code != 203, "{code} accepted"),
                Some(AppError::WriteDenied) => {
                    assert!(matches!(code, 401 | 403 | 203 | 300..=399), "{code} denied")
                }
                Some(AppError::WriteRejected) => assert!(matches!(code, 400 | 409 | 412), "{code}"),
                Some(AppError::NotFound) => assert_eq!(code, 404),
                Some(AppError::Api { status, message }) => {
                    assert_eq!(status, code);
                    assert_eq!(message, "the change was not saved");
                }
                Some(other) => panic!("{code}: unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn api_message_prefers_json_message() {
        assert_eq!(
            api_message(r#"{"message":"TF401019: repo not found","typeKey":"X"}"#),
            "TF401019: repo not found"
        );
        assert_eq!(api_message("plain failure"), "plain failure");
    }
}
