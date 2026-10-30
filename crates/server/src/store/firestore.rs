//! Firestore (Native mode) store. Uses Application Default Credentials on Cloud Run and
//! honours `FIRESTORE_EMULATOR_HOST` for local development.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use firestore::errors::{BackoffError, FirestoreError};
use firestore::{
    jiff::Timestamp, path, FirestoreDb, FirestoreDbOptions, FirestoreWritePrecondition,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::{ApiKeyRec, IssuedRec, ProjectRec, Result, SessionRec, Store, UserRec, UserUpdateFn};

const USERS: &str = "users";
const SESSIONS: &str = "sessions";
const API_KEYS: &str = "api_keys";
const PROJECTS: &str = "projects";
const ISSUED: &str = "issued_estimates";

/// Sessions are stored with a native Firestore timestamp so a TTL policy on `expires_at` can
/// delete them after they expire.
#[derive(Serialize, Deserialize)]
struct SessionDoc {
    id: String,
    user_id: String,
    created_at: DateTime<Utc>,
    #[serde(with = "firestore::serialize_as_timestamp")]
    expires_at: Timestamp,
}

impl From<&SessionRec> for SessionDoc {
    fn from(s: &SessionRec) -> SessionDoc {
        SessionDoc {
            id: s.id.clone(),
            user_id: s.user_id.clone(),
            created_at: s.created_at,
            expires_at: Timestamp::from_microsecond(s.expires_at.timestamp_micros())
                .unwrap_or(Timestamp::MAX),
        }
    }
}

impl From<SessionDoc> for SessionRec {
    fn from(d: SessionDoc) -> SessionRec {
        SessionRec {
            id: d.id,
            user_id: d.user_id,
            created_at: d.created_at,
            expires_at: DateTime::from_timestamp_micros(d.expires_at.as_microsecond())
                .unwrap_or(DateTime::<Utc>::MAX_UTC),
        }
    }
}

pub struct FirestoreStore {
    db: FirestoreDb,
}

impl FirestoreStore {
    pub async fn connect(project_id: &str, database: &str) -> Result<FirestoreStore> {
        let opts =
            FirestoreDbOptions::new(project_id.to_string()).with_database_id(database.to_string());
        let db = FirestoreDb::with_options(opts).await?;
        Ok(FirestoreStore { db })
    }

    async fn get<T: DeserializeOwned + Send>(&self, col: &str, id: &str) -> Result<Option<T>> {
        Ok(self
            .db
            .fluent()
            .select()
            .by_id_in(col)
            .obj::<T>()
            .one(id)
            .await?)
    }

    async fn put<T: Serialize + DeserializeOwned + Send + Sync>(
        &self,
        col: &str,
        id: &str,
        v: &T,
    ) -> Result<()> {
        // `update` without a precondition is an upsert.
        let _: T = self
            .db
            .fluent()
            .update()
            .in_col(col)
            .document_id(id)
            .object(v)
            .execute()
            .await?;
        Ok(())
    }

    /// Writes only `fields` of `v`, and only if the document exists.
    async fn put_fields<T: Serialize + DeserializeOwned + Send + Sync>(
        &self,
        col: &str,
        id: &str,
        fields: &[String],
        v: &T,
    ) -> Result<()> {
        let _: T = self
            .db
            .fluent()
            .update()
            .fields(fields)
            .in_col(col)
            .precondition(FirestoreWritePrecondition::Exists(true))
            .document_id(id)
            .object(v)
            .execute()
            .await?;
        Ok(())
    }

    async fn delete(&self, col: &str, id: &str) -> Result<()> {
        self.db
            .fluent()
            .delete()
            .from(col)
            .document_id(id)
            .execute()
            .await?;
        Ok(())
    }

    async fn all<T: DeserializeOwned + Send>(&self, col: &str) -> Result<Vec<T>> {
        Ok(self
            .db
            .fluent()
            .select()
            .from(col)
            .obj::<T>()
            .query()
            .await?)
    }

    async fn where_eq<T: DeserializeOwned + Send>(
        &self,
        col: &str,
        field: &str,
        value: &str,
    ) -> Result<Vec<T>> {
        let value = value.to_string();
        let field = field.to_string();
        Ok(self
            .db
            .fluent()
            .select()
            .from(col)
            .filter(move |q| q.for_all([q.field(field.clone()).eq(value.clone())]))
            .obj::<T>()
            .query()
            .await?)
    }
}

#[async_trait]
impl Store for FirestoreStore {
    async fn user(&self, id: &str) -> Result<Option<UserRec>> {
        self.get(USERS, id).await
    }

    async fn user_by_email(&self, email: &str) -> Result<Option<UserRec>> {
        Ok(self
            .where_eq::<UserRec>(USERS, &path!(UserRec::email), email)
            .await?
            .into_iter()
            .next())
    }

    async fn users(&self) -> Result<Vec<UserRec>> {
        self.all(USERS).await
    }

    async fn put_user(&self, u: &UserRec) -> Result<()> {
        self.put(USERS, &u.id, u).await
    }

    async fn update_user(&self, id: &str, f: UserUpdateFn) -> Result<Option<UserRec>> {
        let id = id.to_string();
        Ok(self
            .db
            .run_transaction(move |db, tx| {
                let (id, f) = (id.clone(), f.clone());
                Box::pin(async move {
                    let Some(mut user) = db
                        .fluent()
                        .select()
                        .by_id_in(USERS)
                        .obj::<UserRec>()
                        .one(&id)
                        .await?
                    else {
                        return Ok(None);
                    };
                    let patch = f(&user);
                    let fields = patch.fields();
                    if !fields.is_empty() {
                        patch.apply(&mut user);
                        db.fluent()
                            .update()
                            .fields(fields)
                            .in_col(USERS)
                            .document_id(&id)
                            .object(&user)
                            .add_to_transaction(tx)?;
                    }
                    Ok::<_, BackoffError<FirestoreError>>(Some(user))
                })
            })
            .await?)
    }

    async fn session(&self, id: &str) -> Result<Option<SessionRec>> {
        Ok(self.get::<SessionDoc>(SESSIONS, id).await?.map(Into::into))
    }

    async fn put_session(&self, s: &SessionRec) -> Result<()> {
        self.put(SESSIONS, &s.id, &SessionDoc::from(s)).await
    }

    async fn delete_session(&self, id: &str) -> Result<()> {
        self.delete(SESSIONS, id).await
    }

    async fn delete_user_sessions(&self, user_id: &str) -> Result<()> {
        let sessions: Vec<SessionDoc> = self
            .where_eq(SESSIONS, &path!(SessionRec::user_id), user_id)
            .await?;
        for s in sessions {
            self.delete(SESSIONS, &s.id).await?;
        }
        Ok(())
    }

    async fn api_key(&self, id: &str) -> Result<Option<ApiKeyRec>> {
        self.get(API_KEYS, id).await
    }

    async fn api_keys(&self, owner: Option<&str>) -> Result<Vec<ApiKeyRec>> {
        match owner {
            Some(o) => {
                self.where_eq(API_KEYS, &path!(ApiKeyRec::owner_id), o)
                    .await
            }
            None => self.all(API_KEYS).await,
        }
    }

    async fn put_api_key(&self, k: &ApiKeyRec) -> Result<()> {
        self.put(API_KEYS, &k.id, k).await
    }

    async fn record_api_key_use(&self, k: &ApiKeyRec) -> Result<()> {
        self.put_fields(API_KEYS, &k.id, &[path!(ApiKeyRec::last_used_at)], k)
            .await
    }

    async fn revoke_api_key(&self, k: &ApiKeyRec) -> Result<()> {
        self.put_fields(API_KEYS, &k.id, &[path!(ApiKeyRec::revoked_at)], k)
            .await
    }

    async fn project(&self, id: &str) -> Result<Option<ProjectRec>> {
        self.get(PROJECTS, id).await
    }

    async fn projects(&self) -> Result<Vec<ProjectRec>> {
        self.all(PROJECTS).await
    }

    async fn put_project(&self, p: &ProjectRec) -> Result<()> {
        self.put(PROJECTS, &p.id, p).await
    }

    async fn delete_project(&self, id: &str) -> Result<()> {
        for i in self.issued_for(id).await? {
            self.delete(ISSUED, &i.id).await?;
        }
        self.delete(PROJECTS, id).await
    }

    async fn issued(&self, id: &str) -> Result<Option<IssuedRec>> {
        self.get(ISSUED, id).await
    }

    async fn issued_for(&self, project_id: &str) -> Result<Vec<IssuedRec>> {
        self.where_eq(ISSUED, &path!(IssuedRec::project_id), project_id)
            .await
    }

    async fn put_issued(&self, i: &IssuedRec) -> Result<()> {
        self.put(ISSUED, &i.id, i).await
    }
}
