//! Authentication: Argon2id passwords, server-side cookie sessions, and expiring API keys.
//!
//! Every request is authenticated by the `Auth` extractor, which accepts either
//! `Authorization: Bearer pdk_<id>_<secret>` or the `pd_session` cookie. Entra ID SSO will
//! slot in as another way of creating a session; handlers don't change.

use std::sync::LazyLock;

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use base64::Engine;
use chrono::{Duration, Utc};
use rand::distributions::{Alphanumeric, DistString};
use rand::RngCore;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;

use crate::error::{AppError, AppResult};
use crate::store::{SessionRec, UserPatch, UserRec};
use crate::AppState;
use api_types::Role;

pub const SESSION_COOKIE: &str = "pd_session";
pub const SESSION_HOURS: i64 = 12;
/// Sliding expiry: extend once less than this much time is left (limits writes).
const SESSION_REFRESH_HOURS: i64 = 6;
pub const API_KEY_PREFIX: &str = "pdk_";
const API_KEY_ID_LEN: usize = 10;
const API_KEY_SECRET_LEN: usize = 40;
pub const MIN_PASSWORD_LEN: usize = 10;
/// Sign-in attempts allowed before the account locks (a success resets the count).
pub const MAX_FAILED_LOGINS: u32 = 5;
pub const LOCKOUT_MINUTES: i64 = 15;
/// Only record `last_used_at` once per this interval per key.
const LAST_USED_GRANULARITY_SECS: i64 = 60;

/// Each Argon2 hash holds ~19 MiB while it runs. Capping concurrent hashes keeps a burst of
/// sign-ins from exhausting the instance's memory; extra requests wait their turn.
static HASH_PERMITS: Semaphore = Semaphore::const_new(4);

/// Hash used when the account doesn't exist, so login timing doesn't reveal valid emails.
static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| hash_password_blocking("not-a-real-password"));

pub fn sha256_hex(s: &str) -> String {
    let d = Sha256::digest(s.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn random_token() -> String {
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

pub fn random_id() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 20)
}

fn hash_password_blocking(pw: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .expect("argon2 hashing")
        .to_string()
}

pub async fn hash_password(pw: String) -> AppResult<String> {
    let _permit = HASH_PERMITS
        .acquire()
        .await
        .map_err(|e| AppError::from(anyhow::anyhow!(e)))?;
    tokio::task::spawn_blocking(move || hash_password_blocking(&pw))
        .await
        .map_err(|e| AppError::from(anyhow::anyhow!(e)))
}

pub async fn verify_password(pw: String, hash: Option<String>) -> bool {
    let Ok(_permit) = HASH_PERMITS.acquire().await else {
        return false;
    };
    tokio::task::spawn_blocking(move || {
        let real = hash.is_some();
        let h = hash.unwrap_or_else(|| DUMMY_HASH.clone());
        let ok = PasswordHash::new(&h)
            .map(|parsed| {
                Argon2::default()
                    .verify_password(pw.as_bytes(), &parsed)
                    .is_ok()
            })
            .unwrap_or(false);
        ok && real
    })
    .await
    .unwrap_or(false)
}

/// Records one more sign-in attempt and applies the lockout policy. Runs inside the store's
/// atomic update, before the password is checked, so parallel guesses each count.
pub fn login_attempt(u: &UserRec, now: chrono::DateTime<Utc>) -> UserPatch {
    match u.locked_until {
        Some(t) if t > now => UserPatch::default(),
        lock => {
            // An expired lock starts a fresh count.
            let attempts = if lock.is_some() {
                1
            } else {
                u.failed_logins + 1
            };
            UserPatch {
                failed_logins: Some(attempts),
                locked_until: Some(
                    (attempts > MAX_FAILED_LOGINS)
                        .then(|| now + Duration::minutes(LOCKOUT_MINUTES)),
                ),
                ..Default::default()
            }
        }
    }
}

pub fn check_password_policy(pw: &str) -> AppResult<()> {
    if pw.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::bad_request(format!(
            "Passwords must be at least {MIN_PASSWORD_LEN} characters."
        )));
    }
    Ok(())
}

