//! 채널 동작: 내 채널 설정.

use super::{AppService, ServiceError};
use crate::dto::MyChannelSettings;

/// OBS "서버" 칸에 넣을 값. 스트리머는 앱과 같은 PC의 MediaMTX(RTMP 1935)로 송출한다.
const OBS_SERVER: &str = "rtmp://localhost/";

/// 제목 규칙: 다음 라이브 제목과 라이브 제목 모두 최대 100자(홈 카드가 깨지지 않게).
const MAX_TITLE_CHARS: usize = 100;

#[derive(sqlx::FromRow)]
struct ChannelSettingsRow {
    id: String,
    stream_key: String,
    next_live_title: Option<String>,
    live_title: Option<String>,
}

impl AppService {
    /// 내 채널 설정 조회. 사용자가 없으면 `None`.
    /// 라이브 중이면 진행 중인 라이브의 제목도 돌려준다.
    pub async fn my_channel_settings(
        &self,
        user_id: i64,
    ) -> Result<Option<MyChannelSettings>, ServiceError> {
        let row = sqlx::query_as::<_, ChannelSettingsRow>(
            "SELECT channels.id, channels.stream_key, channels.next_live_title,
                    lives.title AS live_title
             FROM channels
             LEFT JOIN lives ON lives.channel_id = channels.id AND lives.ended_at_ms IS NULL
             WHERE channels.user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| MyChannelSettings {
            // MediaMTX는 경로를 채널 ID로, 쿼리의 `key`를 송출 키로 받는다.
            obs_stream_key: format!("{}?key={}", row.id, row.stream_key),
            obs_server: OBS_SERVER.to_owned(),
            channel_id: row.id,
            stream_key: row.stream_key,
            next_live_title: row.next_live_title,
            live_title: row.live_title,
        }))
    }

    /// 다음 라이브 제목 바꾸기. 앞뒤 공백을 지우고, 비어 있으면 제목을 지운다(`None`).
    /// 저장한 제목을 돌려준다.
    pub async fn change_next_live_title(
        &self,
        user_id: i64,
        title: &str,
    ) -> Result<Option<String>, ServiceError> {
        let title = valid_title(title)?;
        let updated = sqlx::query("UPDATE channels SET next_live_title = ? WHERE user_id = ?")
            .bind(title)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        // 사용자와 채널은 1:1이므로, 바뀐 행이 없으면 그런 사용자가 없다.
        if updated.rows_affected() == 0 {
            return Err(ServiceError::UnknownUser);
        }
        Ok(title.map(str::to_owned))
    }
}

/// 제목 규칙: 앞뒤 공백을 지운 뒤 최대 100자. 비어 있으면 `None`(제목 없음).
/// 다음 라이브 제목과 진행 중인 라이브 제목([`super::lives`])이 함께 쓴다.
pub(super) fn valid_title(input: &str) -> Result<Option<&str>, ServiceError> {
    let title = input.trim();
    // 바이트가 아니라 글자 수로 센다(한글 한 글자 = 3바이트).
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(ServiceError::TitleTooLong);
    }
    Ok(Some(title).filter(|title| !title.is_empty()))
}
