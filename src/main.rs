use actix_cors::Cors;
use actix_web::{http::header, App, HttpServer, web};
use sqlx::postgres::PgPoolOptions;
use dotenv::dotenv;
use std::env;

mod core;
mod workers;
mod api;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    
    // Create database pool
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to create pool");
    
    println!("✅ Connected to Neon DB");
    
    // Auto-migrate schema changes
    println!("🔄 Running Database Migration...");
    
    // 1. Create temp_aliases table
    let table_res = sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS temp_aliases (
            alias TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at TIMESTAMP DEFAULT NOW()
        );
        "#
    )
    .execute(&pool)
    .await;
    
    match table_res {
        Ok(_) => println!("✅ Table 'temp_aliases' checked/created."),
        Err(e) => eprintln!("❌ Failed to create table: {}", e),
    }

    // 2. Drop old column
    let drop_res = sqlx::query("ALTER TABLE users DROP COLUMN IF EXISTS temp_alias")
        .execute(&pool)
        .await;
        
    match drop_res {
        Ok(_) => println!("✅ Column 'temp_alias' cleanup done."),
        Err(e) => eprintln!("⚠️ Failed to drop column (might not exist): {}", e),
    }

    // 3. Add OTP column to emails table
    let otp_col_res = sqlx::query("ALTER TABLE emails ADD COLUMN IF NOT EXISTS otp TEXT")
        .execute(&pool)
        .await;
        
    match otp_col_res {
        Ok(_) => println!("✅ Column 'otp' checked/added to 'emails'."),
        Err(e) => eprintln!("⚠️ Failed to add 'otp' column: {}", e),
    }
    
    // Spawn SMTP server in background
    let smtp_pool = pool.clone();
    tokio::spawn(async move {
        workers::smtp::start_server(smtp_pool).await;
    });
    
    println!("🚀 HTTP API running on http://0.0.0.0:8080");
    
    // Start HTTP server
    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin("http://localhost:5173")
            .allowed_origin("http://127.0.0.1:5173")
            .allowed_origin("https://mail.rapidxoxo.dpdns.org")
            .allowed_origin("https://rapidxoxo.dpdns.org")
            .allowed_methods(vec!["GET", "POST", "DELETE", "OPTIONS"])
            .allowed_headers(vec![header::AUTHORIZATION, header::ACCEPT])
            .allowed_header(header::CONTENT_TYPE)
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(web::Data::new(pool.clone()))
            .configure(api::routes::config)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
