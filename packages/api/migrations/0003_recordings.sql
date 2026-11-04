-- 녹화방송 (Recording): 채널에 올려 둔 녹화 영상 한 편. 영상 파일은 녹화방송 폴더에 있다(ADR 0003).
CREATE TABLE recordings (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id       TEXT    NOT NULL REFERENCES channels(id),
    title            TEXT    NOT NULL,                -- 앞뒤 공백을 지운 제목, 비어 있지 않다
    file_name        TEXT    NOT NULL UNIQUE,         -- 영상 위치: 녹화방송 폴더 안의 파일 이름(무작위 이름.확장자)
    registered_at_ms INTEGER NOT NULL                 -- 유닉스 밀리초
);
