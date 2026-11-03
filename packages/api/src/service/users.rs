//! 사용자 동작.

use sqlx::SqliteConnection;

use super::{ids, now_ms, AppService, ServiceError};
use crate::dto::User;

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

/// 사용자와 그 사용자의 채널(채널 이름 = 닉네임)을 함께 만든다.
/// 닉네임 규칙 확인은 부르는 쪽이 맡는다.
async fn insert_user_with_channel(
    conn: &mut SqliteConnection,
    nickname: &str,
) -> Result<(), sqlx::Error> {
    let user_id = sqlx::query(
        "INSERT INTO users (nickname, nickname_key, created_at_ms) VALUES (?, ?, ?)",
    )
    .bind(nickname)
    .bind(nickname_key(nickname))
    .bind(now_ms())
    .execute(&mut *conn)
    .await?
    .last_insert_rowid();

    sqlx::query("INSERT INTO channels (id, user_id, name, stream_key) VALUES (?, ?, ?, ?)")
        .bind(ids::new_channel_id())
        .bind(user_id)
        .bind(nickname)
        .bind(ids::new_stream_key())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// 닉네임 중복 판단용 키: 영문 대소문자만 무시한다(한글 등은 그대로).
fn nickname_key(nickname: &str) -> String {
    nickname.to_ascii_lowercase()
}