pub fn normalize_email(e: &str) -> AppResult<String> {
    let e = e.trim().to_lowercase();
    match e.split_once('@') {
        Some((local, domain))
            if !local.is_empty() && domain.contains('.') && !e.contains(char::is_whitespace) =>
        {
            Ok(e)
        }
        _ => Err(AppError::bad_request("Enter a valid email address.")),
    }
}

pub fn session_cookie(token: String, secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time_duration(SESSION_HOURS * 3600))
        .build()
}

pub fn clear_session_cookie(secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, ""))
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time_duration(0))
        .build()
}

fn time_duration(secs: i64) -> time::Duration {
    time::Duration::seconds(secs)
}

/// Create a session for `user_id`; returns the cookie token.
pub async fn start_session(state: &AppState, user_id: &str) -> AppResult<String> {
    let token = random_token();
    let now = Utc::now();
    state
        .store
        .put_session(&SessionRec {
            id: sha256_hex(&token),
            user_id: user_id.to_string(),
            created_at: now,
            expires_at: now + Duration::hours(SESSION_HOURS),
        })
        .await?;
    Ok(token)
}

/// A new API key token and its stored hash: (`id`, `token`, `secret_hash`).
pub fn new_api_key_token() -> (String, String, String) {
    let mut rng = rand::thread_rng();
    let id = Alphanumeric.sample_string(&mut rng, API_KEY_ID_LEN);
    let secret = Alphanumeric.sample_string(&mut rng, API_KEY_SECRET_LEN);
    let token = format!("{API_KEY_PREFIX}{id}_{secret}");
    (id, token, sha256_hex(&secret))
}

pub fn api_key_prefix(id: &str) -> String {
    format!("{API_KEY_PREFIX}{id}")
}

fn parse_api_key(token: &str) -> Option<(&str, &str)> {
    let rest = token.strip_prefix(API_KEY_PREFIX)?;
    let (id, secret) = rest.split_once('_')?;
    (id.len() == API_KEY_ID_LEN && secret.len() == API_KEY_SECRET_LEN).then_some((id, secret))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Via {
    Session { id: String },
    ApiKey { id: String },
}

/// The authenticated caller.
#[derive(Debug, Clone)]
pub struct Auth {
    pub user: UserRec,
    pub via: Via,
}

impl Auth {
    pub fn is_admin(&self) -> bool {
        self.user.role == Role::Admin
    }

    pub fn require_admin(&self) -> AppResult<()> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(AppError::forbidden("Admins only."))
        }
    }

    /// Credential management (API keys, passwords) requires an interactive session so a
    /// leaked key can't mint new keys or lock out its owner.
    pub fn require_session(&self) -> AppResult<()> {
        match self.via {
            Via::Session { .. } => Ok(()),
            Via::ApiKey { .. } => Err(AppError::forbidden(
                "This action requires signing in; API keys can't do it.",
            )),
        }
    }
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Auth, AppError> {
        if let Some(h) = parts.headers.get(AUTHORIZATION) {
            let v = h.to_str().map_err(|_| AppError::unauthorized())?;
            let token = v
                .strip_prefix("Bearer ")
                .ok_or_else(AppError::unauthorized)?
                .trim();
            return auth_api_key(state, token).await;
        }
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_string())
            .ok_or_else(AppError::unauthorized)?;
        auth_session(state, &token).await
    }
}

impl axum::extract::OptionalFromRequestParts<AppState> for Auth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Option<Auth>, AppError> {
        match <Auth as FromRequestParts<AppState>>::from_request_parts(parts, state).await {
            Ok(a) => Ok(Some(a)),
            Err(e) if e.status == axum::http::StatusCode::UNAUTHORIZED => Ok(None),
            Err(e) => Err(e),
        }
    }
}

