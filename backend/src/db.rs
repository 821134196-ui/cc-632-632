use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Row, Sqlite};
use std::str::FromStr;

pub type Db = Pool<Sqlite>;

/// 连接数据库并在需要时执行迁移与种子数据。
pub async fn init(database_url: &str) -> anyhow_lite::Result<Db> {
    let create_if_missing = !database_url.starts_with(":memory:");
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(create_if_missing)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    migrate(&pool).await?;
    seed(&pool).await?;
    Ok(pool)
}

/// 内存数据库（测试用）
pub async fn init_memory() -> Db {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    seed(&pool).await.unwrap();
    pool
}

async fn migrate(pool: &Db) -> anyhow_lite::Result<()> {
    sqlx::query(include_str!("../migrations/0001_init.sql"))
        .execute(pool)
        .await?;
    Ok(())
}

// ---- 轻量 anyhow 替代，避免引入额外依赖 ----
pub mod anyhow_lite {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
}

async fn seed(pool: &Db) -> anyhow_lite::Result<()> {
    let count: i64 = sqlx::query("SELECT COUNT(*) AS n FROM users")
        .fetch_one(pool)
        .await?
        .get("n");
    if count > 0 {
        return Ok(());
    }

    for (username, name, role) in [
        ("teacher", "王老师", "teacher"),
        ("checker", "李检录", "checker"),
        ("referee", "张裁判", "referee"),
        ("referee2", "刘裁判", "referee"),
        ("chief", "总裁判", "chief"),
    ] {
        sqlx::query(
            "INSERT INTO users (username, display_name, role, password_hash) VALUES (?, ?, ?, ?)",
        )
        .bind(username)
        .bind(name)
        .bind(role)
        .bind(crate::auth::hash_password("123456"))
        .execute(pool)
        .await?;
    }

    // 学生
    let students = [
        ("陈一", "三年级", "1班"),
        ("赵二", "三年级", "1班"),
        ("孙三", "三年级", "2班"),
        ("李四", "三年级", "2班"),
        ("周五", "三年级", "3班"),
        ("吴六", "三年级", "3班"),
        ("郑七", "四年级", "1班"),
        ("王八", "四年级", "2班"),
    ];
    for (name, grade, class) in students {
        sqlx::query("INSERT INTO students (name, grade, class) VALUES (?, ?, ?)")
            .bind(name)
            .bind(grade)
            .bind(class)
            .execute(pool)
            .await?;
    }

    // 项目与组别
    sqlx::query("INSERT INTO events (id, name, grp, unit, higher_better) VALUES (1, '100米', '三年级男子组', '秒', 0)").execute(pool).await?;
    sqlx::query("INSERT INTO events (id, name, grp, unit, higher_better) VALUES (2, '100米', '三年级女子组', '秒', 0)").execute(pool).await?;
    sqlx::query("INSERT INTO events (id, name, grp, unit, higher_better) VALUES (3, '跳远', '三年级男子组', '米', 1)").execute(pool).await?;

    // 轮次
    sqlx::query("INSERT INTO rounds (id, event_id, label) VALUES (1, 1, '预赛')").execute(pool).await?;
    sqlx::query("INSERT INTO rounds (id, event_id, label) VALUES (2, 1, '决赛')").execute(pool).await?;
    sqlx::query("INSERT INTO rounds (id, event_id, label) VALUES (3, 3, '预赛')").execute(pool).await?;

    // 报名：100米男子预赛 4 人
    let regs = [(1i64, 1i64, "0301"), (1, 2, "0302"), (1, 3, "0303"), (1, 4, "0304")];
    for (round_id, student_id, bib) in regs {
        sqlx::query(
            "INSERT INTO registrations (round_id, student_id, bib, created_by) VALUES (?, ?, ?, 1)",
        )
        .bind(round_id)
        .bind(student_id)
        .bind(bib)
        .execute(pool)
        .await?;
    }
    // 跳远男子预赛 2 人
    for (student_id, bib) in [(1i64, "0301"), (5i64, "0305")] {
        sqlx::query(
            "INSERT INTO registrations (round_id, student_id, bib, created_by) VALUES (3, ?, ?, 1)",
        )
        .bind(student_id)
        .bind(bib)
        .execute(pool)
        .await?;
    }

    Ok(())
}
