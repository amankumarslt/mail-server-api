use std::env;
use serde::{Deserialize, Serialize};

/// WorkOS User Management configuration
pub struct WorkOSConfig {
    pub api_key: String,
    pub client_id: String,
    pub redirect_uri: String,
}

impl WorkOSConfig {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            api_key: env::var("WORKOS_API_KEY")
                .map_err(|_| "WORKOS_API_KEY not set")?,
            client_id: env::var("WORKOS_CLIENT_ID")
                .map_err(|_| "WORKOS_CLIENT_ID not set")?,
            redirect_uri: format!(
                "{}/auth/workos/callback",
                env::var("SERVER_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
            ),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkOSUser {
    pub id: String,
    pub email: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email_verified: bool,
}

#[derive(Debug, Deserialize)]
struct AuthCodeResponse {
    user: WorkOSUserResponse,
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct WorkOSUserResponse {
    id: String,
    email: String,
    first_name: Option<String>,
    last_name: Option<String>,
    email_verified: bool,
}

/// Get authorization URL for AuthKit hosted UI - shows all enabled auth providers
/// (Email, Google, Microsoft, GitHub, etc.)
pub fn get_auth_url(config: &WorkOSConfig, state: &str) -> String {
    // Using provider=authkit gives the full hosted login page with all auth methods
    format!(
        "https://api.workos.com/user_management/authorize?response_type=code&client_id={}&redirect_uri={}&provider=authkit&state={}",
        config.client_id,
        url::form_urlencoded::byte_serialize(config.redirect_uri.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(state.as_bytes()).collect::<String>()
    )
}

/// Exchange authorization code for user and tokens
pub async fn authenticate_with_code(
    config: &WorkOSConfig,
    code: &str,
) -> Result<(WorkOSUser, String, String), String> {
    let client = reqwest::Client::new();
    
    // WorkOS expects client_secret in the request body, not bearer auth header
    let response = client
        .post("https://api.workos.com/user_management/authenticate")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "client_id": config.client_id,
            "client_secret": config.api_key,
            "code": code,
            "grant_type": "authorization_code"
        }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("WorkOS API error: {}", error_text));
    }

    let auth_response: AuthCodeResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let user = WorkOSUser {
        id: auth_response.user.id,
        email: auth_response.user.email,
        first_name: auth_response.user.first_name,
        last_name: auth_response.user.last_name,
        email_verified: auth_response.user.email_verified,
    };

    Ok((user, auth_response.access_token, auth_response.refresh_token))
}
