//! 사용자 동작.

use sqlx::SqliteConnection;

use super::{ids, now_ms, AppService, ServiceError};
use crate::dto::{CreatedUser, User};

/// DB가 비어 있을 때 만드는 기본 사용자.
const DEFAULT_NICKNAMES: [&str; 3] = ["철수", "영희", "민수"];

#[derive(sqlx::FromRow)]
struct UserRow {
    id: i64,
    nickname: String,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        User {
            id: row.id,
            nickname: row.nickname,
        }
    }
}

impl AppService {
    /// 사용자 목록. 만든 순서대로 돌려준다.
    pub async fn list_users(&self) -> Result<Vec<User>, ServiceError> {
        let rows = sqlx::query_as::<_, UserRow>("SELECT id, nickname FROM users ORDER BY id")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(User::from).collect())
    }

    /// ID로 사용자 찾기. 없으면 `None`(예: 쿠키에 남은 ID가 지워진 DB에 없을 때).
    pub async fn find_user(&self, user_id: i64) -> Result<Option<User>, ServiceError> {
        let row = sqlx::query_as::<_, UserRow>("SELECT id, nickname FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(User::from))
    }

    /// 사용자 만들기: 사용자와 그 사용자의 채널(채널 이름 = 닉네임)을 함께 만든다.
    pub async fn create_user(&self, nickname: &str) -> Result<CreatedUser, ServiceError> {
        let nickname = valid_nickname(nickname)?;
        let mut tx = self.pool.begin().await?;
        let created = insert_user_with_channel(&mut tx, nickname).await?;
        tx.commit().await?;
        Ok(created)
    }

    /// 기본 데이터: 사용자가 한 명도 없을 때만 사용자 3명과 각자의 채널을 만든다.
    /// 라이브는 만들지 않는다. 만들었으면 `true`를 돌려준다.
    pub async fn seed_default_users(&self) -> Result<bool, ServiceError> {
        let mut tx = self.pool.begin().await?;
        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&mut *tx)
            .await?;
        if user_count > 0 {
            return Ok(false);
        }
        for nickname in DEFAULT_NICKNAMES {
            insert_user_with_channel(&mut tx, nickname).await?;
        }
        tx.commit().await?;
        Ok(true)
    }
}

/// 사용자와 그 사용자의 채널(채널 이름 = 닉네임)을 함께 만들고, 저장된 값을 돌려준다.
/// 닉네임 규칙 확인은 부르는 쪽이 맡고, 중복은 `nickname_key`의 UNIQUE 제약으로 거절한다.
async fn insert_user_with_channel(
    conn: &mut SqliteConnection,
    nickname: &str,
) -> Result<CreatedUser, ServiceError> {
    let user = sqlx::query_as::<_, UserRow>(
        "INSERT INTO users (nickname, nickname_key, created_at_ms) VALUES (?, ?, ?)
         RETURNING id, nickname",
    )
    .bind(nickname)
    .bind(nickname_key(nickname))
    .bind(now_ms())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| match e {
        // users의 UNIQUE 열은 nickname_key뿐이다(id는 자동 증가).
        sqlx::Error::Database(db) if db.is_unique_violation() => ServiceError::DuplicateNickname,
        other => other.into(),
    })?;

    let (channel_id, channel_name) = sqlx::query_as::<_, (String, String)>(
        "INSERT INTO channels (id, user_id, name, stream_key) VALUES (?, ?, ?, ?)
         RETURNING id, name",
    )
    .bind(ids::new_channel_id())
    .bind(user.id)
    .bind(nickname)
    .bind(ids::new_stream_key())
    .fetch_one(&mut *conn)
    .await?;

    Ok(CreatedUser {
        user: user.into(),
        channel_id,
        channel_name,
    })
}

/// 닉네임 규칙: 앞뒤 공백을 지운 뒤 2~10자이고 한글·영문·숫자만 쓴다.
/// 통과하면 공백을 지운 닉네임을 돌려준다.
fn valid_nickname(input: &str) -> Result<&str, ServiceError> {
    let nickname = input.trim();
    // 바이트가 아니라 글자 수로 센다(한글 한 글자 = 3바이트).
    if !(2..=10).contains(&nickname.chars().count()) {
        return Err(ServiceError::NicknameLength);
    }
    // `char::is_alphanumeric`은 다른 문자 체계도 통과시키므로 범위를 직접 정한다.
    let allowed = |c: char| c.is_ascii_alphanumeric() || ('가'..='힣').contains(&c);
    if !nickname.chars().all(allowed) {
        return Err(ServiceError::NicknameCharacters);
    }
    Ok(nickname)
}

/// 닉네임 중복 판단용 키: 영문 대소문자만 무시한다(한글 등은 그대로).
fn nickname_key(nickname: &str) -> String {
    nickname.to_ascii_lowercase()
}
