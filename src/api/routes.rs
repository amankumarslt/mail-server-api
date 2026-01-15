use actix_web::{web, HttpRequest, HttpResponse};
use sqlx::{PgPool, Row};
use serde::{Deserialize, Serialize};
use crate::core::imap_client::{fetch_latest_email, ImapCredentials};
use crate::core::oauth;
use crate::core::jwt;
use crate::core::workos_auth;

#[derive(Serialize)]
pub struct EmailResponse {
    sender: String,
    subject: String,
    preview: String,
    received_at: String,
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    id: String,
    email: String,
    imap_server: Option<String>,
    imap_port: Option<i32>,
    imap_password: Option<String>,
}

#[derive(Serialize)]
pub struct UserResponse {
    id: String,
    email: String,
    created: bool,
}

#[derive(Serialize)]
pub struct SyncResponse {
    synced: bool,
    email: Option<EmailResponse>,
    message: String,
}

#[derive(Deserialize)]
pub struct AuthQuery {
    user_id: String,
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: String,
    state: String,  // Contains user_id and provider
}

/// Create a new user with IMAP credentials
pub async fn create_user(
    pool: web::Data<PgPool>,
    body: web::Json<CreateUserRequest>,
) -> HttpResponse {
    let result = sqlx::query(
        r#"
        INSERT INTO users (id, email, imap_server, imap_port, imap_password)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (id) DO UPDATE SET
            email = EXCLUDED.email,
            imap_server = EXCLUDED.imap_server,
            imap_port = EXCLUDED.imap_port,
            imap_password = EXCLUDED.imap_password
        "#
    )
    .bind(&body.id)
    .bind(&body.email)
    .bind(&body.imap_server)
    .bind(body.imap_port.unwrap_or(993))
    .bind(&body.imap_password)
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Ok().json(UserResponse {
            id: body.id.clone(),
            email: body.email.clone(),
            created: true,
        }),
        Err(e) => HttpResponse::InternalServerError().json(format!("Error: {}", e)),
    }
}

/// Start Google OAuth flow
pub async fn auth_google(query: web::Query<AuthQuery>) -> HttpResponse {
    match oauth::google_auth_url(&format!("{}:google", query.user_id)) {
        Ok(url) => HttpResponse::Found()
            .append_header(("Location", url))
            .finish(),
        Err(e) => HttpResponse::InternalServerError().json(format!("Error: {}", e)),
    }
}

/// Start Microsoft OAuth flow
pub async fn auth_microsoft(query: web::Query<AuthQuery>) -> HttpResponse {
    match oauth::microsoft_auth_url(&format!("{}:microsoft", query.user_id)) {
        Ok(url) => HttpResponse::Found()
            .append_header(("Location", url))
            .finish(),
        Err(e) => HttpResponse::InternalServerError().json(format!("Error: {}", e)),
    }
}

