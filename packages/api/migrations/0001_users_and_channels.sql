-- 사용자 (User)
CREATE TABLE users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    nickname      TEXT    NOT NULL,            -- 입력한 그대로(앞뒤 공백만 지움)
    nickname_key  TEXT    NOT NULL UNIQUE,     -- 영문 대소문자를 무시한 중복 판단용 키(소문자)
    created_at_ms INTEGER NOT NULL             -- 유닉스 밀리초
);

-- 채널 (Channel): 사용자와 1:1
CREATE TABLE channels (
    id              TEXT    PRIMARY KEY,                          -- 무작위, URL에 넣을 수 있는 문자
    user_id         INTEGER NOT NULL UNIQUE REFERENCES users(id),
    name            TEXT    NOT NULL,                             -- 만들 때의 닉네임
    stream_key      TEXT    NOT NULL UNIQUE,                      -- 송출 키(StreamKey): 충분히 긴 무작위 값
    next_live_title TEXT                                          -- 다음 라이브 제목(NextLiveTitle), 없을 수 있음
);
