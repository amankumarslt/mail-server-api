use oauth2::{
    AuthorizationCode, AuthUrl, ClientId, ClientSecret, CsrfToken,
    RedirectUrl, Scope, TokenResponse, TokenUrl,
    basic::BasicClient, reqwest::async_http_client,
};
use std::env;

pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
}

/// Create Google OAuth client
pub fn google_client() -> Result<BasicClient, String> {
    let client_id = env::var("GOOGLE_CLIENT_ID")
        .map_err(|_| "GOOGLE_CLIENT_ID not set")?;
    let client_secret = env::var("GOOGLE_CLIENT_SECRET")
        .map_err(|_| "GOOGLE_CLIENT_SECRET not set")?;
    let server_url = env::var("SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());

    let client = BasicClient::new(
        ClientId::new(client_id),
        Some(ClientSecret::new(client_secret)),
        AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())
            .map_err(|e| format!("Invalid auth URL: {}", e))?,
        Some(TokenUrl::new("https://oauth2.googleapis.com/token".to_string())
            .map_err(|e| format!("Invalid token URL: {}", e))?),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!("{}/auth/callback", server_url))
            .map_err(|e| format!("Invalid redirect URL: {}", e))?,
    );

    Ok(client)
}

/// Create Microsoft OAuth client
pub fn microsoft_client() -> Result<BasicClient, String> {
    let client_id = env::var("MICROSOFT_CLIENT_ID")
        .map_err(|_| "MICROSOFT_CLIENT_ID not set")?;
    let client_secret = env::var("MICROSOFT_CLIENT_SECRET")
        .map_err(|_| "MICROSOFT_CLIENT_SECRET not set")?;
    let server_url = env::var("SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());

    let client = BasicClient::new(
        ClientId::new(client_id),
        Some(ClientSecret::new(client_secret)),
        AuthUrl::new("https://login.microsoftonline.com/common/oauth2/v2.0/authorize".to_string())
            .map_err(|e| format!("Invalid auth URL: {}", e))?,
        Some(TokenUrl::new("https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string())
            .map_err(|e| format!("Invalid token URL: {}", e))?),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!("{}/auth/callback", server_url))
            .map_err(|e| format!("Invalid redirect URL: {}", e))?,
    );

    Ok(client)
}

/// Generate Google authorization URL (used for WorkOS login flow)
pub fn google_auth_url(user_id: &str) -> Result<String, String> {
    let client = google_client()?;
    
    let (auth_url, _csrf_token) = client
        .authorize_url(|| CsrfToken::new(user_id.to_string()))
        .add_scope(Scope::new("https://mail.google.com/".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_extra_param("access_type", "offline")
        .add_extra_param("prompt", "consent")
        .url();
    
    Ok(auth_url.to_string())
}

/// Generate Microsoft authorization URL
pub fn microsoft_auth_url(user_id: &str) -> Result<String, String> {
    let client = microsoft_client()?;
    
    let (auth_url, _csrf_token) = client
        .authorize_url(|| CsrfToken::new(user_id.to_string()))
        .add_scope(Scope::new("https://outlook.office.com/IMAP.AccessAsUser.All".to_string()))
        .add_scope(Scope::new("offline_access".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .url();

    Ok(auth_url.to_string())
}

/// Exchange authorization code for tokens (Google)
pub async fn google_exchange_code(code: &str) -> Result<OAuthTokens, String> {
    let client = google_client()?;
    
    let token_result = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .request_async(async_http_client)
        .await
        .map_err(|e| format!("Token exchange failed: {}", e))?;

    Ok(OAuthTokens {
        access_token: token_result.access_token().secret().clone(),
        refresh_token: token_result.refresh_token().map(|t| t.secret().clone()),
        expires_in: token_result.expires_in().map(|d| d.as_secs()),
    })
}

/// Exchange authorization code for tokens (Microsoft)
pub async fn microsoft_exchange_code(code: &str) -> Result<OAuthTokens, String> {
    let client = microsoft_client()?;
    
    let token_result = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .request_async(async_http_client)
        .await
        .map_err(|e| format!("Token exchange failed: {}", e))?;

    Ok(OAuthTokens {
        access_token: token_result.access_token().secret().clone(),
        refresh_token: token_result.refresh_token().map(|t| t.secret().clone()),
        expires_in: token_result.expires_in().map(|d| d.as_secs()),
    })
}

/// Generate XOAUTH2 string for IMAP authentication
pub fn xoauth2_string(email: &str, access_token: &str) -> String {
    let auth_string = format!("user={}\x01auth=Bearer {}\x01\x01", email, access_token);
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, auth_string)
}