/// Handle OAuth callback from Google/Microsoft
pub async fn auth_callback(
    pool: web::Data<PgPool>,
    query: web::Query<CallbackQuery>,
) -> HttpResponse {
    // Parse state to get user_id, provider, and optional redirect
    // Format: user_id:provider[:redirect_url]
    let parts: Vec<&str> = query.state.splitn(3, ':').collect();
    if parts.len() < 2 {
        return HttpResponse::BadRequest().json("Invalid state");
    }
    let user_id = parts[0];
    let provider = parts[1];
    let redirect_url = if parts.len() == 3 { Some(parts[2]) } else { None };

    // Exchange code for tokens based on provider
    let tokens = match provider {
        "google" | "gmail_connect" => oauth::google_exchange_code(&query.code).await,
        "microsoft" => oauth::microsoft_exchange_code(&query.code).await,
        _ => return HttpResponse::BadRequest().json("Unknown provider"),
    };

    let tokens = match tokens {
        Ok(t) => t,
        Err(e) => return HttpResponse::InternalServerError().json(format!("Token exchange failed: {}", e)),
    };

    // Calculate expiration time
    let expires_at = tokens.expires_in.map(|secs| {
        chrono::Utc::now() + chrono::Duration::seconds(secs as i64)
    });

    // Fetch real email from provider's API
    let email = match provider {
        "google" | "gmail_connect" => {
            // Get email from Google userinfo API
            let client = reqwest::Client::new();
            let resp = client
                .get("https://www.googleapis.com/oauth2/v2/userinfo")
                .bearer_auth(&tokens.access_token)
                .send()
                .await;
            
            match resp {
                Ok(r) => {
                    if let Ok(info) = r.json::<serde_json::Value>().await {
                        info["email"].as_str().unwrap_or("unknown@gmail.com").to_string()
                    } else {
                        format!("{}@gmail.com", user_id)
                    }
                }
                Err(_) => format!("{}@gmail.com", user_id),
            }
        }
        "microsoft" => {
            // Get email from Microsoft Graph API
            let client = reqwest::Client::new();
            let resp = client
                .get("https://graph.microsoft.com/v1.0/me")
                .bearer_auth(&tokens.access_token)
                .send()
                .await;
            
            match resp {
                Ok(r) => {
                    if let Ok(info) = r.json::<serde_json::Value>().await {
                        info["mail"].as_str()
                            .or(info["userPrincipalName"].as_str())
                            .unwrap_or("unknown@outlook.com")
                            .to_string()
                    } else {
                        format!("{}@outlook.com", user_id)
                    }
                }
                Err(_) => format!("{}@outlook.com", user_id),
            }
        }
        _ => format!("{}@email.com", user_id),
    };

    let imap_server = match provider {
        "google" | "gmail_connect" => "imap.gmail.com",
        "microsoft" => "outlook.office365.com",
        _ => "imap.email.com",
    };
    
    // Normalized provider name for DB
    let db_provider = if provider == "gmail_connect" { "google" } else { provider };

    // Save tokens to database
    let result = sqlx::query(
        r#"
        INSERT INTO users (id, email, auth_provider, access_token, refresh_token, token_expires_at, imap_server, imap_port)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 993)
        ON CONFLICT (id) DO UPDATE SET
            auth_provider = EXCLUDED.auth_provider,
            access_token = EXCLUDED.access_token,
            refresh_token = EXCLUDED.refresh_token,
            token_expires_at = EXCLUDED.token_expires_at,
            imap_server = EXCLUDED.imap_server
        "#
    )
    .bind(user_id)
    .bind(&email)
    .bind(db_provider)
    .bind(&tokens.access_token)
    .bind(&tokens.refresh_token)
    .bind(expires_at)
    .bind(imap_server)
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => {
            if let Some(url) = redirect_url {
                 HttpResponse::Found().append_header(("Location", url)).finish()
            } else {
                // Generate JWT token for the user
                let token = jwt::generate_token(user_id).unwrap_or_default();
                HttpResponse::Ok().json(serde_json::json!({
                    "success": true,
                    "user_id": user_id,
                    "provider": provider,
                    "token": token,
                    "message": "OAuth authentication successful!"
                }))
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(format!("Failed to save tokens: {}", e)),
    }
}

