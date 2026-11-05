-- 로그인(과제 ③): 사용자 비밀번호와 로그인 세션 (ADR 0005)
-- 이전 단계에서 비밀번호 없이 만든 사용자는 NULL이며, 앱이 시작할 때 기본 비밀번호를 넣는다.
ALTER TABLE users ADD COLUMN password_hash TEXT;   -- Argon2id PHC 문자열

-- 로그인 세션 (Session): 로그인할 때 하나 생기고, 로그아웃하거나 기한이 지나면 사라진다.
CREATE TABLE sessions (
    token_hash    TEXT    PRIMARY KEY,                     -- 쿠키에 담는 토큰의 SHA-256(16진수). 토큰 자체는 저장하지 않는다
    user_id       INTEGER NOT NULL REFERENCES users(id),
    expires_at_ms INTEGER NOT NULL                         -- 유닉스 밀리초
);
CREATE INDEX sessions_user ON sessions (user_id);

-- 댓글 (Comment): 녹화방송 하나 또는 라이브 하나에 로그인한 사용자가 남긴 글. 정확히 하나에만 붙는다.
CREATE TABLE comments (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    recording_id  INTEGER REFERENCES recordings(id) ON DELETE CASCADE,
    live_id       INTEGER REFERENCES lives(id),
    author_id     INTEGER NOT NULL REFERENCES users(id),
    content       TEXT    NOT NULL,                        -- 앞뒤 공백을 지운 내용, 비어 있지 않다
    created_at_ms INTEGER NOT NULL,                        -- 유닉스 밀리초
    edited_at_ms  INTEGER,                                 -- 마지막으로 고친 시각, 고친 적 없으면 NULL
    CHECK ((recording_id IS NULL) <> (live_id IS NULL))
);
CREATE INDEX comments_recording ON comments (recording_id, id);
CREATE INDEX comments_live ON comments (live_id, id);
