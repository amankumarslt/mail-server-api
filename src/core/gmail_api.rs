use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct GmailMessageList {
    messages: Option<Vec<GmailMessageRef>>,
}

#[derive(Debug, Deserialize)]
struct GmailMessageRef {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailMessage {
    id: String,
    payload: Option<GmailPayload>,
    snippet: Option<String>,
    internal_date: Option<String>, // Gmail returns this as stringified long
}

#[derive(Debug, Deserialize)]
struct GmailPayload {
    headers: Option<Vec<GmailHeader>>,
}

#[derive(Debug, Deserialize)]
struct GmailHeader {
    name: String,
    value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FetchedEmail {
    pub message_id: String,
    pub sender: String,
    pub subject: String,
    pub body_preview: String,
    pub received_at: i64, // Timestamp in milliseconds
}

/// Fetch emails from Gmail API
pub async fn fetch_gmail_emails(access_token: &str, max_results: u32) -> Result<Vec<FetchedEmail>, String> {
    let client = reqwest::Client::new();
    
    // 1. List messages
    let list_url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults={}",
        max_results
    );
    
    let list_resp = client
        .get(&list_url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Failed to list messages: {}", e))?;

    if !list_resp.status().is_success() {
        let error_text = list_resp.text().await.unwrap_or_default();
        return Err(format!("Gmail API error: {}", error_text));
    }

    let message_list: GmailMessageList = list_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse message list: {}", e))?;

    let messages = match message_list.messages {
        Some(m) if !m.is_empty() => m,
        _ => return Ok(vec![]),
    };

    // 2. Fetch each message details
    let mut emails = Vec::new();
    
    for msg_ref in messages.iter().take(max_results as usize) {
        let msg_url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=metadata&metadataHeaders=From&metadataHeaders=Subject",
            msg_ref.id
        );
        
        let msg_resp = client
            .get(&msg_url)
            .bearer_auth(access_token)
            .send()
            .await;

        if let Ok(resp) = msg_resp {
            if let Ok(msg) = resp.json::<GmailMessage>().await {
                let mut sender = String::new();
                let mut subject = String::new();
                
                if let Some(payload) = msg.payload {
                    if let Some(headers) = payload.headers {
                        for header in headers {
                            match header.name.as_str() {
                                "From" => sender = header.value,
                                "Subject" => subject = header.value,
                                _ => {}
                            }
                        }
                    }
                }
                
                let internal_date = msg.internal_date
                    .and_then(|d| d.parse::<i64>().ok())
                    .map(|ms| ms / 1000)
                    .unwrap_or_else(|| chrono::Utc::now().timestamp());

                emails.push(FetchedEmail {
                    message_id: msg.id,
                    sender,
                    subject,
                    body_preview: msg.snippet.unwrap_or_default(),
                    received_at: internal_date,
                });
            }
        }
    }

    Ok(emails)
}

/// Fetch latest email from Gmail API
pub async fn fetch_gmail_latest(access_token: &str) -> Result<Option<FetchedEmail>, String> {
    let emails = fetch_gmail_emails(access_token, 1).await?;
    Ok(emails.into_iter().next())
}
