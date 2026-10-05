-- 小学运动会检录与成绩更正系统 数据库结构
PRAGMA foreign_keys = ON;

-- 工作人员（老师/检录员/裁判/总裁判）
CREATE TABLE IF NOT EXISTS users (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    username    TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    role        TEXT NOT NULL CHECK (role IN ('teacher','clerk','referee','head_referee'))
);

-- 学生（以年级/班级标识）
CREATE TABLE IF NOT EXISTS students (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    grade       INTEGER NOT NULL,        -- 年级
    class       TEXT NOT NULL,           -- 班级，如 "1班"
    bib_number  TEXT NOT NULL UNIQUE,    -- 号码布
    UNIQUE (name, grade, class)
);

-- 比赛项目（名称 + 性别组/年级组 = 组别）
CREATE TABLE IF NOT EXISTS events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,           -- 如 60米、跳远
    group_name  TEXT NOT NULL,           -- 组别，如 "一年级男子组"
    unit        TEXT NOT NULL DEFAULT 'seconds', -- seconds / meters
    sort_rank   INTEGER NOT NULL DEFAULT 1,      -- 1=时间越短越好(径赛) -1=距离越大越好(田赛)
    UNIQUE (name, group_name)
);

-- 轮次（每个项目可有预赛/决赛等；同一学生在同一轮次只能检录一次）
CREATE TABLE IF NOT EXISTS rounds (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id    INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,           -- 预赛 / 决赛 / 补赛
    is_reshoot  INTEGER NOT NULL DEFAULT 0, -- 是否为补赛轮次
    UNIQUE (event_id, name)
);

-- 报名（老师录入）
CREATE TABLE IF NOT EXISTS entries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id    INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    student_id  INTEGER NOT NULL REFERENCES students(id) ON DELETE CASCADE,
    entered_by  INTEGER NOT NULL REFERENCES users(id),
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (event_id, student_id)
);

-- 检录记录：每学生每轮次至多一条（重复检录在应用层拒绝）
CREATE TABLE IF NOT EXISTS checkins (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_id    INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    round_id    INTEGER NOT NULL REFERENCES rounds(id) ON DELETE CASCADE,
    status      TEXT NOT NULL CHECK (status IN ('present','absent')),
    clerk_id    INTEGER NOT NULL REFERENCES users(id),
    note        TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (entry_id, round_id)
);

-- 原始计时/测量记录：电子计时与手记各一条，均为不可变证据
CREATE TABLE IF NOT EXISTS raw_results (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    checkin_id  INTEGER NOT NULL REFERENCES checkins(id) ON DELETE CASCADE,
    source      TEXT NOT NULL CHECK (source IN ('electronic','manual')),
    value       REAL NOT NULL,           -- 秒（径赛）或米（田赛）
    recorded_by INTEGER NOT NULL REFERENCES users(id),
    recorded_at TEXT NOT NULL DEFAULT (datetime('now')),
    note        TEXT NOT NULL DEFAULT '',
    UNIQUE (checkin_id, source)
);

-- 成绩（一条检录对应一条成绩）
-- confirmation: pending 待复核（电子/手记分歧 或 漏记补录 或 补赛）
--               confirmed 已确认可进入榜单
--               rejected 复核驳回（不参与排名）
CREATE TABLE IF NOT EXISTS results (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    checkin_id      INTEGER NOT NULL UNIQUE REFERENCES checkins(id) ON DELETE CASCADE,
    published_value REAL,                 -- 最终采用值，确认前为 NULL
    resolution      TEXT NOT NULL DEFAULT 'auto', -- auto/head/manual/electronic/corrected
    confirmation    TEXT NOT NULL DEFAULT 'pending',
    confirmed_by    INTEGER REFERENCES users(id),
    confirmed_at    TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 电子 vs 手记 分歧复核单
CREATE TABLE IF NOT EXISTS timing_disputes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    result_id       INTEGER NOT NULL UNIQUE REFERENCES results(id) ON DELETE CASCADE,
    electronic_val  REAL NOT NULL,
    manual_val      REAL NOT NULL,
    chosen_value    REAL,
    basis           TEXT NOT NULL DEFAULT '', -- 总裁判裁决依据
    decided_by      INTEGER REFERENCES users(id),
    decided_at      TEXT,
    status          TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','resolved'))
);

-- 成绩更正：必须说明原因，且由“另一人”审核
CREATE TABLE IF NOT EXISTS corrections (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    result_id       INTEGER NOT NULL REFERENCES results(id) ON DELETE CASCADE,
    old_value       REAL NOT NULL,
    new_value       REAL NOT NULL,
    reason          TEXT NOT NULL,
    requested_by    INTEGER NOT NULL REFERENCES users(id),
    requested_at    TEXT NOT NULL DEFAULT (datetime('now')),
    reviewed_by     INTEGER REFERENCES users(id),
    reviewed_at     TEXT,
    status          TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','approved','rejected'))
);

-- 榜单发布版本：每次发布生成新版本，旧版本永久保留
CREATE TABLE IF NOT EXISTS leaderboard_versions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    round_id    INTEGER NOT NULL REFERENCES rounds(id) ON DELETE CASCADE,
    version     INTEGER NOT NULL,
    published_by INTEGER NOT NULL REFERENCES users(id),
    published_at TEXT NOT NULL DEFAULT (datetime('now')),
    note        TEXT NOT NULL DEFAULT '',
    UNIQUE (round_id, version)
);

-- 榜单条目（快照，存并列名次）
CREATE TABLE IF NOT EXISTS leaderboard_rows (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    version_id  INTEGER NOT NULL REFERENCES leaderboard_versions(id) ON DELETE CASCADE,
    rank        INTEGER NOT NULL,         -- 并列名次（1,1,3 规则）
    order_index INTEGER NOT NULL,
    student_id  INTEGER NOT NULL REFERENCES students(id),
    result_id   INTEGER NOT NULL REFERENCES results(id),
    value       REAL NOT NULL
);

-- 审计日志
CREATE TABLE IF NOT EXISTS audit_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    actor_id    INTEGER REFERENCES users(id),
    action      TEXT NOT NULL,
    detail      TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_entries_event ON entries(event_id);
CREATE INDEX IF NOT EXISTS idx_checkins_round ON checkins(round_id);
CREATE INDEX IF NOT EXISTS idx_results_confirm ON results(confirmation);
