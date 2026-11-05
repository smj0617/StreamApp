//! 녹화방송(Recording) 동작: 등록(영상 파일 저장 포함), 목록, 상세, 제목 수정·삭제(주인만).
//!
//! 영상 파일은 녹화방송 폴더에 무작위 이름으로 저장하고, DB에는 제목과 함께 그 파일 이름(영상 위치)을
//! 남긴다(ADR 0003). 등록이 어느 단계에서 실패하든 폴더에 쓰다 만 파일을 남기지 않는다.

use std::fmt::Display;
use std::path::Path;

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use tokio::io::AsyncWriteExt;

use super::channels::valid_title;
use super::{ids, AppService, ServiceError};
use crate::dto::{RecordingPage, RecordingSummary};
use crate::rules::recording_extension;
use crate::RECORDING_MEDIA_PATH;

/// 기본 녹화방송 폴더. 서버를 실행한 폴더(개발 스크립트는 레포 루트) 기준이며, DB 파일처럼 git에서 제외한다.
pub const DEFAULT_RECORDINGS_DIR: &str = "data/recordings";

/// 받는 중인 파일에 붙이는 확장자. 다 받은 뒤에 떼어 내므로, 이 확장자가 붙은 파일은 등록되지 않은 것이다.
const PARTIAL_SUFFIX: &str = ".part";

#[derive(sqlx::FromRow)]
struct RecordingSummaryRow {
    id: i64,
    title: String,
    channel_name: String,
    registered_at_ms: i64,
}

#[derive(sqlx::FromRow)]
struct RecordingPageRow {
    id: i64,
    title: String,
    channel_id: String,
    owner_id: i64,
    channel_name: String,
    file_name: String,
    registered_at_ms: i64,
}

impl AppService {
    /// 녹화방송 등록: 그 사용자의 채널에 제목과 영상 파일로 녹화방송을 하나 만들고 ID를 돌려준다.
    ///
    /// `file_name`은 사용자가 고른 원래 파일 이름이며 확장자(mp4·webm)만 본다. `body`는 영상 파일 내용을
    /// 조각으로 받는 흐름이다. 제목·확장자·사용자를 먼저 확인한 뒤에 받기 시작하므로, 규칙에 어긋나면
    /// 파일을 받지 않는다.
    ///
    /// 거절: 제목이 비었으면 [`ServiceError::RecordingTitleEmpty`], 100자를 넘으면
    /// [`ServiceError::TitleTooLong`], 확장자가 다르면 [`ServiceError::RecordingFormat`], 없는 사용자면
    /// [`ServiceError::UnknownUser`], 빈 파일이면 [`ServiceError::RecordingFileEmpty`], 최대 크기를 넘으면
    /// [`ServiceError::RecordingTooLarge`], 받는 중 흐름이 오류를 내면 [`ServiceError::UploadInterrupted`].
    pub async fn register_recording<S, E>(
        &self,
        user_id: i64,
        title: &str,
        file_name: &str,
        body: S,
    ) -> Result<i64, ServiceError>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin,
        E: Display,
    {
        let title = valid_title(title)?.ok_or(ServiceError::RecordingTitleEmpty)?;
        let extension = recording_extension(file_name).ok_or(ServiceError::RecordingFormat)?;
        let channel_id =
            sqlx::query_scalar::<_, String>("SELECT id FROM channels WHERE user_id = ?")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?
                // 사용자와 채널은 1:1이므로, 채널이 없으면 그런 사용자가 없다.
                .ok_or(ServiceError::UnknownUser)?;

        let stored_name = format!("{}.{extension}", ids::new_recording_file_stem());
        let path = self.recordings_dir.join(&stored_name);
        let partial = self
            .recordings_dir
            .join(format!("{stored_name}{PARTIAL_SUFFIX}"));
        let saved = match self.write_upload(&partial, body).await {
            Ok(()) => tokio::fs::rename(&partial, &path)
                .await
                .map_err(ServiceError::from),
            Err(error) => Err(error),
        };
        if let Err(error) = saved {
            remove_quietly(&partial).await;
            return Err(error);
        }

        let inserted = sqlx::query(
            "INSERT INTO recordings (channel_id, title, file_name, registered_at_ms)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&channel_id)
        .bind(title)
        .bind(&stored_name)
        .bind(self.now_ms())
        .execute(&self.pool)
        .await;
        match inserted {
            Ok(done) => Ok(done.last_insert_rowid()),
            Err(error) => {
                remove_quietly(&path).await;
                Err(error.into())
            }
        }
    }

    /// 받은 조각을 `path`에 차례로 쓴다. 최대 크기를 넘는 조각은 쓰기 전에 거절한다.
    async fn write_upload<S, E>(&self, path: &Path, mut body: S) -> Result<(), ServiceError>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin,
        E: Display,
    {
        tokio::fs::create_dir_all(&self.recordings_dir).await?;
        let mut file = tokio::fs::File::create(path).await?;
        let mut written: u64 = 0;
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|error| {
                tracing::info!("녹화방송 영상을 받다가 멈췄습니다: {error}");
                ServiceError::UploadInterrupted
            })?;
            written += chunk.len() as u64;
            if written > self.max_recording_bytes {
                return Err(ServiceError::RecordingTooLarge);
            }
            file.write_all(&chunk).await?;
        }
        if written == 0 {
            return Err(ServiceError::RecordingFileEmpty);
        }
        // 이름을 바꾸기 전에 남은 쓰기를 모두 끝낸다(tokio 파일은 쓰기를 뒤에서 마저 한다).
        file.flush().await?;
        Ok(())
    }

    /// 녹화방송 목록. 최근에 등록한 순서로 돌려준다.
    pub async fn recordings(&self) -> Result<Vec<RecordingSummary>, ServiceError> {
        let rows = sqlx::query_as::<_, RecordingSummaryRow>(
            "SELECT recordings.id, recordings.title, channels.name AS channel_name,
                    recordings.registered_at_ms
             FROM recordings JOIN channels ON channels.id = recordings.channel_id
             ORDER BY recordings.registered_at_ms DESC, recordings.id DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| RecordingSummary {
                id: row.id,
                title: row.title,
                channel_name: row.channel_name,
                registered_at_ms: row.registered_at_ms,
            })
            .collect())
    }

    /// 녹화방송 상세: 제목, 채널, 영상 주소, 등록 시각. 없는 녹화방송이면 `None`.
    pub async fn recording_page(&self, id: i64) -> Result<Option<RecordingPage>, ServiceError> {
        let row = sqlx::query_as::<_, RecordingPageRow>(
            "SELECT recordings.id, recordings.title, channels.id AS channel_id,
                    channels.user_id AS owner_id, channels.name AS channel_name,
                    recordings.file_name, recordings.registered_at_ms
             FROM recordings JOIN channels ON channels.id = recordings.channel_id
             WHERE recordings.id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| RecordingPage {
            id: row.id,
            title: row.title,
            channel_id: row.channel_id,
            owner_id: row.owner_id,
            channel_name: row.channel_name,
            video_url: format!("{RECORDING_MEDIA_PATH}/{}", row.file_name),
            registered_at_ms: row.registered_at_ms,
            server_now_ms: self.now_ms(),
        }))
    }
}

