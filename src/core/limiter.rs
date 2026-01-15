use sqlx::PgPool;
use sqlx::Row;

const MAX_EMAILS: i64 = 100;
// const TIME_WINDOW_MINUTES: i64 = 10; // Used in query

/// Returns TRUE if user is allowed to receive mail
/// Returns FALSE if they hit the limit
pub async fn check_rate_limit(pool: &PgPool, user_id: &str) -> bool {
    // ⚡ Efficient Neon Query
    // Thanks to the Index, this count is extremely fast/cheap
    let result = sqlx::query(
        r#"
        SELECT count(*) as count
        FROM emails 
        WHERE user_id = $1 
          AND received_at > NOW() - INTERVAL '10 minutes'
        "#
    )
    .bind(user_id)
    .fetch_one(pool)
    .await;

    match result {
        Ok(row) => {
            let count: i64 = row.get("count");
            count < MAX_EMAILS
        }
        Err(_) => false, // Fail closed (deny) on DB error for safety
    }
}
