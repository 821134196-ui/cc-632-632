use meet_server::app;
use meet_server::db;
use sqlx::SqlitePool;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/meet.db?mode=rwc".into());

    // 确保数据目录存在
    if let Some(path) = database_url.strip_prefix("sqlite://") {
        if let Some(dir) = std::path::Path::new(path.split('?').next().unwrap_or(path)).parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
    }

    let pool: SqlitePool = db::init(&database_url).await?;

    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;

    let app = app(pool).layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("🏃 运动会检录与成绩系统已启动: http://{addr}");
    println!("   工作台: http://{addr}/  |  公示屏: http://{addr}/#/screen");
    axum::serve(listener, app).await?;
    Ok(())
}
