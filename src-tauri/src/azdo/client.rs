//! Thin HTTP adapter over the Azure DevOps REST API (api-version 7.1).

use futures::{FutureExt, StreamExt, future::BoxFuture, stream};
use reqwest::{RequestBuilder, StatusCode, header::HeaderMap, redirect};
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
}

impl AzdoClient {
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
        })
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    fn org(&self) -> &str {
        &self.session.organization
    }

    fn url(&self, segments: &[&str]) -> Url {
        let mut full = vec![self.org()];
        full.extend_from_slice(segments);
        let mut url = model::build_url(&full);
        url.query_pairs_mut().append_pair("api-version", API_VERSION);
        url
    }

    fn get(&self, url: Url) -> RequestBuilder {
        self.http.get(url).basic_auth("", Some(&self.pat))
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

    async fn projects(&self) -> AppResult<Vec<String>> {
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

/// Extracts `message` from an Azure DevOps error body, or a short excerpt otherwise.
fn api_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| body.chars().take(200).collect())
}

#[cfg(test)]
mod tests {
    use super::api_message;

    #[test]
    fn api_message_prefers_json_message() {
        assert_eq!(
            api_message(r#"{"message":"TF401019: repo not found","typeKey":"X"}"#),
            "TF401019: repo not found"
        );
        assert_eq!(api_message("plain failure"), "plain failure");
    }
}
