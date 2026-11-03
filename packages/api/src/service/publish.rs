//! 송출(Publish) 동작.

use std::collections::HashSet;

use super::lives::close_open_live;
use super::{AppService, ServiceError};

/// 송출 상태 맞추기의 입력: 미디어 서버(MediaMTX)에 물어 알아낸 실제 송출.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublishingStatus {
    /// 지금 송출 중인 채널 ID 목록을 알아냈다.
    Known(Vec<String>),
    /// 미디어 서버에 연결할 수 없다(연결 거부·타임아웃). 꺼져 있는 미디어 서버로는 송출할 수 없으므로
    /// 송출 중인 채널이 없는 것과 같다.
    Unreachable,
    /// 연결은 됐지만 답을 쓸 수 없다(HTTP 오류 상태, 응답 해석 실패). 누가 송출 중인지 알 수 없다.
    Unknown,
}

impl AppService {
    /// 송출 허용 판단: 그 채널이 있고 송출 키가 그 채널의 것이면 `true`(허용), 아니면 `false`(거절).
    pub async fn can_publish(
        &self,
        channel_id: &str,
        stream_key: &str,
    ) -> Result<bool, ServiceError> {
        let allowed: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM channels WHERE id = ? AND stream_key = ?)",
        )
        .bind(channel_id)
        .bind(stream_key)
        .fetch_one(&self.pool)
        .await?;
        Ok(allowed)
    }

    /// 송출 상태 맞추기: 라이브를 실제 송출에 맞춘다.
    ///
    /// 송출 중인 채널 ID 목록([`PublishingStatus::Known`])이면
    ///
    /// - 목록에 없는데 열려 있는 라이브는 닫고, 그 채널의 채팅 참여자에게 라이브 종료를 알린다
    ///   (종료 알림을 놓친 유령 라이브).
    /// - 목록에 있는데 열린 라이브가 없는 채널은 새 라이브를 연다(송출 시작 반영과 같다).
    /// - 목록에 있고 열린 라이브도 있는 채널은 그 라이브를 그대로 둔다(시작 시각·제목 유지).
    /// - 채널이 아닌 이름은 경고만 남기고 건너뛴다.
    ///
    /// 미디어 서버에 연결할 수 없으면([`PublishingStatus::Unreachable`]) 빈 목록처럼 열린 라이브를 모두
    /// 닫고, 송출 상태를 알 수 없으면([`PublishingStatus::Unknown`]) 라이브를 건드리지 않는다.
    ///
    /// 앱이 시작할 때(아직 요청을 받기 전) 한 번 부르는 것을 전제로 하므로 전체를 한 트랜잭션으로
    /// 묶지 않는다. 닫기를 먼저 한 번에 하고, 열기는 채널마다 [`AppService::publish_started`]로 한다.
    pub async fn reconcile_publishing(&self, status: PublishingStatus) -> Result<(), ServiceError> {
        let publishing = match status {
            PublishingStatus::Known(channel_ids) => channel_ids,
            PublishingStatus::Unreachable => Vec::new(),
            PublishingStatus::Unknown => return Ok(()),
        };
        let mut live_channel_ids: HashSet<String> = self
            .live_channels()
            .await?
            .into_iter()
            .map(|live| live.channel_id)
            .collect();

        let now = self.now_ms();
        let ghosts: Vec<&String> = live_channel_ids
            .iter()
            .filter(|id| !publishing.contains(id))
            .collect();
        let mut tx = self.pool.begin().await?;
        for channel_id in &ghosts {
            close_open_live(&mut tx, channel_id, now).await?;
        }
        tx.commit().await?;
        for channel_id in ghosts {
            self.chat.end_live(channel_id);
        }

        for channel_id in &publishing {
            // 이미 열린 라이브가 있거나(같은 ID가 목록에 두 번 있어도) 방금 연 채널은 건너뛴다.
            if !live_channel_ids.insert(channel_id.clone()) {
                continue;
            }
            match self.publish_started(channel_id).await {
                Ok(()) => {}
                Err(ServiceError::UnknownChannel) => {
                    tracing::warn!(
                        "송출 상태 맞추기: 채널이 아닌 경로 '{channel_id}'는 건너뜁니다"
                    );
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
