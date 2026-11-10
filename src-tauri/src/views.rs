//! Saved views: named filter sets kept in `settings.json` under `views`. The shape mirrors
//! `SavedView` in `src/lib/settings.ts`.
//!
//! Rust validates only the envelope (id, name, organization, scope, size). The filter is kept as
//! opaque JSON: the frontend validates it with the same parsers it uses for the filters kept in
//! `localStorage`, and every query built from it still goes through `queries.rs`. Loading is
//! lenient: a malformed view is dropped without touching the rest of the settings file.
//!
//! Views only change through `save_view` and `delete_view`; `update_settings` keeps the stored
//! views, so a window holding a stale copy of the settings never erases them.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::settings::{self, Settings, SettingsState};

pub const MAX_VIEWS: usize = 50;
/// Counted in characters, not bytes.
pub const MAX_NAME_CHARS: usize = 60;
/// Size of the filter serialized as JSON.
pub const MAX_FILTER_BYTES: usize = 4096;
const MAX_ID_LEN: usize = 64;
const MAX_ORGANIZATION_LEN: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewScope {
    WorkItems,
    PullRequests,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedView {
    pub id: String,
    pub name: String,
    /// Lowercase organization name; `None` for the built-in starters, shown in every organization.
    pub organization: Option<String>,
    pub scope: ViewScope,
    /// A `WorkItemFilter` or `PullRequestFilter` as the frontend sent it.
    pub filter: Value,
}

/// The saved views, in sidebar order. A settings file without `views` gets the starters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Views(pub Vec<SavedView>);

impl Default for Views {
    fn default() -> Self {
        Self(starters())
    }
}

impl<'de> Deserialize<'de> for Views {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(from_value(Value::deserialize(deserializer)?))
    }
}

/// Built-in views without a project, so they work in every organization. They can be removed.
pub fn starters() -> Vec<SavedView> {
    vec![
        SavedView {
            id: "starter-review".into(),
            name: "Waiting for my review".into(),
            organization: None,
            scope: ViewScope::PullRequests,
            filter: json!({ "project": null, "status": "active", "role": "reviewing" }),
        },
        SavedView {
            id: "starter-closed".into(),
            name: "My closed work items".into(),
            organization: None,
            scope: ViewScope::WorkItems,
            filter: json!({
                "project": null,
                "assignee": { "kind": "me" },
                "iteration": { "kind": "any" },
                "types": [],
                "state": "closed"
            }),
        },
    ]
}

/// Why a view could not be saved. Messages never echo user input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewError {
    InvalidName,
    InvalidFilter,
    NoOrganization,
    TooMany,
}

impl ViewError {
    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidName => "View names need 1 to 60 characters and no control characters.",
            Self::InvalidFilter => "These filters cannot be saved as a view.",
            Self::NoOrganization => "Sign in to an organization before saving a view.",
            Self::TooMany => "You can keep up to 50 views. Remove one first.",
        }
    }
}

/// Keeps the valid entries of a stored `views` value; anything but a list yields the starters.
pub fn from_value(value: Value) -> Views {
    let Value::Array(items) = value else {
        return Views::default();
    };
    let total = items.len();
    let mut views: Vec<SavedView> = Vec::new();
    for item in items {
        let Some(view) = serde_json::from_value::<SavedView>(item).ok().and_then(clean) else {
            continue;
        };
        if views.len() == MAX_VIEWS {
            break;
        }
        if !views.iter().any(|v| v.id == view.id) {
            views.push(view);
        }
    }
    if views.len() < total {
        // The next settings write persists the reduced list; leave a trace without the content.
        eprintln!("settings: ignored {} saved view(s) that could not be read", total - views.len());
    }
    Views(views)
}

/// Appends a new view for `organization`, or explains why it cannot be saved.
pub fn add(
    views: &Views,
    organization: &str,
    scope: ViewScope,
    name: &str,
    filter: Value,
    id: String,
) -> Result<Views, ViewError> {
    if views.0.len() >= MAX_VIEWS {
        return Err(ViewError::TooMany);
    }
    let organization = clean_organization(organization).ok_or(ViewError::NoOrganization)?;
    let name = clean_name(name).ok_or(ViewError::InvalidName)?;
    if !is_valid_filter(&filter) {
        return Err(ViewError::InvalidFilter);
    }
    let mut next = views.0.clone();
    next.push(SavedView { id, name, organization: Some(organization), scope, filter });
    Ok(Views(next))
}

