pub mod auth;
pub mod db;
pub mod error;
pub mod handlers;
pub mod ranking;
pub mod timer;

use axum::routing::{get, post};
use axum::Router;
use db::Db;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

pub fn app(pool: Db) -> Router {
    let api = Router::new()
        // 鉴权
        .route("/auth/login", post(handlers::login))
        .route("/auth/logout", post(handlers::logout))
        .route("/auth/me", get(handlers::me))
        // 基础数据
        .route("/students", get(handlers::list_students))
        .route("/events", get(handlers::list_events))
        // 报名
        .route("/registrations", post(handlers::create_registration))
        .route("/rounds/{id}/roster", get(handlers::round_roster))
        // 检录
        .route("/checkins", post(handlers::checkin))
        .route("/checkins/{id}/makeup", post(handlers::makeup_checkin))
        // 成绩
        .route("/results/electronic", post(handlers::enter_electronic))
        .route("/results/manual", post(handlers::enter_manual))
        .route("/results/{id}/confirm", post(handlers::confirm_result))
        .route("/results/{id}/resolve", post(handlers::resolve_dispute))
        // 更正
        .route("/results/{id}/corrections", post(handlers::request_correction))
        .route("/corrections", get(handlers::list_corrections))
        .route("/corrections/{id}/review", post(handlers::review_correction))
        // 排名 / 发布
        .route("/rounds/{id}/ranking", get(handlers::current_ranking))
        .route("/rounds/{id}/publish", post(handlers::publish))
        .route("/rounds/{id}/publications", get(handlers::list_publications))
        .route("/publications/{id}", get(handlers::publication_detail))
        // 工作台 / 公示屏
        .route("/workbench", get(handlers::workbench))
        .with_state(pool.clone());

    let public = Router::new()
        .route("/public/screen", get(handlers::public_screen))
        .with_state(pool);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let frontend = std::env::var("FRONTEND_DIR").unwrap_or_else(|_| "frontend/dist".to_string());
    // 静态资源 + SPA 回退到 index.html
    let index = ServeFile::new(format!("{frontend}/index.html"));
    let serve_dir = ServeDir::new(&frontend).fallback(index);

    Router::new()
        .nest("/api", api)
        .merge(public)
        .fallback_service(serve_dir)
        .layer(cors)
}
