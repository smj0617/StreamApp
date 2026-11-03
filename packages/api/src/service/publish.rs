//! 송출(Publish) 동작.

use std::collections::HashSet;

use super::lives::close_open_live;
use super::{AppService, ServiceError};

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

    /// 송출 상태 맞추기: 라이브를 실제 송출(`publishing` = 지금 송출 중인 채널 ID 목록)에 맞춘다.
    ///
    /// - 목록에 없는데 열려 있는 라이브는 닫는다(종료 알림을 놓친 유령 라이브).
    /// - 목록에 있는데 열린 라이브가 없는 채널은 새 라이브를 연다(송출 시작 반영과 같다).
    /// - 목록에 있고 열린 라이브도 있는 채널은 그 라이브를 그대로 둔다(시작 시각·제목 유지).
    /// - 채널이 아닌 이름은 경고만 남기고 건너뛴다.
    ///
    /// 앱이 시작할 때(아직 요청을 받기 전) 한 번 부르는 것을 전제로 하므로 전체를 한 트랜잭션으로
    /// 묶지 않는다. 닫기를 먼저 한 번에 하고, 열기는 채널마다 [`AppService::publish_started`]로 한다.
    pub async fn reconcile_publishing(&self, publishing: &[String]) -> Result<(), ServiceError> {
        let mut live_channel_ids: HashSet<String> = self
            .live_channels()
            .await?
            .into_iter()
            .map(|live| live.channel_id)
            .collect();

        let now = self.now_ms();
        let mut tx = self.pool.begin().await?;
        for channel_id in live_channel_ids
            .iter()
            .filter(|id| !publishing.contains(id))
        {
            close_open_live(&mut tx, channel_id, now).await?;
        }
        tx.commit().await?;

        for channel_id in publishing {
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