/// Removes the view with `id`; an unknown id changes nothing.
pub fn remove(views: &Views, id: &str) -> Views {
    Views(views.0.iter().filter(|v| v.id != id).cloned().collect())
}

/// `incoming` with the stored views: `update_settings` never changes views.
pub fn with_stored_views(incoming: Settings, stored: &Settings) -> Settings {
    Settings { views: stored.views.clone(), ..incoming }
}

/// A stored view with a trimmed name and lowercase organization, or `None` when invalid.
fn clean(view: SavedView) -> Option<SavedView> {
    if !is_valid_id(&view.id) || !is_valid_filter(&view.filter) {
        return None;
    }
    let organization = match view.organization {
        Some(organization) => Some(clean_organization(&organization)?),
        None => None,
    };
    let name = clean_name(&view.name)?;
    Some(SavedView { name, organization, ..view })
}

fn clean_name(name: &str) -> Option<String> {
    let name = name.trim();
    let length = name.chars().count();
    ((1..=MAX_NAME_CHARS).contains(&length) && !name.chars().any(char::is_control)).then(|| name.to_string())
}

fn clean_organization(organization: &str) -> Option<String> {
    let organization = organization.trim().to_lowercase();
    let valid = !organization.is_empty()
        && organization.len() <= MAX_ORGANIZATION_LEN
        && !organization.chars().any(char::is_control);
    valid.then_some(organization)
}

fn is_valid_filter(filter: &Value) -> bool {
    filter.is_object() && serde_json::to_vec(filter).is_ok_and(|bytes| bytes.len() <= MAX_FILTER_BYTES)
}

/// Ids are short slugs (`[a-z0-9-]`): the starters' fixed ids and `view-<hex>` for saved ones.
fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A fresh id that no existing view uses.
fn new_id(views: &Views) -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
    let mut candidate = nanos;
    loop {
        let id = format!("view-{candidate:x}");
        if !views.0.iter().any(|v| v.id == id) {
            return id;
        }
        candidate = candidate.wrapping_add(1);
    }
}

/// Applies `change` to the stored views and persists the settings. Serialized with
/// `update_settings` through the `updates` gate. A rejected change or a failed write leaves
/// both the file and the in-memory settings as they were.
fn change_views(
    state: &SettingsState,
    change: impl FnOnce(&Views) -> Result<Views, ViewError>,
) -> Result<Settings, String> {
    let _update = state.updates.lock().unwrap_or_else(|e| e.into_inner());
    let mut next = state.current();
    next.views = change(&next.views).map_err(|e| e.message().to_string())?;
    settings::save(&state.path, &next).map_err(|e| format!("Could not save settings: {e}"))?;
    *state.settings.lock().unwrap_or_else(|e| e.into_inner()) = next.clone();
    Ok(next)
}

/// `change_views`, then broadcasts the stored settings to every window.
fn update_views(
    app: &AppHandle,
    state: &SettingsState,
    change: impl FnOnce(&Views) -> Result<Views, ViewError>,
) -> Result<Settings, String> {
    let settings = change_views(state, change)?;
    let _ = app.emit(settings::CHANGED_EVENT, &settings);
    Ok(settings)
}

/// Saves the current filters of a list as a view of `organization`; resolves with the stored
/// settings. Async on purpose: the `updates` gate may be held by `update_settings` while the
/// shortcut plugin waits on the main thread, so this must never block the main thread.
#[tauri::command]
pub async fn save_view(
    app: AppHandle,
    state: State<'_, SettingsState>,
    organization: String,
    scope: ViewScope,
    name: String,
    filter: Value,
) -> Result<Settings, String> {
    update_views(&app, &state, |views| add(views, &organization, scope, &name, filter, new_id(views)))
}

