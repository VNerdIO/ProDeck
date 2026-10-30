//! ProDeck REST API (`/api/v1`) request and response types.
//!
//! These are the API contract: the server derives its OpenAPI spec from them and the web
//! client deserializes into them. Changing a field here is an API change.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use estimator::{Estimate, EstimateError, Project};

/// Error body returned with every non-2xx response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiError {
    /// Stable machine-readable code, e.g. `unauthorized`, `validation`, `not_found`.
    pub code: String,
    pub message: String,
    /// Present when the estimator rejected the deck design.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate_error: Option<EstimateError>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Manages users and every API key.
    Admin,
    /// Creates and edits projects and their own API keys.
    Estimator,
}

// ---- Auth ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: Role,
    pub disabled: bool,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

// ---- Users (admin) ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateUserRequest {
    pub email: String,
    pub name: String,
    pub role: Role,
    /// Initial password; share it with the user out of band.
    pub password: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateUserRequest {
    pub name: Option<String>,
    pub role: Option<Role>,
    /// Disabling a user ends their sessions and stops their API keys working.
    pub disabled: Option<bool>,
    /// Admin password reset.
    pub password: Option<String>,
}

// ---- API keys ----

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyStatus {
    Active,
    Expired,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    /// Non-secret prefix to recognise the key, e.g. `pdk_3fK9xQ2m`.
    pub prefix: String,
    pub owner_id: String,
    pub owner_email: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub status: ApiKeyStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateApiKeyRequest {
    pub name: String,
    /// Lifetime in days (1-365). Use this or `expires_at`.
    pub expires_in_days: Option<u32>,
    /// Exact expiry, at most 365 days from now.
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreatedApiKey {
    pub key: ApiKey,
    /// The full bearer token. Shown once; it cannot be retrieved again.
    pub token: String,
}

/// Longest allowed API key lifetime.
pub const API_KEY_MAX_DAYS: i64 = 365;
/// Keys expiring within this many days are flagged in the UI.
pub const API_KEY_WARN_DAYS: i64 = 14;

// ---- Projects ----

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Draft,
    /// At least one estimate has been issued (snapshotted) for this project.
    Issued,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub customer: String,
    pub address: String,
    pub owner_id: String,
    pub owner_name: String,
    pub updated_at: DateTime<Utc>,
    pub status: ProjectStatus,
    pub deck_area_sqft: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectDetail {
    pub id: String,
    pub name: String,
    pub customer: String,
    pub address: String,
    pub notes: String,
    pub owner_id: String,
    pub owner_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: ProjectStatus,
    pub input: Project,
    pub issued: Vec<IssuedEstimateMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssuedEstimateMeta {
    pub id: String,
    pub issued_at: DateTime<Utc>,
    pub issued_by: String,
    pub label: String,
}

/// An issued estimate: the exact input and output at the time it was issued.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssuedEstimate {
    pub meta: IssuedEstimateMeta,
    pub input: Project,
    pub estimate: Estimate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SaveProjectRequest {
    pub name: String,
    #[serde(default)]
    pub customer: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub notes: String,
    pub input: Project,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueRequest {
    /// e.g. "Customer quote", "Final pickup list".
    pub label: String,
}

impl ApiKey {
    pub fn compute_status(
        expires_at: DateTime<Utc>,
        revoked_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> ApiKeyStatus {
        if revoked_at.is_some() {
            ApiKeyStatus::Revoked
        } else if expires_at <= now {
            ApiKeyStatus::Expired
        } else {
            ApiKeyStatus::Active
        }
    }
}
