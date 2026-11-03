//! SQLite 연결과 마이그레이션.

use std::{path::Path, str::FromStr, time::Duration};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

/// `packages/api/migrations`의 파일을 컴파일할 때 바이너리에 넣는다.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// 테스트용: 부를 때마다 서로 격리된 새 메모리 DB를 만들고 마이그레이션을 적용한다.
///
/// 메모리 DB는 연결이 모두 닫히면 사라지므로 연결 1개를 만료 없이 유지한다.
pub async fn connect_memory() -> Result<SqlitePool, sqlx::Error> {
    // "sqlite::memory:"는 파싱할 때마다 새 이름의 메모리 DB가 된다.
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

/// 실행용: 파일 DB에 연결하고(없으면 폴더와 파일을 만든다) 마이그레이션을 적용한다.
pub async fn connect_file(path: &Path) -> Result<SqlitePool, sqlx::Error> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}
