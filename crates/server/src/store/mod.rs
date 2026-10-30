//! Persistence. `Store` is implemented by Firestore (production) and an in-memory map
//! (tests and local runs without the emulator).

mod firestore;
mod memory;

use std::sync::Arc;

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

/// Changes to an existing user. Only the `Some` fields are written, so concurrent writers
/// (a sign-in and an admin disabling the account) can't undo each other's changes.
#[derive(Debug, Clone, Default)]
pub struct UserPatch {
    pub name: Option<String>,
    pub role: Option<Role>,
    pub pw_hash: Option<String>,
    pub disabled: Option<bool>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub failed_logins: Option<u32>,
    pub locked_until: Option<Option<DateTime<Utc>>>,
}

impl UserPatch {
    /// Stored names of the fields this patch writes.
    pub fn fields(&self) -> Vec<&'static str> {
        [
            ("name", self.name.is_some()),
            ("role", self.role.is_some()),
            ("pw_hash", self.pw_hash.is_some()),
            ("disabled", self.disabled.is_some()),
            ("last_login_at", self.last_login_at.is_some()),
            ("failed_logins", self.failed_logins.is_some()),
            ("locked_until", self.locked_until.is_some()),
        ]
        .into_iter()
        .filter_map(|(f, set)| set.then_some(f))
        .collect()
    }

    pub fn apply(&self, u: &mut UserRec) {
        if let Some(v) = &self.name {
            u.name = v.clone();
        }
        if let Some(v) = self.role {
            u.role = v;
        }
        if let Some(v) = &self.pw_hash {
            u.pw_hash = v.clone();
        }
        if let Some(v) = self.disabled {
            u.disabled = v;
        }
        if let Some(v) = self.last_login_at {
            u.last_login_at = Some(v);
        }
        if let Some(v) = self.failed_logins {
            u.failed_logins = v;
        }
        if let Some(v) = self.locked_until {
            u.locked_until = v;
        }
    }
}

/// Computes a patch from the user's current record inside an atomic read-modify-write.
/// May run more than once if the store retries.
pub type UserUpdateFn = Arc<dyn Fn(&UserRec) -> UserPatch + Send + Sync>;

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
    /// Creates (or wholly replaces) a user. Use `update_user`/`patch_user` for changes.
    async fn put_user(&self, u: &UserRec) -> Result<()>;
    /// Atomically reads the user, computes a patch with `f`, and writes only the patched
    /// fields. Returns the updated record, or `None` if there is no such user.
    async fn update_user(&self, id: &str, f: UserUpdateFn) -> Result<Option<UserRec>>;
    async fn patch_user(&self, id: &str, patch: UserPatch) -> Result<Option<UserRec>> {
        self.update_user(id, Arc::new(move |_| patch.clone())).await
    }

    async fn session(&self, id: &str) -> Result<Option<SessionRec>>;
    async fn put_session(&self, s: &SessionRec) -> Result<()>;
    async fn delete_session(&self, id: &str) -> Result<()>;
    async fn delete_user_sessions(&self, user_id: &str) -> Result<()>;

    async fn api_key(&self, id: &str) -> Result<Option<ApiKeyRec>>;
    /// All keys, or only `owner`'s.
    async fn api_keys(&self, owner: Option<&str>) -> Result<Vec<ApiKeyRec>>;
    /// Creates (or wholly replaces) a key.
    async fn put_api_key(&self, k: &ApiKeyRec) -> Result<()>;
    /// Writes only `k.last_used_at`, so it can't race with a revoke.
    async fn record_api_key_use(&self, k: &ApiKeyRec) -> Result<()>;
    /// Writes only `k.revoked_at`.
    async fn revoke_api_key(&self, k: &ApiKeyRec) -> Result<()>;

    async fn project(&self, id: &str) -> Result<Option<ProjectRec>>;
    async fn projects(&self) -> Result<Vec<ProjectRec>>;
    async fn put_project(&self, p: &ProjectRec) -> Result<()>;
    /// Deletes the project and its issued estimates.
    async fn delete_project(&self, id: &str) -> Result<()>;

    async fn issued(&self, id: &str) -> Result<Option<IssuedRec>>;
    async fn issued_for(&self, project_id: &str) -> Result<Vec<IssuedRec>>;
    async fn put_issued(&self, i: &IssuedRec) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_patch_fields_match_stored_names() {
        let all = UserPatch {
            name: Some(String::new()),
            role: Some(Role::Admin),
            pw_hash: Some(String::new()),
            disabled: Some(true),
            last_login_at: Some(Utc::now()),
            failed_logins: Some(1),
            locked_until: Some(None),
        };
        let user = UserRec {
            id: String::new(),
            email: String::new(),
            name: String::new(),
            role: Role::Estimator,
            pw_hash: String::new(),
            disabled: false,
            created_at: Utc::now(),
            last_login_at: None,
            failed_logins: 0,
            locked_until: None,
        };
        let stored = serde_json::to_value(&user).unwrap();
        for f in all.fields() {
            assert!(stored.get(f).is_some(), "{f} is not a UserRec field");
        }
        assert_eq!(all.fields().len(), 7);
        assert!(UserPatch::default().fields().is_empty());
    }
}
