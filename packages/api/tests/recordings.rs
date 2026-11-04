//! 경계 A: 앱 서비스의 녹화방송. 테스트마다 새 메모리 SQLite와 임시 녹화방송 폴더를 쓰고,
//! 서비스의 공개 동작으로만 확인한다.

mod common;

use api::dto::{RecordingPage, RecordingSummary};
use api::service::{db, AppService, ServiceError};
use bytes::Bytes;
use common::{service_with_recordings, user, TempDir};
use futures_util::stream;

/// 영상 파일 내용을 조각 하나로 보내는 흐름.
fn body(content: &'static [u8]) -> impl futures_util::Stream<Item = Result<Bytes, String>> + Unpin {
    stream::iter(vec![Ok(Bytes::from_static(content))])
}

/// 녹화방송 폴더에 있는 파일 이름(정렬). 폴더가 아직 없으면 빈 목록.
fn stored_files(svc: &AppService) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(svc.recordings_dir())
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[tokio::test]
async fn registered_recording_is_saved_and_shown_in_list_and_page() {
    let (svc, clock, _dir) = service_with_recordings().await;
    let (user_id, channel_id) = user(&svc, "철수").await;
    clock.set(5_000);

    let id = svc
        .register_recording(user_id, "첫 녹화", "clip.MP4", body(b"fake mp4 bytes"))
        .await
        .unwrap();

    let files = stored_files(&svc);
    assert_eq!(files.len(), 1, "files: {files:?}");
    let file_name = &files[0];
    assert!(
        file_name.ends_with(".mp4"),
        "확장자는 소문자로 저장한다: {file_name}"
    );
    assert_eq!(
        std::fs::read(svc.recordings_dir().join(file_name)).unwrap(),
        b"fake mp4 bytes"
    );
    assert_eq!(
        svc.recordings().await.unwrap(),
        [RecordingSummary {
            id,
            title: "첫 녹화".to_string(),
            channel_name: "철수".to_string(),
            registered_at_ms: 5_000,
        }]
    );
    clock.set(9_000);
    assert_eq!(
        svc.recording_page(id).await.unwrap(),
        Some(RecordingPage {
            id,
            title: "첫 녹화".to_string(),
            channel_id,
            channel_name: "철수".to_string(),
            video_url: format!("/media/recordings/{file_name}"),
            registered_at_ms: 5_000,
            server_now_ms: 9_000,
        })
    );
}

#[tokio::test]
async fn recording_is_split_into_chunks_and_joined_in_order() {
    let (svc, _, _dir) = service_with_recordings().await;
    let (user_id, _) = user(&svc, "철수").await;
    let chunks = stream::iter(vec![
        Ok::<_, String>(Bytes::from_static(b"abc")),
        Ok(Bytes::from_static(b"def")),
    ]);

    svc.register_recording(user_id, "조각", "clip.webm", chunks)
        .await
        .unwrap();

    let files = stored_files(&svc);
    assert!(files[0].ends_with(".webm"));
    assert_eq!(
        std::fs::read(svc.recordings_dir().join(&files[0])).unwrap(),
        b"abcdef"
    );
}

#[tokio::test]
async fn title_is_trimmed_before_saving() {
    let (svc, _, _dir) = service_with_recordings().await;
    let (user_id, _) = user(&svc, "철수").await;

    let id = svc
        .register_recording(user_id, "  공백 제목 \t", "clip.mp4", body(b"x"))
        .await
        .unwrap();

    let page = svc.recording_page(id).await.unwrap().unwrap();
    assert_eq!(page.title, "공백 제목");
}

#[tokio::test]
async fn list_shows_most_recently_registered_first() {
    let (svc, clock, _dir) = service_with_recordings().await;
    let (chulsoo, _) = user(&svc, "철수").await;
    let (younghee, _) = user(&svc, "영희").await;
    clock.set(1_000);
    svc.register_recording(chulsoo, "먼저", "a.mp4", body(b"a"))
        .await
        .unwrap();
    clock.set(2_000);
    svc.register_recording(younghee, "나중", "b.mp4", body(b"b"))
        .await
        .unwrap();

    let titles: Vec<String> = svc
        .recordings()
        .await
        .unwrap()
        .into_iter()
        .map(|r| format!("{} - {}", r.channel_name, r.title))
        .collect();

    assert_eq!(titles, ["영희 - 나중", "철수 - 먼저"]);
}

#[tokio::test]
async fn unknown_recording_has_no_page() {
    let (svc, _, _dir) = service_with_recordings().await;

    assert_eq!(svc.recording_page(42).await.unwrap(), None);
}

// ---- 거절: 어느 경우든 녹화방송도 파일도 남지 않는다 ----

async fn assert_nothing_registered(svc: &AppService) {
    assert!(svc.recordings().await.unwrap().is_empty());
    assert!(
        stored_files(svc).is_empty(),
        "남은 파일: {:?}",
        stored_files(svc)
    );
}

