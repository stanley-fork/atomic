//! OAuth client and authorization-code storage on Postgres.
//!
//! Mirrors the SQLite registry implementation (registry.rs:555-669) so a
//! Postgres-only deployment can serve the OAuth flow without a registry.db
//! file. Tables are server-global (no db_id) — see migration 006_oauth.sql.

use super::PostgresStorage;
use crate::error::AtomicCoreError;
use crate::registry::OAuthCodeInfo;
use crate::storage::pg_runtime_block_on;
use chrono::Utc;
use uuid::Uuid;

impl PostgresStorage {
    pub(crate) async fn create_oauth_client(
        &self,
        client_name: &str,
        client_secret_hash: &str,
        redirect_uris_json: &str,
    ) -> Result<String, AtomicCoreError> {
        let id = Uuid::new_v4().to_string();
        let client_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO oauth_clients (id, client_id, client_secret_hash, client_name, redirect_uris, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&id)
        .bind(&client_id)
        .bind(client_secret_hash)
        .bind(client_name)
        .bind(redirect_uris_json)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        Ok(client_id)
    }

    pub(crate) async fn get_oauth_client_name(
        &self,
        client_id: &str,
    ) -> Result<Option<String>, AtomicCoreError> {
        sqlx::query_scalar("SELECT client_name FROM oauth_clients WHERE client_id = $1")
            .bind(client_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))
    }

    pub(crate) async fn get_oauth_client_redirect_uris(
        &self,
        client_id: &str,
    ) -> Result<Option<String>, AtomicCoreError> {
        sqlx::query_scalar("SELECT redirect_uris FROM oauth_clients WHERE client_id = $1")
            .bind(client_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))
    }

    pub(crate) async fn get_oauth_client_secret_hash(
        &self,
        client_id: &str,
    ) -> Result<Option<String>, AtomicCoreError> {
        sqlx::query_scalar("SELECT client_secret_hash FROM oauth_clients WHERE client_id = $1")
            .bind(client_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))
    }

    pub(crate) async fn store_oauth_code(
        &self,
        code_hash: &str,
        client_id: &str,
        code_challenge: &str,
        code_challenge_method: &str,
        redirect_uri: &str,
        created_at: &str,
        expires_at: &str,
    ) -> Result<(), AtomicCoreError> {
        sqlx::query(
            "INSERT INTO oauth_codes (code_hash, client_id, code_challenge, code_challenge_method, redirect_uri, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(code_hash)
        .bind(client_id)
        .bind(code_challenge)
        .bind(code_challenge_method)
        .bind(redirect_uri)
        .bind(created_at)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        Ok(())
    }

    pub(crate) async fn lookup_oauth_code(
        &self,
        code_hash: &str,
    ) -> Result<Option<OAuthCodeInfo>, AtomicCoreError> {
        let row = sqlx::query_as::<_, (String, String, String, i32)>(
            "SELECT client_id, code_challenge, expires_at, used FROM oauth_codes WHERE code_hash = $1",
        )
        .bind(code_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        Ok(row.map(|(client_id, code_challenge, expires_at, used)| OAuthCodeInfo {
            client_id,
            code_challenge,
            expires_at,
            used: used != 0,
        }))
    }

    pub(crate) async fn mark_oauth_code_used(
        &self,
        code_hash: &str,
        token_id: Option<&str>,
    ) -> Result<(), AtomicCoreError> {
        sqlx::query("UPDATE oauth_codes SET used = 1 WHERE code_hash = $1")
            .bind(code_hash)
            .execute(&self.pool)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        if let Some(tid) = token_id {
            sqlx::query("UPDATE oauth_codes SET token_id = $1 WHERE code_hash = $2")
                .bind(tid)
                .bind(code_hash)
                .execute(&self.pool)
                .await
                .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        }
        Ok(())
    }

    // ==================== Sync wrappers ====================

    pub(crate) fn create_oauth_client_sync(
        &self,
        client_name: &str,
        client_secret_hash: &str,
        redirect_uris_json: &str,
    ) -> Result<String, AtomicCoreError> {
        pg_runtime_block_on(self.create_oauth_client(client_name, client_secret_hash, redirect_uris_json))
    }

    pub(crate) fn get_oauth_client_name_sync(
        &self,
        client_id: &str,
    ) -> Result<Option<String>, AtomicCoreError> {
        pg_runtime_block_on(self.get_oauth_client_name(client_id))
    }

    pub(crate) fn get_oauth_client_redirect_uris_sync(
        &self,
        client_id: &str,
    ) -> Result<Option<String>, AtomicCoreError> {
        pg_runtime_block_on(self.get_oauth_client_redirect_uris(client_id))
    }

    pub(crate) fn get_oauth_client_secret_hash_sync(
        &self,
        client_id: &str,
    ) -> Result<Option<String>, AtomicCoreError> {
        pg_runtime_block_on(self.get_oauth_client_secret_hash(client_id))
    }

    pub(crate) fn store_oauth_code_sync(
        &self,
        code_hash: &str,
        client_id: &str,
        code_challenge: &str,
        code_challenge_method: &str,
        redirect_uri: &str,
        created_at: &str,
        expires_at: &str,
    ) -> Result<(), AtomicCoreError> {
        pg_runtime_block_on(self.store_oauth_code(
            code_hash,
            client_id,
            code_challenge,
            code_challenge_method,
            redirect_uri,
            created_at,
            expires_at,
        ))
    }

    pub(crate) fn lookup_oauth_code_sync(
        &self,
        code_hash: &str,
    ) -> Result<Option<OAuthCodeInfo>, AtomicCoreError> {
        pg_runtime_block_on(self.lookup_oauth_code(code_hash))
    }

    pub(crate) fn mark_oauth_code_used_sync(
        &self,
        code_hash: &str,
        token_id: Option<&str>,
    ) -> Result<(), AtomicCoreError> {
        pg_runtime_block_on(self.mark_oauth_code_used(code_hash, token_id))
    }
}
