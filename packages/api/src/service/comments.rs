//! 댓글(Comment) 동작: 방송(녹화방송·라이브)별 목록, 쓰기, 고치기·지우기(쓴 사람만).
//!
//! 목록은 누구나 볼 수 있다. 쓰기·고치기·지우기는 로그인한 사용자 ID를 받는다(로그인 확인은 부르는 쪽이
//! 세션으로 한다). 채팅과 달리 DB에 남는다.

use super::{AppService, ServiceError};
use crate::dto::{Comment, CommentList, CommentTarget};
use crate::rules::MAX_COMMENT_CHARS;

#[derive(sqlx::FromRow)]
struct CommentRow {
    id: i64,
    author_id: i64,
    author_nickname: String,
    content: String,
    created_at_ms: i64,
    edited_at_ms: Option<i64>,
}

impl From<CommentRow> for Comment {
    fn from(row: CommentRow) -> Self {
        Comment {
            id: row.id,
            author_id: row.author_id,
            author_nickname: row.author_nickname,
            content: row.content,
            created_at_ms: row.created_at_ms,
            edited: row.edited_at_ms.is_some(),
        }
    }
}

/// 댓글 하나를 읽는 SELECT(작성자 닉네임 포함). 뒤에 WHERE를 붙여 쓴다.
const SELECT_COMMENT: &str =
    "SELECT comments.id, comments.author_id, users.nickname AS author_nickname,
            comments.content, comments.created_at_ms, comments.edited_at_ms
     FROM comments JOIN users ON users.id = comments.author_id";

impl AppService {
    /// 방송 하나의 댓글 목록(오래된 순). 없는 녹화방송·라이브면 [`ServiceError::UnknownRecording`]·
    /// [`ServiceError::UnknownLive`].
    pub async fn comments(&self, target: CommentTarget) -> Result<CommentList, ServiceError> {
        self.check_target_exists(target).await?;
        let (column, id) = target_column(target);
        let rows = sqlx::query_as::<_, CommentRow>(&format!(
            "{SELECT_COMMENT} WHERE comments.{column} = ? ORDER BY comments.id"
        ))
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        Ok(CommentList {
            comments: rows.into_iter().map(Comment::from).collect(),
            server_now_ms: self.now_ms(),
        })
    }

    /// 댓글 쓰기: 그 사용자를 작성자로 방송에 댓글을 단다. 만든 댓글을 돌려준다.
    ///
    /// 거절: 내용이 비었으면 [`ServiceError::CommentEmpty`], 500자를 넘으면 [`ServiceError::CommentTooLong`],
    /// 없는 방송이면 [`ServiceError::UnknownRecording`]·[`ServiceError::UnknownLive`], 없는 사용자면
    /// [`ServiceError::UnknownUser`].
    pub async fn write_comment(
        &self,
        user_id: i64,
        target: CommentTarget,
        content: &str,
    ) -> Result<Comment, ServiceError> {
        let content = valid_comment(content)?;
        self.check_target_exists(target).await?;
        let (column, target_id) = target_column(target);
        let id = sqlx::query_scalar::<_, i64>(&format!(
            "INSERT INTO comments ({column}, author_id, content, created_at_ms) VALUES (?, ?, ?, ?)
             RETURNING id"
        ))
        .bind(target_id)
        .bind(user_id)
        .bind(content)
        .bind(self.now_ms())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| match error {
            // 방송은 위에서 확인했으므로 남은 외래 키는 작성자다.
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => ServiceError::UnknownUser,
            other => other.into(),
        })?;
        self.comment(id).await?.ok_or(ServiceError::UnknownComment)
    }

    /// 댓글 고치기(쓴 사람만). 고친 댓글을 돌려준다.
    ///
    /// 거절: 없는 댓글이면 [`ServiceError::UnknownComment`], 쓴 사람이 아니면
    /// [`ServiceError::NotCommentAuthor`], 내용 규칙은 쓰기와 같다.
    pub async fn edit_comment(
        &self,
        user_id: i64,
        comment_id: i64,
        content: &str,
    ) -> Result<Comment, ServiceError> {
        self.check_comment_author(user_id, comment_id).await?;
        let content = valid_comment(content)?;
        sqlx::query("UPDATE comments SET content = ?, edited_at_ms = ? WHERE id = ?")
            .bind(content)
            .bind(self.now_ms())
            .bind(comment_id)
            .execute(&self.pool)
            .await?;
        self.comment(comment_id)
            .await?
            .ok_or(ServiceError::UnknownComment)
    }

    /// 댓글 지우기(쓴 사람만). 없는 댓글이면 [`ServiceError::UnknownComment`], 쓴 사람이 아니면
    /// [`ServiceError::NotCommentAuthor`].
    pub async fn delete_comment(&self, user_id: i64, comment_id: i64) -> Result<(), ServiceError> {
        self.check_comment_author(user_id, comment_id).await?;
        sqlx::query("DELETE FROM comments WHERE id = ?")
            .bind(comment_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn comment(&self, comment_id: i64) -> Result<Option<Comment>, ServiceError> {
        let row =
            sqlx::query_as::<_, CommentRow>(&format!("{SELECT_COMMENT} WHERE comments.id = ?"))
                .bind(comment_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(Comment::from))
    }

    /// 그 사용자가 댓글을 쓴 사람인지 확인한다.
    async fn check_comment_author(
        &self,
        user_id: i64,
        comment_id: i64,
    ) -> Result<(), ServiceError> {
        let author_id = sqlx::query_scalar::<_, i64>("SELECT author_id FROM comments WHERE id = ?")
            .bind(comment_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(ServiceError::UnknownComment)?;
        if author_id == user_id {
            Ok(())
        } else {
            Err(ServiceError::NotCommentAuthor)
        }
    }

    async fn check_target_exists(&self, target: CommentTarget) -> Result<(), ServiceError> {
        let (table, id, missing) = match target {
            CommentTarget::Recording(id) => ("recordings", id, ServiceError::UnknownRecording),
            CommentTarget::Live(id) => ("lives", id, ServiceError::UnknownLive),
        };
        let found = sqlx::query_scalar::<_, i64>(&format!("SELECT id FROM {table} WHERE id = ?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        found.map(drop).ok_or(missing)
    }
}

/// 댓글이 붙는 열 이름과 방송 ID. 열 이름은 이 두 값뿐이라 SQL에 그대로 넣어도 된다.
fn target_column(target: CommentTarget) -> (&'static str, i64) {
    match target {
        CommentTarget::Recording(id) => ("recording_id", id),
        CommentTarget::Live(id) => ("live_id", id),
    }
}

/// 댓글 내용 규칙: 앞뒤 공백을 지운 뒤 1~[`MAX_COMMENT_CHARS`]자.
fn valid_comment(input: &str) -> Result<&str, ServiceError> {
    let content = input.trim();
    if content.is_empty() {
        return Err(ServiceError::CommentEmpty);
    }
    if content.chars().count() > MAX_COMMENT_CHARS {
        return Err(ServiceError::CommentTooLong);
    }
    Ok(content)
}
