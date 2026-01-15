use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use crate::core::limiter::check_rate_limit;
use mail_parser::{Message, HeaderValue, Addr};

fn extract_sender(message: &Message) -> String {
    // from() returns Option<&HeaderValue>, but we need to handle it correctly
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
            HeaderValue::Group(group) => {
                group.addresses.first()
                    .and_then(|a: &Addr| a.address.as_ref())
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            }
            HeaderValue::GroupList(groups) => {
                groups.first()
                    .and_then(|g| g.addresses.first())
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

pub async fn start_server(pool: PgPool) {
    let listener = TcpListener::bind("0.0.0.0:2525").await.unwrap();
    println!("🛡️ SMTP Server running on :2525 with Rate Limits active");

    let pool = Arc::new(pool);

    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let pool = pool.clone();

        tokio::spawn(async move {
            let mut buffer = [0; 2048]; // 2KB Buffer
            
            // 1. Handshake
            if socket.write_all(b"220 mailpulse.net ESMTP\r\n").await.is_err() { return; }

            // Simplistic State Tracking
            let mut current_user_id = String::new();
            
            loop {
                let n = match socket.read(&mut buffer).await {
                    Ok(n) if n == 0 => return,
                    Ok(n) => n,
                    Err(_) => return,
                };
                let request = String::from_utf8_lossy(&buffer[0..n]);

                // --- LOGIC FLOW ---

                if request.starts_with("HELO") || request.starts_with("EHLO") {
                    let _ = socket.write_all(b"250 OK\r\n").await;
                }
                else if request.starts_with("MAIL FROM") {
                    let _ = socket.write_all(b"250 OK\r\n").await;
                }
                else if request.starts_with("RCPT TO") {
                    // Extract user from: RCPT TO:<user_123@mailpulse.net>
                    // (Simplified parsing logic for demo)
                    if let Some(start) = request.find('<') {
                        if let Some(end) = request.find('@') {
                            let extracted = request[start+1..end].to_string();
                            
                            // Resolve Alias or ID
                            // Check users (id) or temp_aliases (alias)
                            let row = sqlx::query(
                                r#"
                                SELECT id FROM users WHERE id=$1
                                UNION
                                SELECT user_id AS id FROM temp_aliases WHERE alias=$1
                                "#
                            )
                                .bind(&extracted)
                                .fetch_optional(pool.as_ref())
                                .await
                                .unwrap_or(None);
                                
                            if let Some(r) = row {
                                current_user_id = r.get("id");
                            } else {
                                current_user_id = extracted;
                            }
                        }
                    }

                    // 🛑 STEP 1: CHECK RATE LIMIT
                    // Before we say "OK", we check Neon DB
                    if check_rate_limit(&pool, &current_user_id).await {
                        let _ = socket.write_all(b"250 OK\r\n").await;
                    } else {
                        // Rate limit hit: Reject connection
                        println!("🚫 Rate limit hit for {}", current_user_id);
                        let _ = socket.write_all(b"450 Requested mail action not taken: limit exceeded\r\n").await;
                        return; // Close connection
                    }
                }
                else if request.starts_with("DATA") {
                    let _ = socket.write_all(b"354 End data with <CRLF>.<CRLF>\r\n").await;
                    
                    // Read email data until we get <CRLF>.<CRLF>
                    let mut email_data = Vec::new();
                    loop {
                        let n = match socket.read(&mut buffer).await {
                            Ok(n) if n == 0 => break,
                            Ok(n) => n,
                            Err(_) => break,
                        };
                        email_data.extend_from_slice(&buffer[0..n]);
                        
                        // Check for end of data marker
                        if email_data.ends_with(b"\r\n.\r\n") {
                            break;
                        }
                    }
                    
                    // Parse email using mail-parser
                    let (sender, subject, body_preview) = if let Some(message) = Message::parse(&email_data) {
                        let sender_str = extract_sender(&message);
                        let subject_str = message.subject().unwrap_or("").to_string();
                        let body_str = message.body_text(0)
                            .map(|b| b.chars().take(500).collect::<String>())
                            .unwrap_or_default();
                        
                        (sender_str, subject_str, body_str)
                    } else {
                        (String::new(), String::new(), String::new())
                    };
                    
                    // Insert into database
                    let result = sqlx::query(
                        r#"
                        INSERT INTO emails (user_id, sender, subject, body_preview)
                        VALUES ($1, $2, $3, $4)
                        "#
                    )
                    .bind(&current_user_id)
                    .bind(&sender)
                    .bind(&subject)
                    .bind(&body_preview)
                    .execute(pool.as_ref())
                    .await;
                    
                    match result {
                        Ok(_) => {
                            println!("📧 Email saved for {}", current_user_id);
                            let _ = socket.write_all(b"250 OK\r\n").await;
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to save email: {}", e);
                            let _ = socket.write_all(b"451 Requested action aborted: local error\r\n").await;
                        }
                    }
                }
                else if request.starts_with("QUIT") {
                    let _ = socket.write_all(b"221 Bye\r\n").await;
                    return;
                }
            }
        });
    }
}