async fn auth_api_key(state: &AppState, token: &str) -> AppResult<Auth> {
    let (id, secret) = parse_api_key(token).ok_or_else(AppError::unauthorized)?;
    let key = state
        .store
        .api_key(id)
        .await?
        .ok_or_else(AppError::unauthorized)?;
    let presented = sha256_hex(secret);
    if !bool::from(presented.as_bytes().ct_eq(key.secret_hash.as_bytes())) {
        return Err(AppError::unauthorized());
    }
    let now = Utc::now();
    if key.revoked_at.is_some() || key.expires_at <= now {
        return Err(AppError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "api_key_expired",
            "This API key has expired or been revoked.",
        ));
    }
    let user = state
        .store
        .user(&key.owner_id)
        .await?
        .ok_or_else(AppError::unauthorized)?;
    if user.disabled {
        return Err(AppError::unauthorized());
    }
    if key
        .last_used_at
        .is_none_or(|t| (now - t).num_seconds() >= LAST_USED_GRANULARITY_SECS)
    {
        let store = state.store.clone();
        let mut key = key.clone();
        key.last_used_at = Some(now);
        tokio::spawn(async move {
            if let Err(e) = store.record_api_key_use(&key).await {
                tracing::warn!(error = ?e, "failed to record api key use");
            }
        });
    }
    Ok(Auth {
        user,
        via: Via::ApiKey { id: key.id },
    })
}

async fn auth_session(state: &AppState, token: &str) -> AppResult<Auth> {
    let id = sha256_hex(token);
    let mut session = state
        .store
        .session(&id)
        .await?
        .ok_or_else(AppError::unauthorized)?;
    let now = Utc::now();
    if session.expires_at <= now {
        state.store.delete_session(&id).await?;
        return Err(AppError::unauthorized());
    }
    let user = state
        .store
        .user(&session.user_id)
        .await?
        .ok_or_else(AppError::unauthorized)?;
    if user.disabled {
        return Err(AppError::unauthorized());
    }
    if session.expires_at - now < Duration::hours(SESSION_REFRESH_HOURS) {
        session.expires_at = now + Duration::hours(SESSION_HOURS);
        state.store.put_session(&session).await?;
    }
    Ok(Auth {
        user,
        via: Via::Session { id },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_round_trip() {
        let (id, token, hash) = new_api_key_token();
        let (pid, secret) = parse_api_key(&token).unwrap();
        assert_eq!(pid, id);
        assert_eq!(sha256_hex(secret), hash);
        assert!(parse_api_key("pdk_short_x").is_none());
        assert!(parse_api_key("Bearer junk").is_none());
    }

    #[tokio::test]
    async fn password_hash_verifies() {
        let h = hash_password("correct horse battery".into()).await.unwrap();
        assert!(verify_password("correct horse battery".into(), Some(h.clone())).await);
        assert!(!verify_password("wrong".into(), Some(h)).await);
        assert!(!verify_password("anything".into(), None).await);
    }

    #[test]
    fn lockout_policy() {
        let now = Utc::now();
        let mut u = crate::store::UserRec {
            id: "u".into(),
            email: "u@example.com".into(),
            name: "U".into(),
            role: Role::Estimator,
            pw_hash: String::new(),
            disabled: false,
            created_at: now,
            last_login_at: None,
            failed_logins: 0,
            locked_until: None,
        };
        for n in 1..=MAX_FAILED_LOGINS {
            login_attempt(&u, now).apply(&mut u);
            assert_eq!(u.failed_logins, n);
            assert_eq!(u.locked_until, None);
        }
        login_attempt(&u, now).apply(&mut u);
        let until = u.locked_until.expect("locked after the limit");
        // While locked, attempts change nothing.
        assert!(login_attempt(&u, now).fields().is_empty());
        // After the lock expires the count starts over.
        login_attempt(&u, until + Duration::seconds(1)).apply(&mut u);
        assert_eq!((u.failed_logins, u.locked_until), (1, None));
    }

    #[test]
    fn email_normalization() {
        assert_eq!(
            normalize_email("  Pat@Example.COM ").unwrap(),
            "pat@example.com"
        );
        assert!(normalize_email("nope").is_err());
    }
}
