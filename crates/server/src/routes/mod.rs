//! `/api/v1` routes and the OpenAPI document generated from them.

mod api_keys;
mod auth;
mod projects;
mod users;

use api_types::User;
use axum::response::Html;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::store::UserRec;
use crate::AppState;

pub(crate) fn user_view(u: &UserRec) -> User {
    User {
        id: u.id.clone(),
        email: u.email.clone(),
        name: u.name.clone(),
        role: u.role,
        disabled: u.disabled,
        created_at: u.created_at,
        last_login_at: u.last_login_at,
    }
}

pub fn api() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/me", get(auth::me))
        .route("/me/password", post(auth::change_password))
        .route("/users", get(users::list).post(users::create))
        .route("/users/{id}", patch(users::update))
        .route("/api-keys", get(api_keys::list).post(api_keys::create))
        .route("/api-keys/{id}", axum::routing::delete(api_keys::revoke))
        .route("/estimate", post(projects::estimate_adhoc))
        .route("/projects", get(projects::list).post(projects::create))
        .route(
            "/projects/{id}",
            get(projects::get)
                .put(projects::update)
                .delete(projects::delete),
        )
        .route("/projects/{id}/duplicate", post(projects::duplicate))
        .route("/projects/{id}/estimate", get(projects::estimate))
        .route("/projects/{id}/issued", post(projects::issue))
        .route(
            "/projects/{id}/issued/{issued_id}",
            get(projects::get_issued),
        )
}

struct Security;

impl Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let c = openapi.components.get_or_insert_with(Default::default);
        c.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                crate::auth::SESSION_COOKIE,
                "Set by POST /api/v1/auth/login.",
            ))),
        );
        c.add_security_scheme(
            "api_key",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("pdk_<id>_<secret>")
                    .description(Some(
                        "Create keys under Settings > API Keys. Every key expires (max 365 days).",
                    ))
                    .build(),
            ),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "ProDeck API",
        description = "Deck material estimation. Every feature of the ProDeck web app is available here.\n\n\
            Authenticate with a session cookie (`POST /api/v1/auth/login`) or an API key \
            (`Authorization: Bearer pdk_...`). Lengths are in inches.",
        version = "1.0.0"
    ),
    paths(
        auth::login, auth::logout, auth::me, auth::change_password,
        users::list, users::create, users::update,
        api_keys::list, api_keys::create, api_keys::revoke,
        projects::estimate_adhoc,
        projects::list, projects::create, projects::get, projects::update, projects::delete,
        projects::duplicate, projects::estimate, projects::issue, projects::get_issued,
    ),
    modifiers(&Security),
    tags(
        (name = "auth", description = "Sign in, sessions, and your account"),
        (name = "users", description = "User administration (admins)"),
        (name = "api-keys", description = "Expiring API keys for integrations"),
        (name = "projects", description = "Saved deck projects and issued estimates"),
        (name = "estimate", description = "Stateless estimation"),
    )
)]
pub struct ApiDoc;

pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// Swagger UI from a pinned CDN build, pointed at our spec.
pub async fn docs() -> Html<&'static str> {
    Html(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ProDeck API</title>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.17.14/swagger-ui.css">
</head>
<body>
<div id="ui"></div>
<script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.17.14/swagger-ui-bundle.js"></script>
<script>SwaggerUIBundle({ url: "/api/openapi.json", dom_id: "#ui", withCredentials: true });</script>
</body>
</html>"##,
    )
}
