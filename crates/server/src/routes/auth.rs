use std::sync::Arc;

use api_types::{ApiError, ChangePasswordRequest, LoginRequest, User};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use axum_extra::extract::CookieJar;
use chrono::Utc;

use crate::auth::{self, Auth, Via};
use crate::error::{AppError, AppResult};
use crate::routes::user_view;
use crate::store::UserPatch;
use crate::AppState;

/// Sign in with email and password; sets the `pd_session` cookie.
#[utoipa::path(post, path = "/api/v1/auth/login", tag = "auth", request_body = LoginRequest,
    responses((status = 200, body = User), (status = 401, body = ApiError), (status = 429, body = ApiError)))]
pub async fn login(
    State(st): State<AppState>,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> AppResult<(CookieJar, Json<User>)> {
    let invalid = || {
        AppError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "Email or password is incorrect.",
        )
    };
    let email = auth::normalize_email(&req.email).map_err(|_| invalid())?;
    let Some(user) = st.store.user_by_email(&email).await? else {
        // Same password work as a real account, so timing doesn't reveal which emails exist.
        auth::verify_password(req.password, None).await;
        return Err(invalid());
    };
    let now = Utc::now();
    let user = st
        .store
        .update_user(&user.id, Arc::new(move |u| auth::login_attempt(u, now)))
        .await?
        .ok_or_else(invalid)?;
    if user.locked_until.is_some_and(|t| t > now) {
        return Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "locked",
            format!(
                "Too many failed sign-ins. Try again in {} minutes.",
                auth::LOCKOUT_MINUTES
            ),
        ));
    }
    if !auth::verify_password(req.password, Some(user.pw_hash.clone())).await || user.disabled {
        return Err(invalid());
    }
    st.store
        .patch_user(
            &user.id,
            UserPatch {
                failed_logins: Some(0),
                locked_until: Some(None),
                last_login_at: Some(now),
                ..Default::default()
            },
        )
        .await?;
    let token = auth::start_session(&st, &user.id).await?;
    // An admin may have reset the password or disabled the account while it was being checked.
    // Their change ends existing sessions, so make sure this new one isn't left behind.
    let current = st.store.user(&user.id).await?;
    if current.is_none_or(|c| c.pw_hash != user.pw_hash || c.disabled) {
        st.store.delete_session(&auth::sha256_hex(&token)).await?;
        return Err(invalid());
    }
    Ok((
        jar.add(auth::session_cookie(token, st.cfg.cookie_secure)),
        Json(user_view(&user)),
    ))
}

/// End the current session.
#[utoipa::path(post, path = "/api/v1/auth/logout", tag = "auth", responses((status = 204)))]
pub async fn logout(
    State(st): State<AppState>,
    jar: CookieJar,
    a: Option<Auth>,
) -> AppResult<(CookieJar, StatusCode)> {
    if let Some(Auth {
        via: Via::Session { id },
        ..
    }) = a
    {
        st.store.delete_session(&id).await?;
    }
    Ok((
        jar.add(auth::clear_session_cookie(st.cfg.cookie_secure)),
        StatusCode::NO_CONTENT,
    ))
}

/// The signed-in user (or the owner of the API key).
#[utoipa::path(get, path = "/api/v1/me", tag = "auth",
    responses((status = 200, body = User), (status = 401, body = ApiError)),
    security(("session" = []), ("api_key" = [])))]
pub async fn me(a: Auth) -> Json<User> {
    Json(user_view(&a.user))
}

/// Change your own password. Signs out every other session; this device gets a fresh one.
#[utoipa::path(post, path = "/api/v1/me/password", tag = "auth", request_body = ChangePasswordRequest,
    responses((status = 204), (status = 400, body = ApiError), (status = 403, body = ApiError)),
    security(("session" = [])))]
pub async fn change_password(
    State(st): State<AppState>,
    jar: CookieJar,
    a: Auth,
    Json(req): Json<ChangePasswordRequest>,
) -> AppResult<(CookieJar, StatusCode)> {
    a.require_session()?;
    if !auth::verify_password(req.current_password, Some(a.user.pw_hash.clone())).await {
        return Err(AppError::bad_request("Current password is incorrect."));
    }
    auth::check_password_policy(&req.new_password)?;
    let pw_hash = auth::hash_password(req.new_password).await?;
    st.store
        .patch_user(
            &a.user.id,
            UserPatch {
                pw_hash: Some(pw_hash),
                ..Default::default()
            },
        )
        .await?;
    st.store.delete_user_sessions(&a.user.id).await?;
    let token = auth::start_session(&st, &a.user.id).await?;
    Ok((
        jar.add(auth::session_cookie(token, st.cfg.cookie_secure)),
        StatusCode::NO_CONTENT,
    ))
}
