//! Persistence. `Store` is implemented by Firestore (production) and an in-memory map
//! (tests and local runs without the emulator).

mod firestore;
mod memory;

use api_types::{IssuedEstimate, Role};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use estimator::Project;
use serde::{Deserialize, Serialize};

pub use self::firestore::FirestoreStore;
pub use memory::MemoryStore;

pub type Result<T> = anyhow::Result<T>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserRec {
    pub id: String,
    /// Lowercased.
    pub email: String,
    pub name: String,
    pub role: Role,
    pub pw_hash: String,
    pub disabled: bool,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub failed_logins: u32,
    #[serde(default)]
    pub locked_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionRec {
    /// SHA-256 of the cookie token (the token itself is never stored).
    pub id: String,
    pub user_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiKeyRec {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    /// SHA-256 of the secret part of the token, hex.
    pub secret_hash: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectRec {
    pub id: String,
    pub name: String,
    pub customer: String,
    pub address: String,
    pub notes: String,
    pub owner_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub input: Project,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IssuedRec {
    pub id: String,
    pub project_id: String,
    pub issued: IssuedEstimate,
}

#[async_trait]
pub trait Store: Send + Sync + 'static {
    async fn user(&self, id: &str) -> Result<Option<UserRec>>;
    async fn user_by_email(&self, email: &str) -> Result<Option<UserRec>>;
    async fn users(&self) -> Result<Vec<UserRec>>;
    async fn put_user(&self, u: &UserRec) -> Result<()>;

    async fn session(&self, id: &str) -> Result<Option<SessionRec>>;
    async fn put_session(&self, s: &SessionRec) -> Result<()>;
    async fn delete_session(&self, id: &str) -> Result<()>;
    async fn delete_user_sessions(&self, user_id: &str) -> Result<()>;

    async fn api_key(&self, id: &str) -> Result<Option<ApiKeyRec>>;
    /// All keys, or only `owner`'s.
    async fn api_keys(&self, owner: Option<&str>) -> Result<Vec<ApiKeyRec>>;
    async fn put_api_key(&self, k: &ApiKeyRec) -> Result<()>;

    async fn project(&self, id: &str) -> Result<Option<ProjectRec>>;
    async fn projects(&self) -> Result<Vec<ProjectRec>>;
    async fn put_project(&self, p: &ProjectRec) -> Result<()>;
    /// Deletes the project and its issued estimates.
    async fn delete_project(&self, id: &str) -> Result<()>;

    async fn issued(&self, id: &str) -> Result<Option<IssuedRec>>;
    async fn issued_for(&self, project_id: &str) -> Result<Vec<IssuedRec>>;
    async fn put_issued(&self, i: &IssuedRec) -> Result<()>;
}
