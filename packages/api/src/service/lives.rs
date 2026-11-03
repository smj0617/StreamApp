//! 라이브(Live) 동작: 송출 시작·종료 반영과 라이브 중인 채널 목록.
//!
//! 라이브는 송출이 시작될 때 열리고 송출이 끊길 때 닫힌다. 송출이 끊겼다가 다시 붙으면
//! 새 라이브가 된다(유예 시간 없음). 채널마다 열린 라이브는 최대 하나다.

use super::channels::valid_title;
use super::{AppService, ChatEvent, ServiceError};
use crate::dto::LiveChannel;
use crate::rules::fallback_title;

#[derive(sqlx::FromRow)]
struct LiveChannelRow {
    channel_id: String,
    channel_name: String,
    title: String,
    started_at_ms: i64,
}

impl From<LiveChannelRow> for LiveChannel {
    fn from(row: LiveChannelRow) -> Self {
        LiveChannel {
            channel_id: row.channel_id,
            channel_name: row.channel_name,
            title: row.title,
            started_at_ms: row.started_at_ms,
        }
    }
}

impl AppService {
    /// 송출 시작 반영: 그 채널의 새 라이브를 연다. 이미 열린 라이브가 있으면(송출 종료 알림을 놓쳤다)
    /// 먼저 닫고, 송출 종료 반영 때처럼 그 라이브의 채팅 참여자에게 라이브 종료를 알린다.
    ///
    /// 제목은 다음 라이브 제목이고, 비어 있으면 자동 제목("`<채널 이름>`님의 라이브")이다.
    /// 없는 채널이면 [`ServiceError::UnknownChannel`].
    pub async fn publish_started(&self, channel_id: &str) -> Result<(), ServiceError> {
        let now = self.now_ms();
        let mut tx = self.pool.begin().await?;
        // 읽기보다 쓰기를 먼저 해서 트랜잭션이 처음부터 쓰기 잠금을 잡게 한다
        // (읽기로 시작한 트랜잭션이 나중에 쓰기로 올라가면 동시 요청과 SQLITE_BUSY가 날 수 있다).
        let replaced = close_open_live(&mut tx, channel_id, now).await?;
        let channel = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT name, next_live_title FROM channels WHERE id = ?",
        )
        .bind(channel_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((channel_name, next_live_title)) = channel else {
            return Err(ServiceError::UnknownChannel);
        };
        // 다음 라이브 제목은 비어 있으면 NULL로 저장된다(빈 문자열은 저장되지 않는다).
        let title = next_live_title.unwrap_or_else(|| fallback_title(&channel_name));
        sqlx::query("INSERT INTO lives (channel_id, title, started_at_ms) VALUES (?, ?, ?)")
            .bind(channel_id)
            .bind(title)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        if replaced {
            // 커밋 전에 알린다. 지금 참여자는 닫힌 라이브의 참여자이고, 이 뒤에 참여하는 쪽은 새
            // broadcast에 붙어 새 라이브의 채팅을 받는다(커밋 뒤에 알리면 그 사이 새 라이브에 참여한
            // 쪽까지 종료 알림을 받는다).
            self.chat.end_live(channel_id);
        }
        tx.commit().await?;
        Ok(())
    }

    /// 송출 종료 반영: 그 채널의 열린 라이브를 닫고 종료 시각을 기록한 뒤, 그 채널의 채팅
    /// 참여자에게 라이브 종료를 알린다. 열린 라이브가 없으면(없는 채널 포함) 아무것도 하지 않는다.
    pub async fn publish_ended(&self, channel_id: &str) -> Result<(), ServiceError> {
        let mut conn = self.pool.acquire().await?;
        close_open_live(&mut conn, channel_id, self.now_ms()).await?;
        // 닫은 뒤에 알린다: 알림을 받고 다시 참여하려는 쪽이 라이브 중이 아님을 보게 된다.
        self.chat.end_live(channel_id);
        Ok(())
    }

    /// 그 채널에 열린 라이브가 있는지. 없는 채널이면 `false`.
    pub(super) async fn is_live(&self, channel_id: &str) -> Result<bool, ServiceError> {
        let live = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM lives WHERE channel_id = ? AND ended_at_ms IS NULL)",
        )
        .bind(channel_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(live)
    }

    /// 라이브 중인 채널 목록. 최근에 시작한 순서로 돌려준다.
    pub async fn live_channels(&self) -> Result<Vec<LiveChannel>, ServiceError> {
        let rows = sqlx::query_as::<_, LiveChannelRow>(
            "SELECT lives.channel_id, channels.name AS channel_name, lives.title, lives.started_at_ms
             FROM lives JOIN channels ON channels.id = lives.channel_id
             WHERE lives.ended_at_ms IS NULL
             ORDER BY lives.started_at_ms DESC, lives.id DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(LiveChannel::from).collect())
    }

    /// 진행 중인 라이브 제목 바꾸기. 저장한 제목을 돌려주고, 그 라이브의 채팅 참여자에게
    /// [`ChatEvent::LiveTitleChanged`]로 알린다(열려 있는 시청 페이지가 제목을 바로 바꾼다).
    ///
    /// 제목 규칙은 다음 라이브 제목과 같다(앞뒤 공백을 지운 뒤 최대 100자). 비우면 다음 라이브
    /// 제목 없이 시작했을 때처럼 자동 제목("`<채널 이름>`님의 라이브")이 된다(제목 없는 라이브는 없다).
    /// 그 사용자의 채널이 라이브 중이 아니면 [`ServiceError::NotLive`], 없는 사용자면
    /// [`ServiceError::UnknownUser`].
    pub async fn change_live_title(
        &self,
        user_id: i64,
        title: &str,
    ) -> Result<String, ServiceError> {
        let title = valid_title(title)?;
        let channel = sqlx::query_as::<_, (String, String)>(
            "SELECT id, name FROM channels WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        // 사용자와 채널은 1:1이므로, 채널이 없으면 그런 사용자가 없다.
        let Some((channel_id, channel_name)) = channel else {
            return Err(ServiceError::UnknownUser);
        };
        let title = title.map_or_else(|| fallback_title(&channel_name), str::to_owned);
        let updated =
            sqlx::query("UPDATE lives SET title = ? WHERE channel_id = ? AND ended_at_ms IS NULL")
                .bind(&title)
                .bind(&channel_id)
                .execute(&self.pool)
                .await?;
        if updated.rows_affected() == 0 {
            return Err(ServiceError::NotLive);
        }
        self.chat.send(
            &channel_id,
            ChatEvent::LiveTitleChanged {
                title: title.clone(),
            },
        );
        Ok(title)
    }
}

/// 그 채널의 열린 라이브(있으면)를 닫고 종료 시각을 기록한다. 닫은 라이브가 있으면 `true`.
pub(super) async fn close_open_live(
    conn: &mut sqlx::SqliteConnection,
    channel_id: &str,
    now: i64,
) -> Result<bool, ServiceError> {
    let closed = sqlx::query(
        "UPDATE lives SET ended_at_ms = ? WHERE channel_id = ? AND ended_at_ms IS NULL",
    )
    .bind(now)
    .bind(channel_id)
    .execute(conn)
    .await?;
    Ok(closed.rows_affected() > 0)
}
