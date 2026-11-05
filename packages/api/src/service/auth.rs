//! 로그인 동작: 비밀번호 확인, 로그인 세션 만들기·찾기·끝내기(ADR 0005).
//!
//! 비밀번호는 Argon2id 해시(PHC 문자열)로만 저장한다. 세션 토큰은 쿠키에 담아 브라우저에 주고,
//! DB에는 그 SHA-256 값만 남긴다(DB가 새도 토큰을 그대로 쓸 수 없다). HTTP·쿠키는 모른다.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::{distr::Alphanumeric, Rng, RngCore};
use sha2::{Digest, Sha256};

use super::{AppService, ServiceError};
use crate::dto::User;
use crate::rules::{MAX_PASSWORD_CHARS, MIN_PASSWORD_CHARS};

/// 로그인 세션이 유지되는 시간: 30일.
pub const SESSION_LIFETIME_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// 세션 토큰 길이(대소문자 영문·숫자, 약 250비트).
const SESSION_TOKEN_LEN: usize = 43;

#[derive(sqlx::FromRow)]
struct LoginRow {
    id: i64,
    nickname: String,
    password_hash: Option<String>,
}

impl AppService {
    /// 로그인: 닉네임(영문 대소문자 무시)과 비밀번호가 맞으면 그 사용자를 돌려준다. 세션은
    /// [`AppService::start_session`]으로 따로 만든다.
    ///
    /// 닉네임이 없거나, 비밀번호가 없거나(이전 단계 사용자, 앱 시작 때 채운다), 틀리면 모두
    /// [`ServiceError::LoginFailed`]다. 어느 쪽이 틀렸는지 알려 주지 않는다.
    pub async fn log_in(&self, nickname: &str, password: &str) -> Result<User, ServiceError> {
        let row = sqlx::query_as::<_, LoginRow>(
            "SELECT id, nickname, password_hash FROM users WHERE nickname_key = ?",
        )
        .bind(nickname.trim().to_ascii_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        let Some(LoginRow {
            id,
            nickname,
            password_hash: Some(hash),
        }) = row
        else {
            return Err(ServiceError::LoginFailed);
        };
        if verify_password(hash, password.to_owned()).await? {
            Ok(User { id, nickname })
        } else {
            Err(ServiceError::LoginFailed)
        }
    }

    /// 그 사용자의 로그인 세션을 새로 만들고 세션 토큰을 돌려준다(쿠키에 담을 값). 이 김에 기한이 지난
    /// 세션을 지운다. 없는 사용자면 [`ServiceError::UnknownUser`].
    pub async fn start_session(&self, user_id: i64) -> Result<String, ServiceError> {
        let now = self.now_ms();
        sqlx::query("DELETE FROM sessions WHERE expires_at_ms <= ?")
            .bind(now)
            .execute(&self.pool)
            .await?;
        let token: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .map(char::from)
            .take(SESSION_TOKEN_LEN)
            .collect();
        sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at_ms) VALUES (?, ?, ?)")
            .bind(token_hash(&token))
            .bind(user_id)
            .bind(now + SESSION_LIFETIME_MS)
            .execute(&self.pool)
            .await
            .map_err(|error| match error {
                sqlx::Error::Database(db) if db.is_foreign_key_violation() => {
                    ServiceError::UnknownUser
                }
                other => other.into(),
            })?;
        Ok(token)
    }

    /// 세션 토큰의 사용자. 없는 토큰, 기한이 지난 토큰, 로그아웃한 토큰이면 `None`.
    pub async fn session_user(&self, token: &str) -> Result<Option<User>, ServiceError> {
        let row = sqlx::query_as::<_, (i64, String)>(
            "SELECT users.id, users.nickname
             FROM sessions JOIN users ON users.id = sessions.user_id
             WHERE sessions.token_hash = ? AND sessions.expires_at_ms > ?",
        )
        .bind(token_hash(token))
        .bind(self.now_ms())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id, nickname)| User { id, nickname }))
    }

    /// 로그아웃: 그 세션을 지운다. 이미 없는 세션이어도 성공이다.
    pub async fn end_session(&self, token: &str) -> Result<(), ServiceError> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash(token))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 비밀번호가 없는 사용자(이전 단계에서 닉네임만으로 만든 사용자)에게 `password`를 넣는다.
    /// 넣은 사용자 수를 돌려준다.
    pub async fn fill_missing_passwords(&self, password: &str) -> Result<u64, ServiceError> {
        let hash = hash_password(valid_password(password)?.to_owned()).await?;
        let updated = sqlx::query("UPDATE users SET password_hash = ? WHERE password_hash IS NULL")
            .bind(hash)
            .execute(&self.pool)
            .await?;
        Ok(updated.rows_affected())
    }
}

/// 비밀번호 규칙: [`MIN_PASSWORD_CHARS`]~[`MAX_PASSWORD_CHARS`]자. 공백도 그대로 센다(지우지 않는다).
pub(super) fn valid_password(password: &str) -> Result<&str, ServiceError> {
    let chars = password.chars().count();
    if (MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&chars) {
        Ok(password)
    } else {
        Err(ServiceError::PasswordLength)
    }
}

/// 비밀번호를 무작위 소금과 함께 Argon2id로 해시한다(PHC 문자열). 계산이 무거워 블로킹 스레드에서 한다.
pub(super) async fn hash_password(password: String) -> Result<String, ServiceError> {
    let mut salt = [0u8; 16];
    rand::rng().fill_bytes(&mut salt);
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::encode_b64(&salt).map_err(|e| e.to_string())?;
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| ServiceError::PasswordHash(e.to_string()))?
    .map_err(ServiceError::PasswordHash)
}

/// 비밀번호가 저장된 해시와 맞는지. 해시를 읽을 수 없으면 서버 잘못이다.
async fn verify_password(hash: String, password: String) -> Result<bool, ServiceError> {
    tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&hash).map_err(|e| e.to_string())?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .map_err(|e| ServiceError::PasswordHash(e.to_string()))?
    .map_err(ServiceError::PasswordHash)
}

/// DB에 남기는 세션 토큰 값: SHA-256 16진수.
fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
