use api_types::{ApiError, CreateUserRequest, Role, UpdateUserRequest, User};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;

use crate::auth::{self, Auth};
use crate::error::{AppError, AppResult};
use crate::routes::user_view;
use crate::store::UserRec;
use crate::AppState;

/// List all users (admin).
#[utoipa::path(get, path = "/api/v1/users", tag = "users",
    responses((status = 200, body = Vec<User>), (status = 403, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn list(State(st): State<AppState>, a: Auth) -> AppResult<Json<Vec<User>>> {
    a.require_admin()?;
    let mut users: Vec<User> = st.store.users().await?.iter().map(user_view).collect();
    users.sort_by_key(|x| x.name.to_lowercase());
    Ok(Json(users))
}

/// Create a user (admin). There is no self sign-up.
#[utoipa::path(post, path = "/api/v1/users", tag = "users", request_body = CreateUserRequest,
    responses((status = 201, body = User), (status = 400, body = ApiError), (status = 409, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn create(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<User>)> {
    a.require_admin()?;
    let email = auth::normalize_email(&req.email)?;
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::bad_request("Name is required."));
    }
    auth::check_password_policy(&req.password)?;
    if st.store.user_by_email(&email).await?.is_some() {
        return Err(AppError::conflict("A user with that email already exists."));
    }
    let user = UserRec {
        id: auth::random_id(),
        email,
        name,
        role: req.role,
        pw_hash: auth::hash_password(req.password).await?,
        disabled: false,
        created_at: Utc::now(),
        last_login_at: None,
        failed_logins: 0,
        locked_until: None,
    };
    st.store.put_user(&user).await?;
    tracing::info!(actor = %a.user.email, user = %user.email, "user created");
    Ok((StatusCode::CREATED, Json(user_view(&user))))
}

/// Update a user's name, role, disabled flag, or reset their password (admin).
#[utoipa::path(patch, path = "/api/v1/users/{id}", tag = "users", request_body = UpdateUserRequest,
    params(("id" = String, Path)),
    responses((status = 200, body = User), (status = 400, body = ApiError), (status = 404, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn update(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> AppResult<Json<User>> {
    a.require_admin()?;
    let mut user = st
        .store
        .user(&id)
        .await?
        .ok_or_else(|| AppError::not_found("User"))?;
    let demoting = matches!(req.role, Some(Role::Estimator)) && user.role == Role::Admin;
    let disabling = req.disabled == Some(true) && !user.disabled;
    if (demoting || disabling) && user.role == Role::Admin {
        let other_admins = st
            .store
            .users()
            .await?
            .iter()
            .filter(|u| u.role == Role::Admin && !u.disabled && u.id != user.id)
            .count();
        if other_admins == 0 {
            return Err(AppError::bad_request("Can't remove the last active admin."));
        }
    }
    if let Some(name) = req.name {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::bad_request("Name is required."));
        }
        user.name = name;
    }
    if let Some(role) = req.role {
        user.role = role;
    }
    let mut end_sessions = false;
    if let Some(d) = req.disabled {
        end_sessions |= d && !user.disabled;
        user.disabled = d;
    }
    if let Some(pw) = req.password {
        auth::check_password_policy(&pw)?;
        user.pw_hash = auth::hash_password(pw).await?;
        user.failed_logins = 0;
        user.locked_until = None;
        end_sessions = true;
    }
    st.store.put_user(&user).await?;
    if end_sessions {
        st.store.delete_user_sessions(&user.id).await?;
    }
    tracing::info!(actor = %a.user.email, user = %user.email, "user updated");
    Ok(Json(user_view(&user)))
}
