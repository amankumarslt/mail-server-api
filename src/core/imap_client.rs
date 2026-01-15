use async_std::net::TcpStream;
use async_native_tls::TlsStream;
use mail_parser::Message;
use futures::StreamExt;

pub struct ImapCredentials {
    pub email: String,
    pub password: Option<String>,      // Regular password or app password
    pub access_token: Option<String>,  // OAuth access token
    pub server: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct FetchedEmail {
    pub message_id: Option<String>,
    pub sender: String,
    pub subject: String,
    pub body_preview: String,
    pub received_at: i64,
}

/// Fetch the latest email from an IMAP server (supports both password and OAuth)
pub async fn fetch_latest_email(creds: &ImapCredentials) -> Result<Option<FetchedEmail>, String> {
    // Connect to IMAP server using async-std
    let addr = format!("{}:{}", creds.server, creds.port);
    let tcp_stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;

    // Establish TLS connection
    let tls = async_native_tls::TlsConnector::new();
    let tls_stream: TlsStream<TcpStream> = tls
        .connect(&creds.server, tcp_stream)
        .await
        .map_err(|e| format!("TLS error: {}", e))?;

    // Create IMAP client
    let client = async_imap::Client::new(tls_stream);
    
    // Login - use OAuth or password
    let mut session = if let Some(ref access_token) = creds.access_token {
        // XOAUTH2 authentication
        let xoauth2 = xoauth2_string(&creds.email, access_token);
        client
            .authenticate("XOAUTH2", XOAuth2Authenticator { token: xoauth2 })
            .await
            .map_err(|(e, _)| format!("OAuth login failed: {}", e))?
    } else if let Some(ref password) = creds.password {
        // Regular password authentication
        client
            .login(&creds.email, password)
            .await
            .map_err(|(e, _)| format!("Login failed: {}", e))?
    } else {
        return Err("No credentials provided".to_string());
    };

    // Select INBOX
    session
        .select("INBOX")
        .await
        .map_err(|e| format!("Failed to select INBOX: {}", e))?;

    // Search for all messages and get the latest one
    let search_result = session
        .search("ALL")
        .await
        .map_err(|e| format!("Search failed: {}", e))?;

    // Get the highest UID (latest email)
    let latest_uid = search_result.iter().max().copied();
    
    let mut result_email: Option<FetchedEmail> = None;
    
    if let Some(uid) = latest_uid {
        // Fetch the email
        let mut messages_stream = session
            .fetch(uid.to_string(), "RFC822")
            .await
            .map_err(|e| format!("Fetch failed: {}", e))?;

        while let Some(message_result) = messages_stream.next().await {
            if let Ok(message) = message_result {
                if let Some(body) = message.body() {
                    if let Some(parsed) = Message::parse(body) {
                        result_email = Some(FetchedEmail {
                            message_id: parsed.message_id().map(|s| s.to_string()),
                            sender: extract_sender(&parsed),
                            subject: parsed.subject().unwrap_or("").to_string(),
                            body_preview: parsed
                                .body_text(0)
                                .map(|b| b.chars().take(500).collect::<String>())
                                .unwrap_or_default(),
                            received_at: parsed.date()
                                .map(|d| d.to_timestamp())
                                .unwrap_or_else(|| chrono::Utc::now().timestamp()),
                        });
                        break;
                    }
                }
            }
        }
    }

    // Logout
    let _ = session.logout().await;
    Ok(result_email)
}

fn extract_sender(message: &Message) -> String {
    use mail_parser::{HeaderValue, Addr};
    
    if let Some(header_value) = message.header("From") {
        match header_value {
            HeaderValue::Address(addr) => {
                addr.address.as_ref().map(|s| s.to_string()).unwrap_or_default()
            }
            HeaderValue::AddressList(list) => {
                list.first()
                    .and_then(|a: &Addr| a.address.as_ref())
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            }
            _ => String::new(),
        }
    } else {
        String::new()
    }
}

/// Generate XOAUTH2 string for IMAP authentication
fn xoauth2_string(email: &str, access_token: &str) -> String {
    use base64::Engine;
    let auth_string = format!("user={}\x01auth=Bearer {}\x01\x01", email, access_token);
    base64::engine::general_purpose::STANDARD.encode(auth_string)
}

/// XOAUTH2 Authenticator for async-imap
struct XOAuth2Authenticator {
    token: String,
}

impl async_imap::Authenticator for XOAuth2Authenticator {
    type Response = String;
    
    fn process(&mut self, _data: &[u8]) -> Self::Response {
        self.token.clone()
    }
}