#[tokio::test]
async fn empty_title_is_rejected() {
    let (svc, _, _dir) = service_with_recordings().await;
    let (user_id, _) = user(&svc, "철수").await;

    let result = svc
        .register_recording(user_id, "   ", "clip.mp4", body(b"x"))
        .await;

    assert!(matches!(result, Err(ServiceError::RecordingTitleEmpty)));
    assert_nothing_registered(&svc).await;
}

#[tokio::test]
async fn title_longer_than_100_chars_is_rejected() {
    let (svc, _, _dir) = service_with_recordings().await;
    let (user_id, _) = user(&svc, "철수").await;

    let result = svc
        .register_recording(user_id, &"가".repeat(101), "clip.mp4", body(b"x"))
        .await;

    assert!(matches!(result, Err(ServiceError::TitleTooLong)));
    assert_nothing_registered(&svc).await;
}

#[tokio::test]
async fn title_of_exactly_100_chars_is_accepted() {
    let (svc, _, _dir) = service_with_recordings().await;
    let (user_id, _) = user(&svc, "철수").await;

    let result = svc
        .register_recording(user_id, &"가".repeat(100), "clip.mp4", body(b"x"))
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn file_that_is_not_mp4_or_webm_is_rejected() {
    let (svc, _, _dir) = service_with_recordings().await;
    let (user_id, _) = user(&svc, "철수").await;

    for file_name in ["clip.avi", "clip", "mp4", "clip.mp4.exe", ""] {
        let result = svc
            .register_recording(user_id, "제목", file_name, body(b"x"))
            .await;

        assert!(
            matches!(result, Err(ServiceError::RecordingFormat)),
            "{file_name:?}: {result:?}"
        );
    }
    assert_nothing_registered(&svc).await;
}

#[tokio::test]
async fn unknown_user_is_rejected() {
    let (svc, _, _dir) = service_with_recordings().await;

    let result = svc
        .register_recording(999, "제목", "clip.mp4", body(b"x"))
        .await;

    assert!(matches!(result, Err(ServiceError::UnknownUser)));
    assert_nothing_registered(&svc).await;
}

#[tokio::test]
async fn empty_file_is_rejected() {
    let (svc, _, _dir) = service_with_recordings().await;
    let (user_id, _) = user(&svc, "철수").await;

    let result = svc
        .register_recording(user_id, "제목", "clip.mp4", body(b""))
        .await;

    assert!(matches!(result, Err(ServiceError::RecordingFileEmpty)));
    assert_nothing_registered(&svc).await;
}

#[tokio::test]
async fn file_larger_than_limit_is_rejected() {
    let (svc, _, _dir) = service_with_recordings().await;
    let svc = svc.with_max_recording_bytes(5);
    let (user_id, _) = user(&svc, "철수").await;
    let chunks = stream::iter(vec![
        Ok::<_, String>(Bytes::from_static(b"abc")),
        Ok(Bytes::from_static(b"def")),
    ]);

    let result = svc
        .register_recording(user_id, "제목", "clip.mp4", chunks)
        .await;

    assert!(matches!(result, Err(ServiceError::RecordingTooLarge)));
    assert_nothing_registered(&svc).await;
}

#[tokio::test]
async fn file_of_exactly_the_limit_is_accepted() {
    let (svc, _, _dir) = service_with_recordings().await;
    let svc = svc.with_max_recording_bytes(6);
    let (user_id, _) = user(&svc, "철수").await;

    let result = svc
        .register_recording(user_id, "제목", "clip.mp4", body(b"abcdef"))
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn upload_that_breaks_off_is_rejected() {
    let (svc, _, _dir) = service_with_recordings().await;
    let (user_id, _) = user(&svc, "철수").await;
    let chunks = stream::iter(vec![
        Ok(Bytes::from_static(b"abc")),
        Err("연결 끊김".to_string()),
    ]);

    let result = svc
        .register_recording(user_id, "제목", "clip.mp4", chunks)
        .await;

    assert!(matches!(result, Err(ServiceError::UploadInterrupted)));
    assert_nothing_registered(&svc).await;
}

// ---- 재시작 ----

#[tokio::test]
async fn recording_survives_restart_with_same_db_file_and_folder() {
    let dir = TempDir::new();
    let db_path = dir.path().join("streamapp.db");
    let recordings_dir = dir.path().join("recordings");
    let id = {
        let svc = AppService::new(db::connect_file(&db_path).await.unwrap())
            .with_recordings_dir(&recordings_dir);
        let (user_id, _) = user(&svc, "철수").await;
        let id = svc
            .register_recording(user_id, "남는 녹화", "clip.mp4", body(b"keep"))
            .await
            .unwrap();
        id
    };

    // 같은 DB 파일과 폴더로 새로 시작한다.
    let svc = AppService::new(db::connect_file(&db_path).await.unwrap())
        .with_recordings_dir(&recordings_dir);

    let page = svc
        .recording_page(id)
        .await
        .unwrap()
        .expect("재시작 뒤에도 있다");
    assert_eq!(page.title, "남는 녹화");
    let file_name = page.video_url.rsplit('/').next().unwrap();
    assert_eq!(
        std::fs::read(recordings_dir.join(file_name)).unwrap(),
        b"keep"
    );
}
