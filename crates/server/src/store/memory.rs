//! In-memory store. Data is lost on restart.

use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::{ApiKeyRec, IssuedRec, ProjectRec, Result, SessionRec, Store, UserRec, UserUpdateFn};

#[derive(Default)]
pub struct MemoryStore {
    users: RwLock<HashMap<String, UserRec>>,
    sessions: RwLock<HashMap<String, SessionRec>>,
    keys: RwLock<HashMap<String, ApiKeyRec>>,
    projects: RwLock<HashMap<String, ProjectRec>>,
    issued: RwLock<HashMap<String, IssuedRec>>,
}

#[async_trait]
impl Store for MemoryStore {
    async fn user(&self, id: &str) -> Result<Option<UserRec>> {
        Ok(self.users.read().await.get(id).cloned())
    }

    async fn user_by_email(&self, email: &str) -> Result<Option<UserRec>> {
        Ok(self
            .users
            .read()
            .await
            .values()
            .find(|u| u.email == email)
            .cloned())
    }

    async fn users(&self) -> Result<Vec<UserRec>> {
        Ok(self.users.read().await.values().cloned().collect())
    }

    async fn put_user(&self, u: &UserRec) -> Result<()> {
        self.users.write().await.insert(u.id.clone(), u.clone());
        Ok(())
    }

    async fn update_user(&self, id: &str, f: UserUpdateFn) -> Result<Option<UserRec>> {
        let mut users = self.users.write().await;
        Ok(users.get_mut(id).map(|u| {
            f(u).apply(u);
            u.clone()
        }))
    }

    async fn session(&self, id: &str) -> Result<Option<SessionRec>> {
        Ok(self.sessions.read().await.get(id).cloned())
    }

    async fn put_session(&self, s: &SessionRec) -> Result<()> {
        self.sessions.write().await.insert(s.id.clone(), s.clone());
        Ok(())
    }

    async fn delete_session(&self, id: &str) -> Result<()> {
        self.sessions.write().await.remove(id);
        Ok(())
    }

    async fn delete_user_sessions(&self, user_id: &str) -> Result<()> {
        self.sessions
            .write()
            .await
            .retain(|_, s| s.user_id != user_id);
        Ok(())
    }

    async fn api_key(&self, id: &str) -> Result<Option<ApiKeyRec>> {
        Ok(self.keys.read().await.get(id).cloned())
    }

    async fn api_keys(&self, owner: Option<&str>) -> Result<Vec<ApiKeyRec>> {
        Ok(self
            .keys
            .read()
            .await
            .values()
            .filter(|k| owner.is_none_or(|o| k.owner_id == o))
            .cloned()
            .collect())
    }

    async fn put_api_key(&self, k: &ApiKeyRec) -> Result<()> {
        self.keys.write().await.insert(k.id.clone(), k.clone());
        Ok(())
    }

    async fn record_api_key_use(&self, k: &ApiKeyRec) -> Result<()> {
        if let Some(stored) = self.keys.write().await.get_mut(&k.id) {
            stored.last_used_at = k.last_used_at;
        }
        Ok(())
    }

    async fn revoke_api_key(&self, k: &ApiKeyRec) -> Result<()> {
        if let Some(stored) = self.keys.write().await.get_mut(&k.id) {
            stored.revoked_at = k.revoked_at;
        }
        Ok(())
    }

    async fn project(&self, id: &str) -> Result<Option<ProjectRec>> {
        Ok(self.projects.read().await.get(id).cloned())
    }

    async fn projects(&self) -> Result<Vec<ProjectRec>> {
        Ok(self.projects.read().await.values().cloned().collect())
    }

    async fn put_project(&self, p: &ProjectRec) -> Result<()> {
        self.projects.write().await.insert(p.id.clone(), p.clone());
        Ok(())
    }

    async fn delete_project(&self, id: &str) -> Result<()> {
        self.projects.write().await.remove(id);
        self.issued.write().await.retain(|_, i| i.project_id != id);
        Ok(())
    }

    async fn issued(&self, id: &str) -> Result<Option<IssuedRec>> {
        Ok(self.issued.read().await.get(id).cloned())
    }

    async fn issued_for(&self, project_id: &str) -> Result<Vec<IssuedRec>> {
        Ok(self
            .issued
            .read()
            .await
            .values()
            .filter(|i| i.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn put_issued(&self, i: &IssuedRec) -> Result<()> {
        self.issued.write().await.insert(i.id.clone(), i.clone());
        Ok(())
    }
}
