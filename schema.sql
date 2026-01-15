-- Mail Server Database Schema for Neon DB
-- Run this SQL in your Neon Console

-- 1. Users table (with IMAP and OAuth credentials)
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    -- IMAP credentials (fallback)
    imap_server TEXT,
    imap_port INTEGER DEFAULT 993,
    imap_password TEXT,
    -- OAuth credentials
    auth_provider TEXT,              -- 'google', 'microsoft', or null for IMAP
    access_token TEXT,
    refresh_token TEXT,
    token_expires_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW()
);

-- 1b. Temp Aliases table
CREATE TABLE IF NOT EXISTS temp_aliases (
    alias TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMP DEFAULT NOW()
);

-- 2. Emails table
CREATE TABLE IF NOT EXISTS emails (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    message_id TEXT,
    sender TEXT NOT NULL,
    subject TEXT,
    body_preview TEXT,
    received_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, message_id)
);

-- Performance indexes
CREATE INDEX IF NOT EXISTS idx_rate_limit ON emails (user_id, received_at DESC);
CREATE INDEX IF NOT EXISTS idx_user_auth ON users (auth_provider);

-- Migration for existing tables:
-- ALTER TABLE users ADD COLUMN IF NOT EXISTS auth_provider TEXT;
-- ALTER TABLE users ADD COLUMN IF NOT EXISTS access_token TEXT;
-- ALTER TABLE users ADD COLUMN IF NOT EXISTS refresh_token TEXT;
-- ALTER TABLE users ADD COLUMN IF NOT EXISTS token_expires_at TIMESTAMP;
