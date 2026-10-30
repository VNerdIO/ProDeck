use std::collections::HashMap;

use api_types::{ApiError, ApiKey, CreateApiKeyRequest, CreatedApiKey, API_KEY_MAX_DAYS};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use crate::auth::{self, Auth};
use crate::error::{AppError, AppResult};
use crate::store::ApiKeyRec;
use crate::AppState;

#[derive(Deserialize, utoipa::IntoParams)]
pub struct ListQuery {
    /// Admins only: include every user's keys.
    #[serde(default)]
    all: bool,
}

pub(crate) fn key_view(k: &ApiKeyRec, owner_email: &str, now: DateTime<Utc>) -> ApiKey {
    ApiKey {
        id: k.id.clone(),
        name: k.name.clone(),
        prefix: auth::api_key_prefix(&k.id),
        owner_id: k.owner_id.clone(),
        owner_email: owner_email.to_string(),
        created_at: k.created_at,
        expires_at: k.expires_at,
        last_used_at: k.last_used_at,
        revoked_at: k.revoked_at,
        status: ApiKey::compute_status(k.expires_at, k.revoked_at, now),
    }
}

/// List your API keys (admins can pass `?all=true`).
#[utoipa::path(get, path = "/api/v1/api-keys", tag = "api-keys", params(ListQuery),
    responses((status = 200, body = Vec<ApiKey>), (status = 403, body = ApiError)),
    security(("session" = [])))]
pub async fn list(
    State(st): State<AppState>,
    a: Auth,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<ApiKey>>> {
    a.require_session()?;
    if q.all {
        a.require_admin()?;
    }
    let keys = st
        .store
        .api_keys(if q.all { None } else { Some(&a.user.id) })
        .await?;
    let emails: HashMap<String, String> = st
        .store
        .users()
        .await?
        .into_iter()
        .map(|u| (u.id, u.email))
        .collect();
    let now = Utc::now();
    let mut out: Vec<ApiKey> = keys
        .iter()
        .map(|k| {
            key_view(
                k,
                emails
                    .get(&k.owner_id)
                    .map(String::as_str)
                    .unwrap_or("(deleted user)"),
                now,
            )
        })
        .collect();
    out.sort_by_key(|x| std::cmp::Reverse(x.created_at));
    Ok(Json(out))
}

/// Create an API key. The full token is returned once and never again.
///
/// Every key expires: pass `expires_in_days` (1-365) or `expires_at` (at most 365 days out).
/// The key acts with your role. API keys can't create keys or change passwords.
#[utoipa::path(post, path = "/api/v1/api-keys", tag = "api-keys", request_body = CreateApiKeyRequest,
    responses((status = 201, body = CreatedApiKey), (status = 400, body = ApiError), (status = 403, body = ApiError)),
    security(("session" = [])))]
pub async fn create(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<CreateApiKeyRequest>,
) -> AppResult<(StatusCode, Json<CreatedApiKey>)> {
    a.require_session()?;
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::bad_request(
            "Give the key a name (up to 80 characters).",
        ));
    }
    let now = Utc::now();
    let expires_at = match (req.expires_in_days, req.expires_at) {
        (Some(d), None) if (1..=API_KEY_MAX_DAYS).contains(&(d as i64)) => now + Duration::days(d as i64),
        (None, Some(t)) if t > now + Duration::hours(1) && t <= now + Duration::days(API_KEY_MAX_DAYS) => t,
        _ => {
            return Err(AppError::bad_request(format!(
                "Set an expiry: expires_in_days (1-{API_KEY_MAX_DAYS}) or an expires_at within {API_KEY_MAX_DAYS} days."
            )))
        }
    };
    let (id, token, secret_hash) = auth::new_api_key_token();
    let rec = ApiKeyRec {
        id,
        name,
        owner_id: a.user.id.clone(),
        secret_hash,
        created_at: now,
        expires_at,
        last_used_at: None,
        revoked_at: None,
    };
    st.store.put_api_key(&rec).await?;
    tracing::info!(actor = %a.user.email, key = %rec.id, expires = %rec.expires_at, "api key created");
    Ok((
        StatusCode::CREATED,
        Json(CreatedApiKey {
            key: key_view(&rec, &a.user.email, now),
            token,
        }),
    ))
}

/// Revoke an API key (yours, or any key for admins). Takes effect immediately.
#[utoipa::path(delete, path = "/api/v1/api-keys/{id}", tag = "api-keys", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn revoke(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut key = st
        .store
        .api_key(&id)
        .await?
        .ok_or_else(|| AppError::not_found("API key"))?;
    if key.owner_id != a.user.id && !a.is_admin() {
        return Err(AppError::not_found("API key"));
    }
    if key.revoked_at.is_none() {
        key.revoked_at = Some(Utc::now());
        st.store.revoke_api_key(&key).await?;
        tracing::info!(actor = %a.user.email, key = %key.id, "api key revoked");
    }
    Ok(StatusCode::NO_CONTENT)
}