/// Removes a saved or built-in view; resolves with the stored settings. Async for the same
/// reason as `save_view`.
#[tauri::command]
pub async fn delete_view(app: AppHandle, state: State<'_, SettingsState>, id: String) -> Result<Settings, String> {
    update_views(&app, &state, |views| Ok(remove(views, &id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(id: &str, organization: Option<&str>) -> Value {
        json!({
            "id": id,
            "name": format!("View {id}"),
            "organization": organization,
            "scope": "pullRequests",
            "filter": { "project": "Web", "status": "active", "role": "everyone" }
        })
    }

    fn ids(views: &Views) -> Vec<&str> {
        views.0.iter().map(|v| v.id.as_str()).collect()
    }

    fn pr_filter() -> Value {
        json!({ "project": "Web", "status": "completed", "role": "mine" })
    }

    #[test]
    fn a_settings_file_without_views_gets_the_starters() {
        let settings: Settings = serde_json::from_str(r#"{ "appearance": { "mode": "dark" } }"#).unwrap();
        assert_eq!(settings.views.0, starters());
        assert_eq!(ids(&settings.views), vec!["starter-review", "starter-closed"]);
    }

    #[test]
    fn an_empty_views_list_stays_empty() {
        let settings: Settings = serde_json::from_str(r#"{ "views": [] }"#).unwrap();
        assert!(settings.views.0.is_empty());
    }

    #[test]
    fn views_of_the_wrong_type_load_the_starters_and_keep_other_settings() {
        let settings: Settings = serde_json::from_str(r#"{ "appearance": { "mode": "dark" }, "views": 5 }"#).unwrap();
        assert_eq!(settings.views, Views::default());
        assert_eq!(settings.appearance.mode, settings::ThemeMode::Dark);
    }

    #[test]
    fn malformed_views_are_dropped_without_resetting_other_settings() {
        let mut too_long = view("long", Some("contoso"));
        too_long["name"] = json!("x".repeat(MAX_NAME_CHARS + 1));
        let mut not_object = view("array", Some("contoso"));
        not_object["filter"] = json!([1, 2]);
        let mut oversized = view("big", Some("contoso"));
        oversized["filter"] = json!({ "project": "p".repeat(MAX_FILTER_BYTES) });
        let mut bad_scope = view("scope", Some("contoso"));
        bad_scope["scope"] = json!("boards");
        let mut control = view("control", Some("contoso"));
        control["name"] = json!("Bad\u{7}name");
        let file = json!({
            "appearance": { "mode": "dark" },
            "views": [
                view("good", Some("contoso")),
                { "id": "missing-fields" },
                view("Bad Id", Some("contoso")),
                view("empty-org", Some("  ")),
                too_long, not_object, oversized, bad_scope, control,
                "not a view",
                view("starter-like", None)
            ]
        });

        let settings: Settings = serde_json::from_value(file).unwrap();

        assert_eq!(ids(&settings.views), vec!["good", "starter-like"]);
        assert_eq!(settings.appearance.mode, settings::ThemeMode::Dark);
    }

    #[test]
    fn loading_trims_names_lowercases_organizations_drops_duplicate_ids_and_caps_the_list() {
        let mut first = view("same", Some(" Contoso "));
        first["name"] = json!("  Mine  ");
        let mut list = vec![first, view("same", Some("fabrikam"))];
        list.extend((0..MAX_VIEWS + 5).map(|i| view(&format!("v{i}"), Some("contoso"))));

        let views = from_value(Value::Array(list));

        assert_eq!(views.0.len(), MAX_VIEWS);
        assert_eq!(views.0[0].name, "Mine");
        assert_eq!(views.0[0].organization.as_deref(), Some("contoso"));
        assert_eq!(views.0.iter().filter(|v| v.id == "same").count(), 1);
    }

    #[test]
    fn add_appends_a_cleaned_view_for_the_organization() {
        let views = Views(starters());

        let next = add(&views, " Contoso ", ViewScope::PullRequests, "  Done in Web ", pr_filter(), "view-1".into()).unwrap();

        assert_eq!(ids(&next), vec!["starter-review", "starter-closed", "view-1"]);
        let saved = &next.0[2];
        assert_eq!(saved.name, "Done in Web");
        assert_eq!(saved.organization.as_deref(), Some("contoso"));
        assert_eq!(saved.scope, ViewScope::PullRequests);
        assert_eq!(saved.filter, pr_filter());
    }

    #[test]
    fn add_counts_name_length_in_characters() {
        let name = "é".repeat(MAX_NAME_CHARS);
        let next = add(&Views(vec![]), "contoso", ViewScope::WorkItems, &name, json!({}), "view-1".into()).unwrap();
        assert_eq!(next.0[0].name, name);
    }

    #[test]
    fn add_rejects_invalid_input_without_echoing_it() {
        let empty = Views(vec![]);
        let save = |org: &str, name: &str, filter: Value| add(&empty, org, ViewScope::WorkItems, name, filter, "view-1".into());

        assert_eq!(save("contoso", "   ", json!({})), Err(ViewError::InvalidName));
        assert_eq!(save("contoso", &"x".repeat(MAX_NAME_CHARS + 1), json!({})), Err(ViewError::InvalidName));
        assert_eq!(save("contoso", "line\nbreak", json!({})), Err(ViewError::InvalidName));
        assert_eq!(save("  ", "Mine", json!({})), Err(ViewError::NoOrganization));
        assert_eq!(save(&"o".repeat(MAX_ORGANIZATION_LEN + 1), "Mine", json!({})), Err(ViewError::NoOrganization));
        assert_eq!(save("contoso", "Mine", json!("text")), Err(ViewError::InvalidFilter));
        assert_eq!(save("contoso", "Mine", json!({ "project": "p".repeat(MAX_FILTER_BYTES) })), Err(ViewError::InvalidFilter));
        for error in [ViewError::InvalidName, ViewError::InvalidFilter, ViewError::NoOrganization, ViewError::TooMany] {
            assert!(!error.message().contains("Mine"));
        }
    }

    #[test]
    fn add_refuses_a_view_past_the_cap() {
        let full = from_value(Value::Array((0..MAX_VIEWS).map(|i| view(&format!("v{i}"), Some("contoso"))).collect()));
        assert_eq!(full.0.len(), MAX_VIEWS);

        let result = add(&full, "contoso", ViewScope::WorkItems, "One more", json!({}), "view-1".into());

        assert_eq!(result, Err(ViewError::TooMany));
    }

    #[test]
    fn remove_drops_saved_and_starter_views_and_ignores_unknown_ids() {
        let views = add(&Views(starters()), "contoso", ViewScope::PullRequests, "Mine", pr_filter(), "view-1".into()).unwrap();

        let without_starter = remove(&views, "starter-review");
        assert_eq!(ids(&without_starter), vec!["starter-closed", "view-1"]);
        assert_eq!(ids(&remove(&without_starter, "view-1")), vec!["starter-closed"]);
        assert_eq!(remove(&without_starter, "unknown"), without_starter);
    }

    #[test]
    fn update_settings_keeps_the_stored_views() {
        let stored = Settings {
            views: add(&Views(vec![]), "contoso", ViewScope::PullRequests, "Mine", pr_filter(), "view-1".into()).unwrap(),
            ..Settings::default()
        };
        let mut incoming = Settings::default();
        incoming.appearance.mode = settings::ThemeMode::Light;
        incoming.views = Views(vec![]);

        let merged = with_stored_views(incoming, &stored);

        assert_eq!(merged.views, stored.views);
        assert_eq!(merged.appearance.mode, settings::ThemeMode::Light);
    }

    #[test]
    fn a_view_change_is_persisted_and_a_rejected_one_changes_nothing() {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("ado-desk-views-{}-{nanos}", std::process::id()));
        let path = dir.join(settings::FILE_NAME);
        let state = SettingsState::load(path.clone());

        let saved = change_views(&state, |views| add(views, "contoso", ViewScope::PullRequests, "Mine", pr_filter(), "view-1".into())).unwrap();
        assert_eq!(ids(&saved.views), vec!["starter-review", "starter-closed", "view-1"]);
        assert_eq!(settings::load(&path), saved);
        assert_eq!(state.current(), saved);
        let on_disk = std::fs::read(&path).unwrap();

        let rejected = change_views(&state, |views| add(views, "contoso", ViewScope::PullRequests, " ", pr_filter(), "view-2".into()));
        assert_eq!(rejected, Err(ViewError::InvalidName.message().to_string()));
        assert_eq!(std::fs::read(&path).unwrap(), on_disk);
        assert_eq!(state.current(), saved);

        let removed = change_views(&state, |views| Ok(remove(views, "starter-review"))).unwrap();
        assert_eq!(ids(&removed.views), vec!["starter-closed", "view-1"]);
        assert_eq!(settings::load(&path), removed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_ids_are_valid_and_unique() {
        let views = Views(starters());
        let id = new_id(&views);
        assert!(is_valid_id(&id));
        assert!(!ids(&views).contains(&id.as_str()));
    }
}
