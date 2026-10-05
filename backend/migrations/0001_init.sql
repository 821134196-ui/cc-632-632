-- 用户：老师 / 检录员 / 裁判 / 总裁判
CREATE TABLE users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT NOT NULL UNIQUE,
    display_name  TEXT NOT NULL,
    role          TEXT NOT NULL CHECK (role IN ('teacher','checker','referee','chief','admin')),
    password_hash TEXT NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE sessions (
    token      TEXT PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 学生
CREATE TABLE students (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    name   TEXT NOT NULL,
    grade  TEXT NOT NULL,           -- 年级，如 三年级
    class  TEXT NOT NULL,           -- 班级，如 2班
    UNIQUE (name, grade, class)
);

-- 运动项目（按组别区分）
CREATE TABLE events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL,    -- 100米 / 跳远
    grp           TEXT NOT NULL,    -- 组别：男子甲组 / 女子乙组 ...
    unit          TEXT NOT NULL DEFAULT '秒',
    higher_better INTEGER NOT NULL DEFAULT 0,  -- 0=越小越好(径赛) 1=越大越好(田赛)
    UNIQUE (name, grp)
);

-- 轮次：预赛 / 决赛
CREATE TABLE rounds (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id INTEGER NOT NULL REFERENCES events(id),
    label    TEXT NOT NULL,
    UNIQUE (event_id, label)
);

-- 报名：同一学生在同一轮次只能报名一次
CREATE TABLE registrations (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    round_id   INTEGER NOT NULL REFERENCES rounds(id),
    student_id INTEGER NOT NULL REFERENCES students(id),
    bib        TEXT NOT NULL,       -- 号码布
    created_by INTEGER REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (round_id, student_id)
);

-- 检录：同一报名（同一学生同一轮次）只有一条检录记录 => 重复检录被拒绝
-- status: present(到场) / absent(缺席)
-- makeup=1 表示缺席后补赛检录
CREATE TABLE checkins (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    registration_id INTEGER NOT NULL UNIQUE REFERENCES registrations(id),
    status          TEXT NOT NULL CHECK (status IN ('present','absent')),
    makeup          INTEGER NOT NULL DEFAULT 0,
    checked_by      INTEGER REFERENCES users(id),
    note            TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 成绩
-- 电子计时与手记分别保存为原始记录，永不覆盖（更正也不改这两列）
-- review_status:
--   raw         仅有原始计时，裁判尚未提交
--   agree       电子/手记一致，等待确认（或已由裁判确认）
--   pending     电子/手记不一致，等待总裁判复核
--   resolved    总裁判已复核认定最终成绩
--   confirmed   裁判已确认（单一来源或一致）
CREATE TABLE results (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    registration_id INTEGER NOT NULL UNIQUE REFERENCES registrations(id),
    electronic      REAL,                       -- 电子计时原始记录（秒/米）
    manual          REAL,                       -- 手记原始记录
    electronic_at   TEXT,
    manual_at       TEXT,
    final_value     REAL,                       -- 最终用于排名的成绩
    review_status   TEXT NOT NULL DEFAULT 'raw',
    late            INTEGER NOT NULL DEFAULT 0, -- 漏记补录
    note            TEXT NOT NULL DEFAULT '',
    entered_by      INTEGER REFERENCES users(id),
    confirmed_by    INTEGER REFERENCES users(id),
    confirmed_at    TEXT,
    resolved_by     INTEGER REFERENCES users(id),
    resolved_at     TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 成绩更正：必须填写原因，且由另一人审核
CREATE TABLE corrections (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    result_id        INTEGER NOT NULL REFERENCES results(id),
    old_value        REAL NOT NULL,
    new_value        REAL NOT NULL,
    reason           TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','approved','rejected')),
    requested_by     INTEGER NOT NULL REFERENCES users(id),
    reviewed_by      INTEGER REFERENCES users(id),
    review_note      TEXT NOT NULL DEFAULT '',
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    reviewed_at      TEXT
);

-- 榜单：每次发布生成新版本，旧版本永久保留
CREATE TABLE publications (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    round_id   INTEGER NOT NULL REFERENCES rounds(id),
    version    INTEGER NOT NULL,
    created_by INTEGER REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (round_id, version)
);

CREATE TABLE publication_entries (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    publication_id INTEGER NOT NULL REFERENCES publications(id),
    rank           INTEGER NOT NULL,
    student_name   TEXT NOT NULL,   -- 快照：历史榜单不受后续信息修改影响
    grade          TEXT NOT NULL,
    class          TEXT NOT NULL,
    bib            TEXT NOT NULL,
    score          REAL NOT NULL,
    makeup         INTEGER NOT NULL DEFAULT 0,
    tied           INTEGER NOT NULL DEFAULT 0  -- 与他人并列
);

CREATE INDEX idx_reg_student ON registrations(student_id);
CREATE INDEX idx_results_reg ON results(registration_id);
CREATE INDEX idx_checkins_status ON checkins(status);
