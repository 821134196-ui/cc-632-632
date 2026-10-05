use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};
use std::str::FromStr;

const SCHEMA: &str = include_str!("../schema.sql");

pub async fn connect(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .foreign_keys(true)
        .create_if_missing(true);
    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
}

pub async fn init(db: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(SCHEMA).execute(db).await?;
    seed(db).await?;
    Ok(())
}

pub async fn reset(db: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut conn = db.acquire().await?;
    use sqlx::Executor;
    conn.execute("PRAGMA foreign_keys = OFF").await?;
    let tables: Vec<String> = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| r.get::<String, _>(0))
    .collect();
    for t in tables {
        conn.execute(format!("DROP TABLE IF EXISTS {t}").as_str()).await?;
    }
    conn.execute("PRAGMA foreign_keys = ON").await?;
    drop(conn);
    init(db).await
}

/// 初始演示数据（显式 ID，保证全新库与重置后一致）
async fn seed(db: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;

    for (id, username, name, role) in [
        (1_i64, "teacher", "王老师", "teacher"),
        (2, "clerk", "李检录", "clerk"),
        (3, "referee", "赵裁判", "referee"),
        (4, "head", "孙总裁", "head_referee"),
    ] {
        sqlx::query(
            "INSERT OR IGNORE INTO users(id, username, display_name, role) VALUES (?,?,?,?)",
        )
        .bind(id)
        .bind(username)
        .bind(name)
        .bind(role)
        .execute(&mut *tx)
        .await?;
    }

    // (id, 姓名, 年级, 班级, 号码布)
    let students: &[(i64, &str, i64, &str, &str)] = &[
        (1001, "陈一", 1, "1班", "A1001"),
        (1002, "林二", 1, "1班", "A1002"),
        (1003, "黄三", 1, "1班", "A1003"),
        (1004, "周武", 1, "2班", "A1004"),
        (1005, "吴天", 1, "2班", "A1005"),
        (2001, "赵雪", 2, "1班", "B2001"),
        (2002, "钱朵朵", 2, "1班", "B2002"),
        (2003, "孙兰", 2, "2班", "B2003"),
        (2004, "李梅", 2, "2班", "B2004"),
        (3001, "郑强", 3, "1班", "C3001"),
        (3002, "王磊", 3, "1班", "C3002"),
        (3003, "冯涛", 3, "2班", "C3003"),
        (3004, "褚明", 3, "2班", "C3004"),
    ];
    for (id, name, grade, class, bib) in students {
        sqlx::query(
            "INSERT OR IGNORE INTO students(id, name, grade, class, bib_number) VALUES (?,?,?,?,?)",
        )
        .bind(id)
        .bind(name)
        .bind(grade)
        .bind(class)
        .bind(bib)
        .execute(&mut *tx)
        .await?;
    }

    // (id, 项目, 组别, 单位, 方向)  sort_rank=1 越小越好(径赛)，-1 越大越好(田赛)
    let events: &[(i64, &str, &str, &str, i64)] = &[
        (1, "60米", "一年级男子组", "seconds", 1),
        (2, "100米", "二年级女子组", "seconds", 1),
        (3, "跳远", "三年级男子组", "meters", -1),
    ];
    for (id, name, group, unit, dir) in events {
        sqlx::query(
            "INSERT OR IGNORE INTO events(id, name, group_name, unit, sort_rank) VALUES (?,?,?,?,?)",
        )
        .bind(id).bind(name).bind(group).bind(unit).bind(dir)
        .execute(&mut *tx).await?;
    }

    // (id, event, 轮次名, 是否补赛)
    for (id, event_id, name, reshoot) in [
        (1_i64, 1_i64, "决赛", 0_i64),
        (2, 2, "决赛", 0),
        (3, 3, "决赛", 0),
    ] {
        sqlx::query("INSERT OR IGNORE INTO rounds(id, event_id, name, is_reshoot) VALUES (?,?,?,?)")
            .bind(id).bind(event_id).bind(name).bind(reshoot)
            .execute(&mut *tx).await?;
    }

    // 报名：项目1 → 1001..1005，项目2 → 2001..2004，项目3 → 3001..3004
    let mut entry_id = 1_i64;
    let event_students: &[(i64, &[i64])] = &[
        (1, &[1001, 1002, 1003, 1004, 1005]),
        (2, &[2001, 2002, 2003, 2004]),
        (3, &[3001, 3002, 3003, 3004]),
    ];
    for (event_id, sids) in event_students {
        for sid in *sids {
            sqlx::query(
                "INSERT OR IGNORE INTO entries(id, event_id, student_id, entered_by) VALUES (?,?,?,1)",
            )
            .bind(entry_id).bind(event_id).bind(sid)
            .execute(&mut *tx).await?;
            entry_id += 1;
        }
    }

    tx.commit().await?;
    Ok(())
}
