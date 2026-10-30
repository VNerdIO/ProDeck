//! Projects are shared across the company: every user can list, open and edit them.
//! Only the owner or an admin can delete one.

use std::collections::HashMap;

use api_types::{
    ApiError, Estimate, IssueRequest, IssuedEstimate, IssuedEstimateMeta, Project, ProjectDetail,
    ProjectStatus, ProjectSummary, SaveProjectRequest,
};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;

use crate::auth::{self, Auth};
use crate::error::{AppError, AppResult};
use crate::store::{IssuedRec, ProjectRec};
use crate::AppState;

const MAX_SECTIONS: usize = 20;
const MAX_STAIRS: usize = 10;

#[derive(Deserialize, utoipa::IntoParams)]
pub struct ListQuery {
    /// Case-insensitive match on name, customer or address.
    q: Option<String>,
    /// Only projects you own.
    #[serde(default)]
    mine: bool,
}

fn area_sqft(p: &Project) -> f64 {
    (p.sections.iter().map(|s| s.width * s.depth).sum::<f64>() / 144.0 * 10.0).round() / 10.0
}

async fn names(st: &AppState) -> AppResult<HashMap<String, String>> {
    Ok(st
        .store
        .users()
        .await?
        .into_iter()
        .map(|u| (u.id, u.name))
        .collect())
}

async fn load(st: &AppState, id: &str) -> AppResult<ProjectRec> {
    st.store
        .project(id)
        .await?
        .ok_or_else(|| AppError::not_found("Project"))
}

fn validate(req: &SaveProjectRequest) -> AppResult<()> {
    if req.name.trim().is_empty() || req.name.len() > 120 {
        return Err(AppError::bad_request(
            "Project name is required (up to 120 characters).",
        ));
    }
    if req.input.sections.len() > MAX_SECTIONS || req.input.stairs.len() > MAX_STAIRS {
        return Err(AppError::bad_request(format!(
            "Projects are limited to {MAX_SECTIONS} sections and {MAX_STAIRS} stairs."
        )));
    }
    if req.notes.len() > 10_000 || req.customer.len() > 200 || req.address.len() > 300 {
        return Err(AppError::bad_request(
            "Notes, customer or address is too long.",
        ));
    }
    Ok(())
}

async fn detail(st: &AppState, p: &ProjectRec) -> AppResult<ProjectDetail> {
    let names = names(st).await?;
    let mut issued: Vec<IssuedEstimateMeta> = st
        .store
        .issued_for(&p.id)
        .await?
        .into_iter()
        .map(|i| i.issued.meta)
        .collect();
    issued.sort_by_key(|a| std::cmp::Reverse(a.issued_at));
    Ok(ProjectDetail {
        id: p.id.clone(),
        name: p.name.clone(),
        customer: p.customer.clone(),
        address: p.address.clone(),
        notes: p.notes.clone(),
        owner_id: p.owner_id.clone(),
        owner_name: names.get(&p.owner_id).cloned().unwrap_or_default(),
        created_at: p.created_at,
        updated_at: p.updated_at,
        status: if issued.is_empty() {
            ProjectStatus::Draft
        } else {
            ProjectStatus::Issued
        },
        input: p.input.clone(),
        issued,
    })
}

/// List projects, most recently updated first.
#[utoipa::path(get, path = "/api/v1/projects", tag = "projects", params(ListQuery),
    responses((status = 200, body = Vec<ProjectSummary>)),
    security(("session" = []), ("api_key" = [])))]
