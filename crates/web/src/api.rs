//! Thin client for the ProDeck REST API. The web app uses nothing the public API doesn't offer.

use api_types::*;
use gloo_net::http::{Request, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub status: u16,
    pub body: ApiError,
}

impl Error {
    pub fn message(&self) -> String {
        self.body.message.clone()
    }

    fn network(e: impl std::fmt::Display) -> Error {
        Error {
            status: 0,
            body: ApiError {
                code: "network".into(),
                message: format!("Can't reach the server ({e})."),
                estimate_error: None,
            },
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

const BASE: &str = "/api/v1";

async fn finish<T: DeserializeOwned>(
    r: std::result::Result<Response, gloo_net::Error>,
) -> Result<T> {
    let r = r.map_err(Error::network)?;
    if r.ok() {
        if r.status() == 204 {
            return serde_json::from_value(serde_json::Value::Null).map_err(Error::network);
        }
        return r.json::<T>().await.map_err(Error::network);
    }
    let status = r.status();
    let body = r.json::<ApiError>().await.unwrap_or(ApiError {
        code: "http".into(),
        message: format!("Request failed ({status})."),
        estimate_error: None,
    });
    if status == 401 {
        crate::app::on_unauthorized();
    }
    Err(Error { status, body })
}

async fn send<T: DeserializeOwned>(b: RequestBuilder) -> Result<T> {
    finish(b.send().await).await
}

async fn send_json<T: DeserializeOwned, B: Serialize>(b: RequestBuilder, body: &B) -> Result<T> {
    let req = b.json(body).map_err(Error::network)?;
    finish(req.send().await).await
}

fn url(path: &str) -> String {
    format!("{BASE}{path}")
}

/// Percent-encode a query value.
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ---- auth ----

pub async fn login(email: &str, password: &str) -> Result<User> {
    send_json(
        Request::post(&url("/auth/login")),
        &LoginRequest {
            email: email.into(),
            password: password.into(),
        },
    )
    .await
}

pub async fn logout() -> Result<()> {
    send(Request::post(&url("/auth/logout"))).await
}

pub async fn me() -> Result<User> {
    send(Request::get(&url("/me"))).await
}

pub async fn change_password(current: &str, new: &str) -> Result<()> {
    send_json(
        Request::post(&url("/me/password")),
        &ChangePasswordRequest {
            current_password: current.into(),
            new_password: new.into(),
        },
    )
    .await
}

// ---- users ----

pub async fn users() -> Result<Vec<User>> {
    send(Request::get(&url("/users"))).await
}

pub async fn create_user(req: &CreateUserRequest) -> Result<User> {
    send_json(Request::post(&url("/users")), req).await
}

pub async fn update_user(id: &str, req: &UpdateUserRequest) -> Result<User> {
    send_json(Request::patch(&url(&format!("/users/{id}"))), req).await
}

// ---- api keys ----

pub async fn api_keys(all: bool) -> Result<Vec<ApiKey>> {
    send(Request::get(&url(if all {
        "/api-keys?all=true"
    } else {
        "/api-keys"
    })))
    .await
}

pub async fn create_api_key(req: &CreateApiKeyRequest) -> Result<CreatedApiKey> {
    send_json(Request::post(&url("/api-keys")), req).await
}

pub async fn revoke_api_key(id: &str) -> Result<()> {
    send(Request::delete(&url(&format!("/api-keys/{id}")))).await
}

// ---- projects ----

pub async fn projects(q: &str, mine: bool) -> Result<Vec<ProjectSummary>> {
    send(Request::get(&url(&format!(
        "/projects?q={}&mine={mine}",
        enc(q)
    ))))
    .await
}

pub async fn project(id: &str) -> Result<ProjectDetail> {
    send(Request::get(&url(&format!("/projects/{id}")))).await
}

pub async fn create_project(req: &SaveProjectRequest) -> Result<ProjectDetail> {
    send_json(Request::post(&url("/projects")), req).await
}

pub async fn save_project(id: &str, req: &SaveProjectRequest) -> Result<ProjectDetail> {
    send_json(Request::put(&url(&format!("/projects/{id}"))), req).await
}

pub async fn delete_project(id: &str) -> Result<()> {
    send(Request::delete(&url(&format!("/projects/{id}")))).await
}

pub async fn duplicate_project(id: &str) -> Result<ProjectDetail> {
    send(Request::post(&url(&format!("/projects/{id}/duplicate")))).await
}

pub async fn issue(id: &str, label: &str) -> Result<IssuedEstimate> {
    send_json(
        Request::post(&url(&format!("/projects/{id}/issued"))),
        &IssueRequest {
            label: label.into(),
        },
    )
    .await
}

pub async fn issued(id: &str, issued_id: &str) -> Result<IssuedEstimate> {
    send(Request::get(&url(&format!(
        "/projects/{id}/issued/{issued_id}"
    ))))
    .await
}

pub async fn estimate(p: &Project) -> Result<Estimate> {
    send_json(Request::post(&url("/estimate")), p).await
}
