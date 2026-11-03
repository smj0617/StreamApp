fn main() {
    // `sqlx::migrate!`는 컴파일할 때 마이그레이션 파일을 바이너리에 넣는다.
    // 새 파일을 추가해도 Rust 소스가 그대로면 다시 컴파일되지 않으므로 폴더를 지켜본다.
    println!("cargo:rerun-if-changed=migrations");
}