/// Sync latest email from user's IMAP server (requires Bearer token)
pub async fn sync_emails(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path_user_id: web::Path<String>,
) -> HttpResponse {
    // Validate JWT token
    let auth_header = req.headers().get("Authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    
    let token = match jwt::extract_bearer_token(auth_header) {
        Some(t) => t,
        None => return HttpResponse::Unauthorized().json("Missing Authorization: Bearer <token>"),
    };
    
    let token_user_id = match jwt::validate_token(token) {
        Ok(uid) => uid,
        Err(e) => return HttpResponse::Unauthorized().json(format!("Invalid token: {}", e)),
    };
    
    let user_id = path_user_id.into_inner();
    
    // Ensure user can only access their own data
    if token_user_id != user_id {
        return HttpResponse::Forbidden().json("Token does not match user_id");
    }
    
    // Get user's credentials (OAuth or IMAP)
    let user_result = sqlx::query(
        r#"
        SELECT email, imap_server, imap_port, imap_password, 
               auth_provider, access_token, refresh_token, token_expires_at
        FROM users WHERE id = $1
        "#
    )
    .bind(&user_id)
    .fetch_optional(pool.get_ref())
    .await;

    let user = match user_result {
        Ok(Some(row)) => row,
        Ok(None) => return HttpResponse::NotFound().json("User not found"),
        Err(e) => return HttpResponse::InternalServerError().json(format!("DB Error: {}", e)),
    };

    let email: String = user.get("email");
    let imap_server: Option<String> = user.get("imap_server");
    let port: i32 = user.get("imap_port");
    let imap_password: Option<String> = user.get("imap_password");
    let auth_provider: Option<String> = user.get("auth_provider");
    let access_token: Option<String> = user.get("access_token");

    // Use Gmail API for Google OAuth users, IMAP for others (also allow WorkOS users who connected Gmail)
    if (auth_provider.as_deref() == Some("google") || 
        auth_provider.as_deref() == Some("gmail_connect") || 
        auth_provider.as_deref() == Some("workos")) && access_token.is_some() {
        // Use Gmail API (more reliable than IMAP XOAUTH2)
        let token = access_token.unwrap();
        match crate::core::gmail_api::fetch_gmail_emails(&token, 2).await {
            Ok(emails) if !emails.is_empty() => {
                // Save all emails to database
                let mut saved_count = 0;
                for fetched in &emails {
                        let insert_result = sqlx::query(
                            r#"
                            INSERT INTO emails (user_id, message_id, sender, subject, body_preview, received_at)
                            VALUES ($1, $2, $3, $4, $5, TO_TIMESTAMP($6))
                            ON CONFLICT (user_id, message_id) DO UPDATE SET
                                received_at = EXCLUDED.received_at
                            "#
                        )
                        .bind(&user_id)
                        .bind(&fetched.message_id)
                        .bind(&fetched.sender)
                        .bind(&fetched.subject)
                        .bind(&fetched.body_preview)
                        .bind(fetched.received_at as f64)
                        .execute(pool.get_ref())
                        .await;
                    
                    if insert_result.is_ok() {
                        saved_count += 1;
                    }
                }
                
                let latest = emails.first().unwrap();
                HttpResponse::Ok().json(serde_json::json!({
                    "synced": true,
                    "count": saved_count,
                    "email": {
                        "sender": latest.sender,
                        "subject": latest.subject,
                        "preview": latest.body_preview,
                        "received_at": chrono::Utc::now().to_string()
                    },
                    "message": format!("Synced {} emails successfully", saved_count)
                }))
            }
            Ok(_) => HttpResponse::Ok().json(SyncResponse {
                synced: false,
                email: None,
                message: "No emails found in inbox".to_string(),
            }),
            Err(e) => HttpResponse::InternalServerError().json(SyncResponse {
                synced: false,
                email: None,
                message: format!("Gmail API error: {}", e),
            }),
        }
    } else if imap_password.is_some() && imap_server.is_some() {
        // Use IMAP for non-Google providers
        let creds = ImapCredentials {
            email: email.clone(),
            password: imap_password,
            access_token: None,
            server: imap_server.unwrap(),
            port: port as u16,
        };
        
        match fetch_latest_email(&creds).await {
            Ok(Some(fetched)) => {
                let insert_result = sqlx::query(
                    r#"
                    INSERT INTO emails (user_id, message_id, sender, subject, body_preview, received_at)
                    VALUES ($1, $2, $3, $4, $5, TO_TIMESTAMP($6))
                    ON CONFLICT (user_id, message_id) DO UPDATE SET
                        received_at = EXCLUDED.received_at
                    "#
                )
                .bind(&user_id)
                .bind(&fetched.message_id)
                .bind(&fetched.sender)
                .bind(&fetched.subject)
                .bind(&fetched.body_preview)
                .bind(fetched.received_at as f64)
                .execute(pool.get_ref())
                .await;

                match insert_result {
                    Ok(_) => HttpResponse::Ok().json(SyncResponse {
                        synced: true,
                        email: Some(EmailResponse {
                            sender: fetched.sender,
                            subject: fetched.subject,
                            preview: fetched.body_preview,
                            received_at: chrono::Utc::now().to_string(),
                        }),
                        message: "Email synced successfully".to_string(),
                    }),
                    Err(e) => HttpResponse::InternalServerError().json(format!("Failed to save: {}", e)),
                }
            }
            Ok(None) => HttpResponse::Ok().json(SyncResponse {
                synced: false,
                email: None,
                message: "No emails found in inbox".to_string(),
            }),
            Err(e) => HttpResponse::InternalServerError().json(SyncResponse {
                synced: false,
                email: None,
                message: format!("IMAP error: {}", e),
            }),
        }
    } else {
        HttpResponse::BadRequest().json("No credentials configured. Use OAuth or set IMAP password.")
    }
}

/// Get latest email from database (requires Bearer token)
pub async fn get_latest(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path_user_id: web::Path<String> 
) -> HttpResponse {
    // Validate JWT token
    let auth_header = req.headers().get("Authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    
    let token = match jwt::extract_bearer_token(auth_header) {
        Some(t) => t,
        None => return HttpResponse::Unauthorized().json("Missing Authorization: Bearer <token>"),
    };
    
    let token_user_id = match jwt::validate_token(token) {
        Ok(uid) => uid,
        Err(e) => return HttpResponse::Unauthorized().json(format!("Invalid token: {}", e)),
    };
    
    let user_id_str = path_user_id.into_inner();
    
    // Ensure user can only access their own data
    if token_user_id != user_id_str {
        return HttpResponse::Forbidden().json("Token does not match user_id");
    }
    let result = sqlx::query(
        r#"
        SELECT sender, subject, body_preview, received_at::text
        FROM emails
        WHERE user_id = $1
        ORDER BY received_at DESC
        LIMIT 1
        "#
    )
    .bind(&user_id_str)
    .fetch_optional(pool.get_ref())
    .await;

    match result {
        Ok(Some(row)) => HttpResponse::Ok().json(EmailResponse {
            sender: row.get("sender"),
            subject: row.get::<Option<String>, _>("subject").unwrap_or_default(),
            preview: row.get::<Option<String>, _>("body_preview").unwrap_or_default(),
            received_at: row.get::<Option<String>, _>("received_at").unwrap_or_default(),
        }),
        Ok(None) => HttpResponse::NotFound().json("Inbox Empty"),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[derive(Deserialize)]
pub struct SSOQuery {
    email: Option<String>,
    organization_id: Option<String>,
    connection_id: Option<String>,
    redirect_to: Option<String>,
}

#[derive(Deserialize)]
pub struct WorkOSCallbackQuery {
    code: String,
    state: Option<String>,
}

/// Start WorkOS AuthKit login (supports Google, email, etc.)
    pub async fn auth_workos_sso(query: web::Query<SSOQuery>) -> HttpResponse {
    let config = match workos_auth::WorkOSConfig::from_env() {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(format!("Config error: {}", e)),
    };
    
    let redirect_base = query.redirect_to.as_deref().unwrap_or("http://localhost:5176");
    let state = format!("authkit_login|{}", redirect_base);
    let url = workos_auth::get_auth_url(&config, &state);
    
    HttpResponse::Found()
        .append_header(("Location", url))
        .finish()
}

/// Handle WorkOS AuthKit callback
pub async fn auth_workos_callback(
    pool: web::Data<PgPool>,
    query: web::Query<WorkOSCallbackQuery>,
) -> HttpResponse {
    let config = match workos_auth::WorkOSConfig::from_env() {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(format!("Config error: {}", e)),
    };
    
    // Exchange code for user and tokens
    let (user, _access_token, _refresh_token) = match workos_auth::authenticate_with_code(&config, &query.code).await {
        Ok(result) => result,
        Err(e) => return HttpResponse::InternalServerError().json(format!("Auth failed: {}", e)),
    };
    
    // Create or update user in database
    let row = sqlx::query(
        r#"
        INSERT INTO users (id, email, auth_provider)
        VALUES ($1, $2, 'workos')
        ON CONFLICT (id) DO UPDATE SET
            email = EXCLUDED.email,
            auth_provider = EXCLUDED.auth_provider
        "#
    )
    .bind(&user.id)
    .bind(&user.email)
    .execute(pool.get_ref())
    .await;
    
    match row {
        Ok(_) => {
            // Generate JWT
            let token = jwt::generate_token(&user.id).unwrap_or_default();
            
            // Fetch Alias
            let alias_row = sqlx::query("SELECT alias FROM temp_aliases WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1")
                .bind(&user.id)
                .fetch_optional(pool.get_ref())
                .await.unwrap_or(None);
            let temp_alias: Option<String> = alias_row.map(|r| r.get("alias"));
            
            let user_json = serde_json::json!({
                "id": user.id,
                "email": user.email,
                "first_name": user.first_name,
                "last_name": user.last_name,
                "email_verified": user.email_verified,
                "temp_alias": temp_alias
            }).to_string();

            // Extract redirect base URL from state
            let state_parts: Vec<&str> = query.state.as_deref().unwrap_or("").split('|').collect();
            let base_url = if state_parts.len() > 1 { state_parts[1] } else { "http://localhost:5176" };
            let base_url = base_url.trim_end_matches('/');
            let encoded_user = url::form_urlencoded::byte_serialize(user_json.as_bytes()).collect::<String>();
            
            let redirect_url = format!("{}/?token={}&user={}", base_url, token, encoded_user);
            
            HttpResponse::Found()
                .append_header(("Location", redirect_url))
                .finish()
        }
        Err(e) => HttpResponse::InternalServerError().json(format!("DB error: {}", e)),
    }
}

#[derive(Deserialize)]
pub struct ConnectGmailQuery {
    user_id: String,
    redirect_to: Option<String>,
}

/// Start Gmail OAuth to connect email access (after WorkOS login)
pub async fn connect_gmail(query: web::Query<ConnectGmailQuery>) -> HttpResponse {
    let redirect_base = query.redirect_to.as_deref().unwrap_or("http://localhost:5176");
    // State format: user_id:gmail_connect:redirect_base
    let state = format!("{}:gmail_connect:{}", query.user_id, redirect_base);
    
    // Redirect to Google OAuth with gmail scope
    match oauth::google_auth_url(&state) {
        Ok(url) => HttpResponse::Found()
            .append_header(("Location", url))
            .finish(),
        Err(e) => HttpResponse::InternalServerError().json(format!("Error: {}", e)),
    }
}

/// Handle Gmail OAuth callback - stores access token for user
pub async fn connect_gmail_callback(
    pool: web::Data<PgPool>,
    query: web::Query<CallbackQuery>,
) -> HttpResponse {
    // Parse state to get user_id and redirect
    let parts: Vec<&str> = query.state.split(':').collect();
    if parts.len() < 2 {
        return HttpResponse::BadRequest().json("Invalid state");
    }
    let user_id = parts[0];
    let redirect_base = if parts.len() > 2 { parts[2] } else { "http://localhost:5176" };
    // Ensure no trailing slash
    let redirect_base = redirect_base.trim_end_matches('/');
    
    // Exchange code for tokens
    let tokens = match oauth::google_exchange_code(&query.code).await {
        Ok(t) => t,
        Err(e) => return HttpResponse::InternalServerError().json(format!("Token exchange failed: {}", e)),
    };
    
    // Store Gmail tokens for this user
    let result = sqlx::query(
        r#"
        UPDATE users 
        SET access_token = $1, 
            refresh_token = $2,
            auth_provider = COALESCE(auth_provider, 'google')
        WHERE id = $3
        "#
    )
    .bind(&tokens.access_token)
    .bind(&tokens.refresh_token)
    .bind(user_id)
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => {
            HttpResponse::Found()
                .append_header(("Location", redirect_base))
                .finish()
        }
        Err(e) => HttpResponse::InternalServerError().json(format!("DB error: {}", e)),
    }
}



/// Create a temporary email alias for the logged-in user
pub async fn create_temp_mail(
    pool: web::Data<PgPool>,
    req: HttpRequest,
) -> HttpResponse {
    // Validate Token
    let auth_header = req.headers().get("Authorization").and_then(|h| h.to_str().ok()).unwrap_or("");
    let token = match jwt::extract_bearer_token(auth_header) {
        Some(t) => t,
        None => return HttpResponse::Unauthorized().json("Missing token"),
    };
    let user_id = match jwt::validate_token(token) {
        Ok(id) => id,
        Err(_) => return HttpResponse::Unauthorized().json("Invalid token"),
    };

    let timestamp = chrono::Utc::now().timestamp_micros();
    let alias = format!("temp_{}", timestamp);
    // let email = format!("{}@localhost", alias); 
    
    // Clear old aliases (keep 1 for now)
    let _ = sqlx::query("DELETE FROM temp_aliases WHERE user_id = $1")
        .bind(&user_id)
        .execute(pool.get_ref())
        .await;

    // Insert new alias
    let result = sqlx::query("INSERT INTO temp_aliases (alias, user_id) VALUES ($1, $2)")
        .bind(&alias)
        .bind(&user_id)
        .execute(pool.get_ref())
        .await;
    
    match result {
        Ok(_) => {
            HttpResponse::Ok().json(serde_json::json!({
                "id": user_id, // Return real user ID
                "alias": alias,
                "email": format!("{}@localhost", alias)
            }))
        },
        Err(e) => HttpResponse::InternalServerError().json(format!("DB error: {}", e)),
    }
}

/// Delete a temporary email alias
pub async fn delete_temp_mail(
    pool: web::Data<PgPool>,
    req: HttpRequest,
    _path: web::Path<String>, // Keep path for compatibility or ignore? Better to use token user_id
) -> HttpResponse {
     // Validate Token
    let auth_header = req.headers().get("Authorization").and_then(|h| h.to_str().ok()).unwrap_or("");
    let token = match jwt::extract_bearer_token(auth_header) {
        Some(t) => t,
        None => return HttpResponse::Unauthorized().json("Missing token"),
    };
    let user_id = match jwt::validate_token(token) {
        Ok(id) => id,
        Err(_) => return HttpResponse::Unauthorized().json("Invalid token"),
    };

    let result = sqlx::query("DELETE FROM temp_aliases WHERE user_id = $1")
        .bind(&user_id)
        .execute(pool.get_ref())
        .await;
        
    match result {
        Ok(_) => HttpResponse::Ok().json("Deleted alias"),
        Err(e) => HttpResponse::InternalServerError().json(format!("DB error: {}", e)),
    }
}

pub struct SyncedEmail {
    pub sender: String,
    pub subject: String,
    pub preview: String,
    pub received_at: String,
}

impl serde::Serialize for SyncedEmail {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("SyncedEmail", 4)?;
        state.serialize_field("sender", &self.sender)?;
        state.serialize_field("subject", &self.subject)?;
        state.serialize_field("preview", &self.preview)?;
        state.serialize_field("received_at", &self.received_at)?;
        state.end()
    }
}

/// Get all emails for a user
pub async fn get_all_emails(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> HttpResponse {
    let user_id = path.into_inner();
    
    let result = sqlx::query(
        r#"
        SELECT sender, subject, body_preview, received_at::text
        FROM emails
        WHERE user_id = $1
        ORDER BY received_at DESC
        LIMIT 50
        "#
    )
    .bind(user_id)
    .fetch_all(pool.get_ref())
    .await;
    
    match result {
        Ok(rows) => {
            let emails: Vec<SyncedEmail> = rows.into_iter().map(|row| SyncedEmail {
                sender: row.get::<String, _>("sender"),
                subject: row.get::<Option<String>, _>("subject").unwrap_or_default(),
                preview: row.get::<Option<String>, _>("body_preview").unwrap_or_default(),
                received_at: row.get::<Option<String>, _>("received_at").unwrap_or_default(),
            }).collect();
            HttpResponse::Ok().json(emails)
        },
        Err(e) => HttpResponse::InternalServerError().json(format!("DB error: {}", e)),
    }
}

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/users")
            .route(web::post().to(create_user))
    )
    .service(
        web::resource("/auth/google")
            .route(web::get().to(auth_google))
    )
    .service(
        web::resource("/auth/microsoft")
            .route(web::get().to(auth_microsoft))
    )
    .service(
        web::resource("/auth/callback")
            .route(web::get().to(auth_callback))
    )
    .service(
        web::resource("/auth/sso")
            .route(web::get().to(auth_workos_sso))
    )
    .service(
        web::resource("/auth/workos/callback")
            .route(web::get().to(auth_workos_callback))
    )
    .service(
        web::resource("/connect/gmail")
            .route(web::get().to(connect_gmail))
    )
    .service(
        web::resource("/connect/gmail/callback")
            .route(web::get().to(connect_gmail_callback))
    )
    .service(
        web::resource("/sync/{user_id}")
            .route(web::get().to(sync_emails))
    )
    .service(
        web::resource("/latest/{user_id}")
            .route(web::get().to(get_latest))
    )
    .service(
        web::resource("/temp-mail")
            .route(web::post().to(create_temp_mail))
            .route(web::delete().to(delete_temp_mail))
    )
    .service(
        web::resource("/emails/{id}")
            .route(web::get().to(get_all_emails))
    );
}


