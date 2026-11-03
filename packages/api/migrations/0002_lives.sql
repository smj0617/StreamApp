-- 라이브 (Live): 채널에서 송출이 시작될 때 열리고 송출이 끊길 때 닫히는 한 회차
CREATE TABLE lives (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id    TEXT    NOT NULL REFERENCES channels(id),
    title         TEXT    NOT NULL,
    started_at_ms INTEGER NOT NULL,              -- 유닉스 밀리초
    ended_at_ms   INTEGER                        -- 유닉스 밀리초, 열려 있으면 NULL
);

-- 채널마다 열린(종료 시각이 없는) 라이브는 하나만 허용한다.
CREATE UNIQUE INDEX lives_one_open_per_channel ON lives (channel_id) WHERE ended_at_ms IS NULL;
