//! API tests against the in-memory store.

use std::sync::Arc;

use api_types::*;
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::store::{MemoryStore, Store, UserRec};
use crate::{app, auth, AppState, Config};

const ADMIN_PW: &str = "admin-password-1";

struct T {
    app: Router,
    store: Arc<MemoryStore>,
}

enum Cred<'a> {
    None,
    Cookie(&'a str),
    Bearer(&'a str),
}

struct Resp {
    status: StatusCode,
    cookie: Option<String>,
    body: Value,
}

impl Resp {
    fn json<D: DeserializeOwned>(&self) -> D {
        serde_json::from_value(self.body.clone()).unwrap_or_else(|e| panic!("{e}: {}", self.body))
    }
}

impl T {
    async fn new() -> T {
        let store = Arc::new(MemoryStore::default());
        store
            .put_user(&UserRec {
                id: "admin".into(),
                email: "admin@example.com".into(),
                name: "Admin".into(),
                role: Role::Admin,
                pw_hash: auth::hash_password(ADMIN_PW.into()).await.unwrap(),
                disabled: false,
                created_at: Utc::now(),
                last_login_at: None,
                failed_logins: 0,
                locked_until: None,
            })
            .await
            .unwrap();
        let state = AppState {
            store: store.clone(),
            cfg: Arc::new(Config {
                cookie_secure: false,
            }),
        };
        T {
            app: app(state, "does-not-exist"),
            store,
        }
    }

    async fn call(&self, method: Method, path: &str, cred: Cred<'_>, body: Option<Value>) -> Resp {
        let mut req = Request::builder().method(method).uri(path);
        match cred {
            Cred::None => {}
            Cred::Cookie(c) => {
                req = req.header(header::COOKIE, format!("{}={c}", auth::SESSION_COOKIE))
            }
            Cred::Bearer(t) => req = req.header(header::AUTHORIZATION, format!("Bearer {t}")),
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let cookie = res
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find_map(|v| v.strip_prefix(&format!("{}=", auth::SESSION_COOKIE)))
            .map(|v| v.split(';').next().unwrap().to_string())
            .filter(|v| !v.is_empty());
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Resp {
            status,
            cookie,
            body,
        }
    }

    async fn login(&self, email: &str, pw: &str) -> String {
        let r = self
            .call(
                Method::POST,
                "/api/v1/auth/login",
                Cred::None,
                Some(json!({"email": email, "password": pw})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        r.cookie.expect("session cookie")
    }

    async fn create_user(&self, admin: &str, email: &str, role: &str) -> User {
        let r = self
            .call(
                Method::POST,
                "/api/v1/users",
                Cred::Cookie(admin),
                Some(json!({"email": email, "name": "Pat", "role": role, "password": "user-password-1"})),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        r.json()
    }
}

fn deck() -> Value {
    serde_json::from_str(include_str!(
        "../../estimator/tests/fixtures/attached_12x16_pt.json"
    ))
    .unwrap()
}

#[tokio::test]
async fn login_me_logout() {
    let t = T::new().await;
    assert_eq!(
        t.call(Method::GET, "/api/v1/me", Cred::None, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let c = t.login("ADMIN@example.com ", ADMIN_PW).await;
    let me: User = t
        .call(Method::GET, "/api/v1/me", Cred::Cookie(&c), None)
        .await
        .json();
    assert_eq!(me.role, Role::Admin);
    assert_eq!(
        t.call(Method::POST, "/api/v1/auth/logout", Cred::Cookie(&c), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        t.call(Method::GET, "/api/v1/me", Cred::Cookie(&c), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn lockout_after_repeated_failures() {
    let t = T::new().await;
    let bad = json!({"email": "admin@example.com", "password": "wrong-password"});
    for _ in 0..auth::MAX_FAILED_LOGINS {
        let r = t
            .call(
                Method::POST,
                "/api/v1/auth/login",
                Cred::None,
                Some(bad.clone()),
            )
            .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    let good = json!({"email": "admin@example.com", "password": ADMIN_PW});
    let r = t
        .call(Method::POST, "/api/v1/auth/login", Cred::None, Some(good))
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    // Unknown accounts get the same answer as a wrong password.
    let r = t
        .call(
            Method::POST,
            "/api/v1/auth/login",
            Cred::None,
            Some(json!({"email": "nobody@example.com", "password": "x"})),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.body["code"], "invalid_credentials");
}

#[tokio::test]
async fn user_admin_and_roles() {
    let t = T::new().await;
    let admin = t.login("admin@example.com", ADMIN_PW).await;
    let pat = t.create_user(&admin, "pat@example.com", "estimator").await;
    let dup = t
        .call(
            Method::POST,
            "/api/v1/users",
            Cred::Cookie(&admin),
            Some(json!({"email": "PAT@example.com", "name": "Dup", "role": "estimator", "password": "user-password-1"})),
        )
        .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);

    let pc = t.login("pat@example.com", "user-password-1").await;
    assert_eq!(
        t.call(Method::GET, "/api/v1/users", Cred::Cookie(&pc), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    // Can't remove the last admin.
    let r = t
        .call(
            Method::PATCH,
            "/api/v1/users/admin",
            Cred::Cookie(&admin),
            Some(json!({"role": "estimator"})),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // Disabling ends sessions.
    let r = t
        .call(
            Method::PATCH,
            &format!("/api/v1/users/{}", pat.id),
            Cred::Cookie(&admin),
            Some(json!({"disabled": true})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(
        t.call(Method::GET, "/api/v1/me", Cred::Cookie(&pc), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn api_key_lifecycle() {
    let t = T::new().await;
    let admin = t.login("admin@example.com", ADMIN_PW).await;
    let pat = t.create_user(&admin, "pat@example.com", "estimator").await;
    let pc = t.login("pat@example.com", "user-password-1").await;

    // Expiry is mandatory and capped.
    for bad in [
        json!({"name": "ci"}),
        json!({"name": "ci", "expires_in_days": 400}),
        json!({"name": "ci", "expires_in_days": 0}),
    ] {
        let r = t
            .call(
                Method::POST,
                "/api/v1/api-keys",
                Cred::Cookie(&pc),
                Some(bad),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.body);
    }
    let r = t
        .call(
            Method::POST,
            "/api/v1/api-keys",
            Cred::Cookie(&pc),
            Some(json!({"name": "Estimating sheet", "expires_in_days": 90})),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
    let created: CreatedApiKey = r.json();
    assert!(created.token.starts_with(&created.key.prefix));
    assert_eq!(created.key.status, ApiKeyStatus::Active);
    let days = (created.key.expires_at - Utc::now()).num_days();
    assert!((89..=90).contains(&days));

    // The key works for normal API calls and acts as its owner.
    let me: User = t
        .call(
            Method::GET,
            "/api/v1/me",
            Cred::Bearer(&created.token),
            None,
        )
        .await
        .json();
    assert_eq!(me.id, pat.id);
    let r = t
        .call(
            Method::POST,
            "/api/v1/estimate",
            Cred::Bearer(&created.token),
            Some(deck()),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);

    // ...but can't mint more keys or reach admin endpoints.
    let r = t
        .call(
            Method::POST,
            "/api/v1/api-keys",
            Cred::Bearer(&created.token),
            Some(json!({"name": "x", "expires_in_days": 1})),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(
        t.call(
            Method::GET,
            "/api/v1/users",
            Cred::Bearer(&created.token),
            None
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );

    // A tampered secret fails.
    let last = created.token.chars().last().unwrap();
    let tampered = format!(
        "{}{}",
        &created.token[..created.token.len() - 1],
        if last == 'x' { 'y' } else { 'x' }
    );
    assert_eq!(
        t.call(Method::GET, "/api/v1/me", Cred::Bearer(&tampered), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );

    // Listing: owner sees it; admin sees it with ?all=true.
    let mine: Vec<ApiKey> = t
        .call(Method::GET, "/api/v1/api-keys", Cred::Cookie(&pc), None)
        .await
        .json();
    assert_eq!(mine.len(), 1);
    let all: Vec<ApiKey> = t
        .call(
            Method::GET,
            "/api/v1/api-keys?all=true",
            Cred::Cookie(&admin),
            None,
        )
        .await
        .json();
    assert_eq!(all[0].owner_email, "pat@example.com");
    assert_eq!(
        t.call(
            Method::GET,
            "/api/v1/api-keys?all=true",
            Cred::Cookie(&pc),
            None
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );

    // Revoke takes effect immediately.
    let r = t
        .call(
            Method::DELETE,
            &format!("/api/v1/api-keys/{}", created.key.id),
            Cred::Cookie(&pc),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = t
        .call(
            Method::GET,
            "/api/v1/me",
            Cred::Bearer(&created.token),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.body["code"], "api_key_expired");
    let listed: Vec<ApiKey> = t
        .call(Method::GET, "/api/v1/api-keys", Cred::Cookie(&pc), None)
        .await
        .json();
    assert_eq!(listed[0].status, ApiKeyStatus::Revoked);
}

#[tokio::test]
async fn expired_key_and_disabled_owner_are_rejected() {
    let t = T::new().await;
    let admin = t.login("admin@example.com", ADMIN_PW).await;
    let pat = t.create_user(&admin, "pat@example.com", "estimator").await;
    let pc = t.login("pat@example.com", "user-password-1").await;
    let created: CreatedApiKey = t
        .call(
            Method::POST,
            "/api/v1/api-keys",
            Cred::Cookie(&pc),
            Some(json!({"name": "k", "expires_in_days": 30})),
        )
        .await
        .json();

    // Force expiry.
    let mut rec = t.store.api_key(&created.key.id).await.unwrap().unwrap();
    rec.expires_at = Utc::now() - Duration::seconds(1);
    t.store.put_api_key(&rec).await.unwrap();
    assert_eq!(
        t.call(
            Method::GET,
            "/api/v1/me",
            Cred::Bearer(&created.token),
            None
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    let listed: Vec<ApiKey> = t
        .call(Method::GET, "/api/v1/api-keys", Cred::Cookie(&pc), None)
        .await
        .json();
    assert_eq!(listed[0].status, ApiKeyStatus::Expired);

    // A live key stops working when its owner is disabled.
    let k2: CreatedApiKey = t
        .call(
            Method::POST,
            "/api/v1/api-keys",
            Cred::Cookie(&pc),
            Some(json!({"name": "k2", "expires_in_days": 30})),
        )
        .await
        .json();
    t.call(
        Method::PATCH,
        &format!("/api/v1/users/{}", pat.id),
        Cred::Cookie(&admin),
        Some(json!({"disabled": true})),
    )
    .await;
    assert_eq!(
        t.call(Method::GET, "/api/v1/me", Cred::Bearer(&k2.token), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn projects_crud_estimate_issue() {
    let t = T::new().await;
    let admin = t.login("admin@example.com", ADMIN_PW).await;
    t.create_user(&admin, "pat@example.com", "estimator").await;
    let pc = t.login("pat@example.com", "user-password-1").await;

    let body = json!({"name": "Smith deck", "customer": "J. Smith", "address": "12 Oak St", "input": deck()});
    let r = t
        .call(
            Method::POST,
            "/api/v1/projects",
            Cred::Cookie(&pc),
            Some(body),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    let p: ProjectDetail = r.json();
    assert_eq!(p.status, ProjectStatus::Draft);

    let list: Vec<ProjectSummary> = t
        .call(
            Method::GET,
            "/api/v1/projects?q=oak",
            Cred::Cookie(&admin),
            None,
        )
        .await
        .json();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].deck_area_sqft, 192.0);
    let none: Vec<ProjectSummary> = t
        .call(
            Method::GET,
            "/api/v1/projects?mine=true",
            Cred::Cookie(&admin),
            None,
        )
        .await
        .json();
    assert!(none.is_empty());

    let est: Estimate = t
        .call(
            Method::GET,
            &format!("/api/v1/projects/{}/estimate", p.id),
            Cred::Cookie(&pc),
            None,
        )
        .await
        .json();
    assert!(!est.line_items.is_empty());

    let r = t
        .call(
            Method::POST,
            &format!("/api/v1/projects/{}/issued", p.id),
            Cred::Cookie(&pc),
            Some(json!({"label": "Quote"})),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
    let issued: IssuedEstimate = r.json();

    // Editing the design doesn't change the issued snapshot.
    let mut input = deck();
    input["sections"][0]["width"] = json!(240);
    let r = t
        .call(
            Method::PUT,
            &format!("/api/v1/projects/{}", p.id),
            Cred::Cookie(&pc),
            Some(json!({"name": "Smith deck", "input": input})),
        )
        .await;
    let updated: ProjectDetail = r.json();
    assert_eq!(updated.status, ProjectStatus::Issued);
    let snap: IssuedEstimate = t
        .call(
            Method::GET,
            &format!("/api/v1/projects/{}/issued/{}", p.id, issued.meta.id),
            Cred::Cookie(&pc),
            None,
        )
        .await
        .json();
    assert_eq!(snap.input.sections[0].width, 192.0);

    // Duplicate belongs to the caller.
    let dup: ProjectDetail = t
        .call(
            Method::POST,
            &format!("/api/v1/projects/{}/duplicate", p.id),
            Cred::Cookie(&admin),
            None,
        )
        .await
        .json();
    assert_eq!(dup.owner_id, "admin");
    assert!(dup.name.ends_with("(copy)"));

    // Only owner/admin can delete.
    let r = t
        .call(
            Method::DELETE,
            &format!("/api/v1/projects/{}", dup.id),
            Cred::Cookie(&pc),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = t
        .call(
            Method::DELETE,
            &format!("/api/v1/projects/{}", p.id),
            Cred::Cookie(&pc),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert!(t.store.issued_for(&p.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn invalid_design_is_422_with_details() {
    let t = T::new().await;
    let c = t.login("admin@example.com", ADMIN_PW).await;
    let mut d = deck();
    d["sections"][0]["ledger_edge"] = json!("east");
    let r = t
        .call(Method::POST, "/api/v1/estimate", Cred::Cookie(&c), Some(d))
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        r.body["estimate_error"]["code"],
        "ledger_parallel_to_joists"
    );
}

#[tokio::test]
async fn openapi_lists_every_route() {
    let t = T::new().await;
    let r = t
        .call(Method::GET, "/api/openapi.json", Cred::None, None)
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let paths = r.body["paths"].as_object().unwrap();
    for p in [
        "/api/v1/auth/login",
        "/api/v1/api-keys",
        "/api/v1/api-keys/{id}",
        "/api/v1/projects/{id}/issued",
        "/api/v1/estimate",
    ] {
        assert!(paths.contains_key(p), "missing {p}");
    }
    assert!(r.body["components"]["securitySchemes"]["api_key"].is_object());
}

#[tokio::test]
async fn parallel_guesses_all_count_toward_lockout() {
    let t = T::new().await;
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let app = t.app.clone();
        set.spawn(async move {
            let req = Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"email": "admin@example.com", "password": "wrong-password"}).to_string(),
                ))
                .unwrap();
            app.oneshot(req).await.unwrap().status()
        });
    }
    let statuses = set.join_all().await;
    let checked = statuses
        .iter()
        .filter(|s| **s == StatusCode::UNAUTHORIZED)
        .count();
    assert_eq!(checked, auth::MAX_FAILED_LOGINS as usize, "{statuses:?}");
    let r = t
        .call(
            Method::POST,
            "/api/v1/auth/login",
            Cred::None,
            Some(json!({"email": "admin@example.com", "password": ADMIN_PW})),
        )
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn admin_api_key_cannot_manage_credentials_or_roles() {
    let t = T::new().await;
    let admin = t.login("admin@example.com", ADMIN_PW).await;
    let pat = t.create_user(&admin, "pat@example.com", "estimator").await;
    let key: CreatedApiKey = t
        .call(
            Method::POST,
            "/api/v1/api-keys",
            Cred::Cookie(&admin),
            Some(json!({"name": "admin automation", "expires_in_days": 30})),
        )
        .await
        .json();
    let path = format!("/api/v1/users/{}", pat.id);
    for body in [
        json!({"password": "taken-over-1234"}),
        json!({"role": "admin"}),
    ] {
        let r = t
            .call(Method::PATCH, &path, Cred::Bearer(&key.token), Some(body))
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    }
    let r = t
        .call(
            Method::POST,
            "/api/v1/users",
            Cred::Bearer(&key.token),
            Some(json!({"email": "eve@example.com", "name": "Eve", "role": "admin", "password": "user-password-1"})),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // Offboarding automation still works.
    let r = t
        .call(
            Method::PATCH,
            &path,
            Cred::Bearer(&key.token),
            Some(json!({"disabled": true})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let r = t
        .call(
            Method::POST,
            "/api/v1/auth/login",
            Cred::None,
            Some(json!({"email": "pat@example.com", "password": "user-password-1"})),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn recording_key_use_cannot_undo_a_revoke() {
    let t = T::new().await;
    let admin = t.login("admin@example.com", ADMIN_PW).await;
    let created: CreatedApiKey = t
        .call(
            Method::POST,
            "/api/v1/api-keys",
            Cred::Cookie(&admin),
            Some(json!({"name": "k", "expires_in_days": 30})),
        )
        .await
        .json();
    // A request read the key before the revoke and records its use afterwards.
    let mut stale = t.store.api_key(&created.key.id).await.unwrap().unwrap();
    let r = t
        .call(
            Method::DELETE,
            &format!("/api/v1/api-keys/{}", created.key.id),
            Cred::Cookie(&admin),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    stale.last_used_at = Some(Utc::now());
    t.store.record_api_key_use(&stale).await.unwrap();
    let r = t
        .call(
            Method::GET,
            "/api/v1/me",
            Cred::Bearer(&created.token),
            None,
        )
        .await;
    assert_eq!(r.body["code"], "api_key_expired");
}
