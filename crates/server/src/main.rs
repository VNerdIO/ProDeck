//! ProDeck server: the `/api/v1` REST API plus the static web client.
//!
//! Configuration (environment):
//! - `PORT` (default 8080)
//! - `STORE` = `firestore` | `memory` (default: `firestore` when `GOOGLE_CLOUD_PROJECT` is set)
//! - `GOOGLE_CLOUD_PROJECT`, `FIRESTORE_DATABASE` (default `(default)`)
//! - `COOKIE_SECURE` (default `true`; set `false` for plain-http local dev)
//! - `BOOTSTRAP_ADMIN_EMAIL`, `BOOTSTRAP_ADMIN_PASSWORD`: create the first admin if no users exist
//! - `STATIC_DIR` (default `dist`): built web client

mod auth;
mod error;
mod routes;
mod store;

use std::sync::Arc;

use anyhow::Context;
use axum::http::{header, HeaderValue};
use axum::routing::get;
use axum::Router;
use chrono::Utc;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use api_types::Role;
use store::{FirestoreStore, MemoryStore, Store, UserRec};

pub struct Config {
    pub cookie_secure: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn Store>,
    pub cfg: Arc<Config>,
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

pub fn app(state: AppState, static_dir: &str) -> Router {
    let index = format!("{static_dir}/index.html");
    // Unknown paths are client-side routes: serve the app shell with 200.
    let spa = ServeDir::new(static_dir).fallback(ServeFile::new(index));
    Router::new()
        .nest("/api/v1", routes::api())
        .route("/api/openapi.json", get(routes::openapi_json))
        .route("/api/docs", get(routes::docs))
        // `/healthz` is for the container probe; Cloud Run's front end reserves it publicly,
        // so external checks use `/health`.
        .route("/healthz", get(|| async { "ok" }))
        .route("/health", get(|| async { "ok" }))
        .fallback_service(spa)
        .with_state(state)
        .layer(axum::middleware::map_response(cache_headers))
        .layer(CompressionLayer::new())
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("frame-ancestors 'none'"),
        ))
        .layer(TraceLayer::new_for_http())
}

/// Trunk fingerprints asset names, so they can be cached forever; everything else is
/// revalidated (the app shell must update immediately after a deploy).
async fn cache_headers(
    uri: axum::http::Uri,
    mut res: axum::response::Response,
) -> axum::response::Response {
    let path = uri.path();
    let fingerprinted = !path.starts_with("/api/")
        && [".wasm", ".js", ".css"]
            .iter()
            .any(|ext| path.ends_with(ext))
        && path.rsplit('/').next().is_some_and(|f| f.contains('-'));
    let value = if fingerprinted {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    res
}

async fn bootstrap_admin(store: &dyn Store) -> anyhow::Result<()> {
    if !store.users().await?.is_empty() {
        return Ok(());
    }
    let (Some(email), Some(password)) = (
        env("BOOTSTRAP_ADMIN_EMAIL"),
        env("BOOTSTRAP_ADMIN_PASSWORD"),
    ) else {
        tracing::warn!("no users exist; set BOOTSTRAP_ADMIN_EMAIL and BOOTSTRAP_ADMIN_PASSWORD to create the first admin");
        return Ok(());
    };
    let email = auth::normalize_email(&email).map_err(|e| anyhow::anyhow!(e.body.message))?;
    auth::check_password_policy(&password).map_err(|e| anyhow::anyhow!(e.body.message))?;
    let user = UserRec {
        id: auth::random_id(),
        name: "Administrator".into(),
        role: Role::Admin,
        pw_hash: auth::hash_password(password)
            .await
            .map_err(|e| anyhow::anyhow!(e.body.message))?,
        disabled: false,
        created_at: Utc::now(),
        last_login_at: None,
        failed_logins: 0,
        locked_until: None,
        email,
    };
    store.put_user(&user).await?;
    tracing::info!(email = %user.email, "created bootstrap admin");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let json_logs = env("K_SERVICE").is_some(); // set by Cloud Run
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,tower_http=info,firestore=warn".into());
    if json_logs {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    let project = env("GOOGLE_CLOUD_PROJECT");
    let kind = env("STORE").unwrap_or_else(|| {
        if project.is_some() {
            "firestore".into()
        } else {
            "memory".into()
        }
    });
    let store: Arc<dyn Store> = match kind.as_str() {
        "firestore" => {
            let project =
                project.context("GOOGLE_CLOUD_PROJECT is required for the firestore store")?;
            let db = env("FIRESTORE_DATABASE").unwrap_or_else(|| "(default)".into());
            Arc::new(FirestoreStore::connect(&project, &db).await?)
        }
        "memory" => {
            tracing::warn!("using in-memory store: data is lost on restart");
            Arc::new(MemoryStore::default())
        }
        other => anyhow::bail!("unknown STORE {other:?}"),
    };
    bootstrap_admin(store.as_ref()).await?;

    let state = AppState {
        store,
        cfg: Arc::new(Config {
            cookie_secure: env("COOKIE_SECURE").is_none_or(|v| v != "false"),
        }),
    };
    let static_dir = env("STATIC_DIR").unwrap_or_else(|| "dist".into());
    let port = env("PORT").unwrap_or_else(|| "8080".into());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!(%port, store = %kind, "listening");
    axum::serve(listener, app(state, &static_dir))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
