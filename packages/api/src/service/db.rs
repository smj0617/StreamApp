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
///
/// 여러 앱 인스턴스(과제 ⑤)가 공유 볼륨의 같은 DB 파일을 함께 쓸 수 있다. WAL 저널과 잠금 대기
/// (`busy_timeout`)로 동시 읽기·쓰기를 견딘다. 동시에 시작한 인스턴스끼리는 새 DB 파일을 WAL로 바꾸는
/// 첫 연결이나 마이그레이션이 겹쳐 잠김 오류가 날 수 있으므로, 실패하면 잠시 뒤 처음부터 다시 시도한다.
///
/// sqlx의 SQLite 마이그레이션은 잠금(`lock`)을 잡지 않는다. 대신 마이그레이션 하나를 트랜잭션 하나로
/// 적용하므로(스크립트와 `_sqlx_migrations` 기록을 함께 커밋), 두 인스턴스가 같은 마이그레이션을 동시에
/// 적용하면 한쪽만 커밋되고 다른 쪽은 오류(테이블이 이미 있음, 잠김 등)로 되돌려진다. 되돌려진 쪽이 다시
/// 시도하면 이미 적용된 마이그레이션은 건너뛰므로 같은 마이그레이션이 두 번 적용되지 않는다.
pub async fn connect_file(path: &Path) -> Result<SqlitePool, sqlx::Error> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut attempt = 1;
    loop {
        match open_and_migrate(path).await {
            Ok(pool) => return Ok(pool),
            Err(e) if attempt < CONNECT_ATTEMPTS => {
                tracing::warn!(
                    "DB 연결·마이그레이션 실패({attempt}/{CONNECT_ATTEMPTS}), 다른 인스턴스와 겹쳤을 수 있어 다시 시도합니다: {e}"
                );
                tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// [`connect_file`]이 다시 시도하는 횟수(처음 포함).
const CONNECT_ATTEMPTS: u32 = 8;

async fn open_and_migrate(path: &Path) -> Result<SqlitePool, sqlx::Error> {
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