pub async fn list(
    State(st): State<AppState>,
    a: Auth,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<ProjectSummary>>> {
    let names = names(&st).await?;
    let needle =
        q.q.map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
    // Status needs issued counts; one query per project is fine at company scale.
    let mut out = Vec::new();
    for p in st.store.projects().await? {
        if q.mine && p.owner_id != a.user.id {
            continue;
        }
        if let Some(n) = &needle {
            let hay = format!("{} {} {}", p.name, p.customer, p.address).to_lowercase();
            if !hay.contains(n) {
                continue;
            }
        }
        let issued = !st.store.issued_for(&p.id).await?.is_empty();
        out.push(ProjectSummary {
            id: p.id.clone(),
            name: p.name.clone(),
            customer: p.customer.clone(),
            address: p.address.clone(),
            owner_name: names.get(&p.owner_id).cloned().unwrap_or_default(),
            owner_id: p.owner_id,
            updated_at: p.updated_at,
            status: if issued {
                ProjectStatus::Issued
            } else {
                ProjectStatus::Draft
            },
            deck_area_sqft: area_sqft(&p.input),
        });
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
    Ok(Json(out))
}

/// Create a project. The design doesn't have to be valid yet; check it with the estimate endpoints.
#[utoipa::path(post, path = "/api/v1/projects", tag = "projects", request_body = SaveProjectRequest,
    responses((status = 201, body = ProjectDetail), (status = 400, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn create(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<SaveProjectRequest>,
) -> AppResult<(StatusCode, Json<ProjectDetail>)> {
    validate(&req)?;
    let now = Utc::now();
    let p = ProjectRec {
        id: auth::random_id(),
        name: req.name.trim().into(),
        customer: req.customer.trim().into(),
        address: req.address.trim().into(),
        notes: req.notes,
        owner_id: a.user.id.clone(),
        created_at: now,
        updated_at: now,
        input: req.input,
    };
    st.store.put_project(&p).await?;
    Ok((StatusCode::CREATED, Json(detail(&st, &p).await?)))
}

#[utoipa::path(get, path = "/api/v1/projects/{id}", tag = "projects", params(("id" = String, Path)),
    responses((status = 200, body = ProjectDetail), (status = 404, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn get(
    State(st): State<AppState>,
    _a: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<ProjectDetail>> {
    let p = load(&st, &id).await?;
    Ok(Json(detail(&st, &p).await?))
}

/// Replace a project's details and design.
#[utoipa::path(put, path = "/api/v1/projects/{id}", tag = "projects", params(("id" = String, Path)),
    request_body = SaveProjectRequest,
    responses((status = 200, body = ProjectDetail), (status = 400, body = ApiError), (status = 404, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn update(
    State(st): State<AppState>,
    _a: Auth,
    Path(id): Path<String>,
    Json(req): Json<SaveProjectRequest>,
) -> AppResult<Json<ProjectDetail>> {
    validate(&req)?;
    let mut p = load(&st, &id).await?;
    p.name = req.name.trim().into();
    p.customer = req.customer.trim().into();
    p.address = req.address.trim().into();
    p.notes = req.notes;
    p.input = req.input;
    p.updated_at = Utc::now();
    st.store.put_project(&p).await?;
    Ok(Json(detail(&st, &p).await?))
}

/// Delete a project and its issued estimates (owner or admin).
#[utoipa::path(delete, path = "/api/v1/projects/{id}", tag = "projects", params(("id" = String, Path)),
    responses((status = 204), (status = 403, body = ApiError), (status = 404, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn delete(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let p = load(&st, &id).await?;
    if p.owner_id != a.user.id && !a.is_admin() {
        return Err(AppError::forbidden(
            "Only the project owner or an admin can delete it.",
        ));
    }
    st.store.delete_project(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Copy a project (as a starting point for a similar job). You own the copy.
#[utoipa::path(post, path = "/api/v1/projects/{id}/duplicate", tag = "projects", params(("id" = String, Path)),
    responses((status = 201, body = ProjectDetail), (status = 404, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn duplicate(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<ProjectDetail>)> {
    let src = load(&st, &id).await?;
    let now = Utc::now();
    let p = ProjectRec {
        id: auth::random_id(),
        name: format!("{} (copy)", src.name),
        owner_id: a.user.id.clone(),
        created_at: now,
        updated_at: now,
        ..src
    };
    st.store.put_project(&p).await?;
    Ok((StatusCode::CREATED, Json(detail(&st, &p).await?)))
}

/// Estimate the project's current design.
#[utoipa::path(get, path = "/api/v1/projects/{id}/estimate", tag = "projects", params(("id" = String, Path)),
    responses((status = 200, body = Estimate), (status = 404, body = ApiError), (status = 422, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn estimate(
    State(st): State<AppState>,
    _a: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Estimate>> {
    let p = load(&st, &id).await?;
    Ok(Json(estimator::estimate(&p.input)?))
}

/// Issue the current estimate: snapshot the design and results so later edits don't change
/// what was quoted or sent to the store.
#[utoipa::path(post, path = "/api/v1/projects/{id}/issued", tag = "projects", params(("id" = String, Path)),
    request_body = IssueRequest,
    responses((status = 201, body = IssuedEstimate), (status = 404, body = ApiError), (status = 422, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn issue(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<String>,
    Json(req): Json<IssueRequest>,
) -> AppResult<(StatusCode, Json<IssuedEstimate>)> {
    let p = load(&st, &id).await?;
    let estimate = estimator::estimate(&p.input)?;
    let label = req.label.trim();
    let issued = IssuedEstimate {
        meta: IssuedEstimateMeta {
            id: auth::random_id(),
            issued_at: Utc::now(),
            issued_by: a.user.name.clone(),
            label: if label.is_empty() {
                "Estimate".into()
            } else {
                label.chars().take(80).collect()
            },
        },
        input: p.input,
        estimate,
    };
    st.store
        .put_issued(&IssuedRec {
            id: issued.meta.id.clone(),
            project_id: p.id,
            issued: issued.clone(),
        })
        .await?;
    Ok((StatusCode::CREATED, Json(issued)))
}

/// Fetch an issued estimate snapshot.
#[utoipa::path(get, path = "/api/v1/projects/{id}/issued/{issued_id}", tag = "projects",
    params(("id" = String, Path), ("issued_id" = String, Path)),
    responses((status = 200, body = IssuedEstimate), (status = 404, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn get_issued(
    State(st): State<AppState>,
    _a: Auth,
    Path((id, issued_id)): Path<(String, String)>,
) -> AppResult<Json<IssuedEstimate>> {
    let rec = st
        .store
        .issued(&issued_id)
        .await?
        .filter(|r| r.project_id == id)
        .ok_or_else(|| AppError::not_found("Issued estimate"))?;
    Ok(Json(rec.issued))
}

/// Estimate any design without saving it (used for live preview while editing).
#[utoipa::path(post, path = "/api/v1/estimate", tag = "estimate", request_body = Project,
    responses((status = 200, body = Estimate), (status = 422, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn estimate_adhoc(_a: Auth, Json(p): Json<Project>) -> AppResult<Json<Estimate>> {
    if p.sections.len() > MAX_SECTIONS || p.stairs.len() > MAX_STAIRS {
        return Err(AppError::bad_request("Too many sections or stairs."));
    }
    Ok(Json(estimator::estimate(&p)?))
}
