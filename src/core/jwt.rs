use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use serde::{Deserialize, Serialize};
use std::env;
use chrono::{Utc, Duration};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // user_id
    pub exp: usize,       // expiration time
    pub iat: usize,       // issued at
}

/// Generate a JWT token for a user
pub fn generate_token(user_id: &str) -> Result<String, String> {
    let secret = env::var("JWT_SECRET").unwrap_or_else(|_| "default-secret-change-in-production".to_string());
    
    let now = Utc::now();
    let expires_at = now + Duration::hours(24); // Token valid for 24 hours
    
    let claims = Claims {
        sub: user_id.to_string(),
        exp: expires_at.timestamp() as usize,
        iat: now.timestamp() as usize,
    };
    
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| format!("Failed to generate token: {}", e))
}

/// Validate a JWT token and return the user_id
pub fn validate_token(token: &str) -> Result<String, String> {
    let secret = env::var("JWT_SECRET").unwrap_or_else(|_| "default-secret-change-in-production".to_string());
    
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|e| format!("Invalid token: {}", e))?;
    
    Ok(token_data.claims.sub)
}

/// Extract Bearer token from Authorization header
pub fn extract_bearer_token(auth_header: &str) -> Option<&str> {
    if auth_header.starts_with("Bearer ") {
        Some(&auth_header[7..])
    } else {
        None
    }
}