impl AppService {
    /// 녹화방송 제목 바꾸기(주인만). 저장한 제목(앞뒤 공백을 지운 것)을 돌려준다.
    ///
    /// 거절: 없는 녹화방송이면 [`ServiceError::UnknownRecording`], 그 녹화방송 채널의 주인이 아니면
    /// [`ServiceError::NotBroadcastOwner`], 제목이 비었거나 100자를 넘으면 등록 때와 같은 사유.
    pub async fn rename_recording(
        &self,
        user_id: i64,
        recording_id: i64,
        title: &str,
    ) -> Result<String, ServiceError> {
        self.check_recording_owner(user_id, recording_id).await?;
        let title = valid_title(title)?.ok_or(ServiceError::RecordingTitleEmpty)?;
        sqlx::query("UPDATE recordings SET title = ? WHERE id = ?")
            .bind(title)
            .bind(recording_id)
            .execute(&self.pool)
            .await?;
        Ok(title.to_owned())
    }

    /// 녹화방송 삭제(주인만): 녹화방송, 그 댓글, 영상 파일을 지운다.
    ///
    /// 거절: 없는 녹화방송이면 [`ServiceError::UnknownRecording`], 주인이 아니면
    /// [`ServiceError::NotBroadcastOwner`]. 영상 파일을 지우지 못하면 녹화방송은 지우고 로그만 남긴다.
    pub async fn delete_recording(
        &self,
        user_id: i64,
        recording_id: i64,
    ) -> Result<(), ServiceError> {
        self.check_recording_owner(user_id, recording_id).await?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM comments WHERE recording_id = ?")
            .bind(recording_id)
            .execute(&mut *tx)
            .await?;
        let file_name = sqlx::query_scalar::<_, String>(
            "DELETE FROM recordings WHERE id = ? RETURNING file_name",
        )
        .bind(recording_id)
        .fetch_optional(&mut *tx)
        .await?
        // 확인과 삭제 사이에 다른 요청이 먼저 지웠다.
        .ok_or(ServiceError::UnknownRecording)?;
        tx.commit().await?;
        remove_quietly(&self.recordings_dir.join(file_name)).await;
        Ok(())
    }

    /// 그 사용자가 녹화방송 채널의 주인인지 확인한다. 녹화방송이 없으면 [`ServiceError::UnknownRecording`],
    /// 주인이 아니면 [`ServiceError::NotBroadcastOwner`].
    async fn check_recording_owner(
        &self,
        user_id: i64,
        recording_id: i64,
    ) -> Result<(), ServiceError> {
        let owner_id = sqlx::query_scalar::<_, i64>(
            "SELECT channels.user_id
             FROM recordings JOIN channels ON channels.id = recordings.channel_id
             WHERE recordings.id = ?",
        )
        .bind(recording_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(ServiceError::UnknownRecording)?;
        if owner_id == user_id {
            Ok(())
        } else {
            Err(ServiceError::NotBroadcastOwner)
        }
    }
}

/// 실패한 등록의 파일이나 지운 녹화방송의 파일을 지운다. 지우지 못해도 결과를 바꾸지 않고 로그만 남긴다.
async fn remove_quietly(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!("영상 파일을 지우지 못했습니다({}): {error}", path.display());
        }
    }
}
