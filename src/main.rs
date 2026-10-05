mod auth;
mod db;
mod error;
mod handlers;
mod ranking;

use axum::response::IntoResponse;
use axum::routing::get;
use axum::routing::post;
use axum::Router;
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
}

fn api_router() -> Router<AppState> {
    Router::new()
        // 基础数据
        .route("/api/users", get(handlers::list_users))
        .route("/api/students", get(handlers::list_students).post(handlers::create_student))
        .route("/api/events", get(handlers::list_events).post(handlers::create_event))
        // 报名
        .route("/api/entries", get(handlers::list_entries).post(handlers::create_entry))
        // 轮次
        .route("/api/rounds", get(handlers::list_rounds).post(handlers::create_round))
        .route("/api/rounds/:round_id/roster", get(handlers::round_roster))
        // 检录
        .route("/api/checkins", post(handlers::create_checkin))
        // 裁判：计时（电子/手记、模拟计时器、漏记补录）
        .route("/api/timing", post(handlers::record_timing))
        .route("/api/timer/simulate", post(handlers::simulate_timer))
        .route("/api/results/missing", post(handlers::record_missing))
        // 总裁判：复核队列、裁决、确认/驳回
        .route("/api/review/queue", get(handlers::review_queue))
        .route("/api/review/disputes/:dispute_id", post(handlers::decide_dispute))
        .route("/api/review/results/:result_id/confirm", post(handlers::confirm_result))
        .route("/api/review/results/:result_id/reject", post(handlers::reject_result))
        // 成绩更正（双人审核）
        .route("/api/corrections", get(handlers::list_corrections).post(handlers::request_correction))
        .route("/api/corrections/:id/review", post(handlers::review_correction))
        // 榜单：发布 / 最新 / 历史版本 / 版本详情
        .route("/api/rounds/:round_id/leaderboard/publish", post(handlers::publish_leaderboard))
        .route("/api/rounds/:round_id/leaderboard/latest", get(handlers::round_leaderboard_latest))
        .route("/api/rounds/:round_id/leaderboard/versions", get(handlers::round_leaderboard_versions))
        .route("/api/leaderboard/:version_id", get(handlers::get_leaderboard))
        // 成绩溯源（原始计时记录 + 更正历史）
        .route("/api/results/:result_id/trace", get(handlers::result_trace))
        // 工作台 / 公示屏 / 审计
        .route("/api/workbench", get(handlers::workbench))
        .route("/api/public-board", get(handlers::public_board))
        .route("/api/audit", get(handlers::audit_list))
        // 演示数据重置
        .route("/api/admin/reset", post(handlers::reset_db))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=warn,sqlx=warn".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:data/meet.db?mode=rwc".into());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    // 确保数据目录存在（sqlite:data/meet.db 是相对路径时）
    if let Some(parent) = db_url.strip_prefix("sqlite:").and_then(|p| {
        let path = p.split('?').next().unwrap_or(p);
        std::path::Path::new(path).parent()
    }) {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).ok();
        }
    }

    let pool = db::connect(&db_url).await?;
    db::init(&pool).await?;

    let state = AppState { db: pool };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let static_dir = PathBuf::from(
        std::env::var("STATIC_DIR").unwrap_or_else(|_| "frontend/dist".into()),
    );
    tracing::info!("静态资源目录: {}", static_dir.display());

    let serve = ServeDir::new(&static_dir)
        .fallback(get(spa_index))
        .precompressed_gzip();

    let app = Router::new()
        .merge(api_router())
        .fallback_service(serve)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("运动会检录与成绩系统已启动: http://localhost:{port}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// SPA 回退：非 /api 路径一律返回 index.html
async fn spa_index() -> axum::response::Response {
    let static_dir = PathBuf::from(
        std::env::var("STATIC_DIR").unwrap_or_else(|_| "frontend/dist".into()),
    );
    match tokio::fs::read(static_dir.join("index.html")).await {
        Ok(bytes) => (
            axum::http::StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
            bytes,
        )
            .into_response(),
        Err(_) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "前端尚未构建，请先运行 ./start.sh（会自动构建 frontend/dist）",
        )
            .into_response(),
    }
}
