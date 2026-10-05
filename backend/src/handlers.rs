use crate::auth::{new_token, verify_password, User};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::ranking::{rank, RankItem};
use crate::timer::ElectronicTimer;
use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Column, Row, Sqlite};

fn now_sql() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_secs();
    // SQLite datetime('now') 用 UTC，这里同样给出 UTC
    let (y, mo, d, h, mi, s) = ymdhms_from_unix(secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

fn ymdhms_from_unix(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let h = (rem / 3600) as u32;
    let mi = (rem % 3600 / 60) as u32;
    let s = (rem % 60) as u32;
    let (y, mo, d) = civil_from_days(days);
    (y, mo, d, h, mi, s)
}

// Howard Hinnant 公历换算
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

// ---------------- 鉴权 ----------------

#[derive(Deserialize)]
pub struct LoginReq {
    username: String,
    password: String,
}

pub async fn login(
    State(db): State<Db>,
    Json(req): Json<LoginReq>,
) -> AppResult<Json<Value>> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
        .bind(&req.username)
        .fetch_optional(&db)
        .await?
        .ok_or_else(|| AppError::bad_request("用户名或密码错误"))?;

    if !verify_password(&req.password, &user.password_hash) {
        return Err(AppError::bad_request("用户名或密码错误"));
    }

    let token = new_token();
    sqlx::query("INSERT INTO sessions (token, user_id) VALUES (?, ?)")
        .bind(&token)
        .bind(user.id)
        .execute(&db)
        .await?;

    Ok(Json(json!({ "token": token, "user": user })))
}

pub async fn logout(
    State(db): State<Db>,
    user: User,
) -> AppResult<Json<Value>> {
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user.id)
        .execute(&db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn me(user: User) -> Json<Value> {
    Json(json!({ "user": user }))
}

// ---------------- 基础数据 ----------------

pub async fn list_students(State(db): State<Db>, _user: User) -> AppResult<Json<Value>> {
    let rows = sqlx::query("SELECT * FROM students ORDER BY grade, class, id")
        .fetch_all(&db)
        .await?;
    Ok(Json(rows_json(rows)))
}

pub async fn list_events(State(db): State<Db>, _user: User) -> AppResult<Json<Value>> {
    let events = sqlx::query("SELECT * FROM events ORDER BY id").fetch_all(&db).await?;
    let rounds = sqlx::query("SELECT * FROM rounds ORDER BY id").fetch_all(&db).await?;
    Ok(Json(json!({
        "events": rows_json(events),
        "rounds": rows_json(rounds),
    })))
}

/// 通用 SqliteRow 数组 -> JSON 数组
fn rows_json(rows: Vec<sqlx::sqlite::SqliteRow>) -> Value {
    Value::Array(rows.iter().map(row_to_json).collect())
}

/// 通用 SqliteRow -> JSON（本项目列均为整数/实数/文本/空值）
fn row_to_json(row: &sqlx::sqlite::SqliteRow) -> Value {
    use sqlx::TypeInfo;
    let mut obj = serde_json::Map::new();
    for (idx, col) in row.columns().iter().enumerate() {
        let name = col.name().to_string();
        let v: Value = match col.type_info().name() {
            "BOOLEAN" | "INTEGER" => row
                .try_get::<Option<i64>, _>(idx)
                .ok()
                .flatten()
                .map(Value::from)
                .unwrap_or(Value::Null),
            "REAL" => row
                .try_get::<Option<f64>, _>(idx)
                .ok()
                .flatten()
                .map(Value::from)
                .unwrap_or(Value::Null),
            _ => row
                .try_get::<Option<String>, _>(idx)
                .ok()
                .flatten()
                .map(Value::from)
                .unwrap_or(Value::Null),
        };
        obj.insert(name, v);
    }
    Value::Object(obj)
}

// ---------------- 报名 ----------------

#[derive(Deserialize)]
pub struct RegisterReq {
    round_id: i64,
    student_id: i64,
    bib: String,
}

pub async fn create_registration(
    State(db): State<Db>,
    user: User,
    Json(req): Json<RegisterReq>,
) -> AppResult<Json<Value>> {
    user.require_role(&["teacher", "admin"])?;
    let res = sqlx::query(
        "INSERT INTO registrations (round_id, student_id, bib, created_by) VALUES (?, ?, ?, ?)",
    )
    .bind(req.round_id)
    .bind(req.student_id)
    .bind(&req.bib)
    .bind(user.id)
    .execute(&db)
    .await;
    match res {
        Ok(r) => Ok(Json(json!({ "id": r.last_insert_rowid() }))),
        Err(e) => Err(e.into()), // UNIQUE(round_id, student_id) => 409
    }
}

/// 某轮次的报名名单（含检录与成绩状态）
pub async fn round_roster(
    State(db): State<Db>,
    _user: User,
    Path(round_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let rows = sqlx::query(
        r#"
        SELECT r.id AS registration_id, r.bib,
               s.id AS student_id, s.name AS student_name, s.grade, s.class,
               c.id AS checkin_id, c.status AS checkin_status, c.makeup AS checkin_makeup, c.note AS checkin_note,
               c.checked_by AS checked_by,
               res.id AS result_id, res.electronic, res.manual, res.final_value,
               res.review_status, res.late, res.note AS result_note
        FROM registrations r
        JOIN students s ON s.id = r.student_id
        LEFT JOIN checkins c ON c.registration_id = r.id
        LEFT JOIN results res ON res.registration_id = r.id
        WHERE r.round_id = ?
        ORDER BY r.bib
        "#,
    )
    .bind(round_id)
    .fetch_all(&db)
    .await?;
    Ok(Json(json!({ "entries": rows_json(rows) })))
}

// ---------------- 检录 ----------------

#[derive(Deserialize)]
pub struct CheckinReq {
    registration_id: i64,
    status: String, // present / absent
    #[serde(default)]
    note: String,
}

pub async fn checkin(
    State(db): State<Db>,
    user: User,
    Json(req): Json<CheckinReq>,
) -> AppResult<Json<Value>> {
    user.require_role(&["checker", "admin"])?;
    if !["present", "absent"].contains(&req.status.as_str()) {
        return Err(AppError::bad_request("检录状态只能是 present 或 absent"));
    }
    // 同一学生同一轮次仅一条检录记录：UNIQUE 约束兜底 => 重复检录 409
    let res = sqlx::query(
        "INSERT INTO checkins (registration_id, status, makeup, checked_by, note)
         VALUES (?, ?, 0, ?, ?)",
    )
    .bind(req.registration_id)
    .bind(&req.status)
    .bind(user.id)
    .bind(&req.note)
    .execute(&db)
    .await;

    match res {
        Ok(r) => Ok(Json(json!({ "id": r.last_insert_rowid(), "status": req.status }))),
        Err(e) => Err(map_checkin_conflict(e)),
    }
}

/// 补赛检录：缺席者经批准后转为到场（仍只有一条检录记录，不算重复检录）
pub async fn makeup_checkin(
    State(db): State<Db>,
    user: User,
    Path(checkin_id): Path<i64>,
) -> AppResult<Json<Value>> {
    user.require_role(&["checker", "admin"])?;
    let res = sqlx::query(
        "UPDATE checkins SET status = 'present', makeup = 1, updated_at = ?
         WHERE id = ? AND status = 'absent'",
    )
    .bind(now_sql())
    .bind(checkin_id)
    .execute(&db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::bad_request(
            "只有缺席状态的检录记录可以办理补赛，或记录不存在",
        ));
    }
    Ok(Json(json!({ "ok": true, "makeup": true })))
}

fn map_checkin_conflict(e: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db) = &e {
        if db.is_unique_violation() {
            return AppError::conflict("该学生在本轮次已经完成检录，不能重复检录");
        }
    }
    e.into()
}

// ---------------- 成绩：电子计时 / 手记 ----------------


#[derive(Deserialize)]
pub struct ElectronicReq {
    registration_id: i64,
    #[serde(default)]
    late: bool,
    #[serde(default)]
    note: String,
}

/// 读取本地模拟电子计时器
pub async fn enter_electronic(
    State(db): State<Db>,
    user: User,
    Json(req): Json<ElectronicReq>,
) -> AppResult<Json<Value>> {
    user.require_role(&["referee", "chief", "admin"])?;

    let higher_better = event_direction(&db, req.registration_id).await?;
    let value = ElectronicTimer::read(higher_better);

    let mut tx = db.begin().await?;
    // 加锁式读取（SQLite 串行事务即可）
    let (id, _status) =
        get_or_create_result_tx(&mut tx, req.registration_id, user.id, req.late, &req.note).await?;

    sqlx::query("UPDATE results SET electronic = ?, electronic_at = ?, updated_at = ? WHERE id = ?")
        .bind(value)
        .bind(now_sql())
        .bind(now_sql())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    reconcile(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(json!({ "result_id": id, "electronic": value })))
}

#[derive(Deserialize)]
pub struct ManualReq {
    registration_id: i64,
    value: f64,
    #[serde(default)]
    late: bool,
    #[serde(default)]
    note: String,
}

pub async fn enter_manual(
    State(db): State<Db>,
    user: User,
    Json(req): Json<ManualReq>,
) -> AppResult<Json<Value>> {
    user.require_role(&["referee", "chief", "admin"])?;
    if req.value <= 0.0 {
        return Err(AppError::bad_request("成绩必须为正数"));
    }

    let mut tx = db.begin().await?;
    let (id, _status) =
        get_or_create_result_tx(&mut tx, req.registration_id, user.id, req.late, &req.note).await?;
    sqlx::query("UPDATE results SET manual = ?, manual_at = ?, updated_at = ? WHERE id = ?")
        .bind(req.value)
        .bind(now_sql())
        .bind(now_sql())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    reconcile(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(json!({ "result_id": id, "manual": req.value })))
}

async fn get_or_create_result_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    registration_id: i64,
    user_id: i64,
    late: bool,
    note: &str,
) -> AppResult<(i64, String)> {
    if let Some(row) =
        sqlx::query("SELECT id, review_status FROM results WHERE registration_id = ?")
            .bind(registration_id)
            .fetch_optional(&mut **tx)
            .await?
    {
        return Ok((row.get("id"), row.get("review_status")));
    }
    let ck = sqlx::query("SELECT status FROM checkins WHERE registration_id = ?")
        .bind(registration_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| AppError::bad_request("该学生尚未检录，不能录入成绩"))?;
    let status: String = ck.get("status");
    if status != "present" {
        return Err(AppError::bad_request(
            "该学生检录为缺席，需先办理补赛检录后才能录入成绩",
        ));
    }
    let r = sqlx::query(
        "INSERT INTO results (registration_id, review_status, late, note, entered_by)
         VALUES (?, 'raw', ?, ?, ?)",
    )
    .bind(registration_id)
    .bind(late as i64)
    .bind(note)
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    Ok((r.last_insert_rowid(), "raw".to_string()))
}

/// 电子与手记到齐后自动比对：
/// - 一致 => agree，系统给出建议最终值，但仍须裁判确认（不自动发布）
/// - 不一致 => pending，等待总裁判复核，系统不自动选择任何一个
async fn reconcile(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    result_id: i64,
) -> AppResult<()> {
    let row = sqlx::query("SELECT electronic, manual, review_status FROM results WHERE id = ?")
        .bind(result_id)
        .fetch_one(&mut **tx)
        .await?;
    let electronic: Option<f64> = row.get("electronic");
    let manual: Option<f64> = row.get("manual");
    let status: String = row.get("review_status");
    if matches!(status.as_str(), "confirmed" | "resolved") {
        return Ok(()); // 已终局，保留原始记录，不因补录而改变
    }
    if let (Some(e), Some(m)) = (electronic, manual) {
        let agree = (e - m).abs() <= 0.005;
        let (new_status, final_value) = if agree {
            ("agree", Some(e))
        } else {
            ("pending", None)
        };
        sqlx::query("UPDATE results SET review_status = ?, final_value = ? WHERE id = ?")
            .bind(new_status)
            .bind(final_value)
            .bind(result_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn event_direction(db: &Db, registration_id: i64) -> AppResult<bool> {
    let row = sqlx::query(
        "SELECT e.higher_better AS hb
         FROM registrations r JOIN rounds rd ON rd.id = r.round_id
         JOIN events e ON e.id = rd.event_id
         WHERE r.id = ?",
    )
    .bind(registration_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::not_found("报名记录不存在"))?;
    Ok(row.get::<i64, _>("hb") != 0)
}

/// 裁判确认：仅当电子/手记一致（agree），或仅有一个来源且裁判确认其有效
#[derive(Deserialize)]
pub struct ConfirmReq {
    #[serde(default)]
    use_single_source: Option<String>, // "electronic" / "manual"
}

pub async fn confirm_result(
    State(db): State<Db>,
    user: User,
    Path(id): Path<i64>,
    Json(req): Json<ConfirmReq>,
) -> AppResult<Json<Value>> {
    user.require_role(&["referee", "chief", "admin"])?;
    let row = sqlx::query("SELECT * FROM results WHERE id = ?")
        .bind(id)
        .fetch_optional(&db)
        .await?
        .ok_or_else(|| AppError::not_found("成绩不存在"))?;
    let status: String = row.get("review_status");
    let electronic: Option<f64> = row.get("electronic");
    let manual: Option<f64> = row.get("manual");

    let final_value = match status.as_str() {
        "confirmed" | "resolved" => {
            return Err(AppError::bad_request("该成绩已确认，如需修改请发起成绩更正"))
        }
        "pending" => {
            return Err(AppError::conflict(
                "电子计时与手记不一致，须由总裁判复核后才能确认",
            ))
        }
        "agree" => row.get::<Option<f64>, _>("final_value").unwrap(),
        _ => {
            // raw：至少要有一个来源；单一来源需裁判显式指定
            match req.use_single_source.as_deref() {
                Some("electronic") if electronic.is_some() => electronic.unwrap(),
                Some("manual") if manual.is_some() => manual.unwrap(),
                _ => {
                    return Err(AppError::bad_request(
                        "电子/手记尚未到齐或未指定采用哪一个来源，不能确认",
                    ))
                }
            }
        }
    };

    sqlx::query(
        "UPDATE results SET review_status = 'confirmed', final_value = ?,
         confirmed_by = ?, confirmed_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(final_value)
    .bind(user.id)
    .bind(now_sql())
    .bind(now_sql())
    .bind(id)
    .execute(&db)
    .await?;
    Ok(Json(json!({ "ok": true, "final_value": final_value, "review_status": "confirmed" })))
}

#[derive(Deserialize)]
pub struct ResolveReq {
    final_value: f64,
    #[serde(default)]
    note: String,
}

/// 总裁判复核电子/手记不一致：必须人工认定最终成绩并说明依据，
/// 系统不会直接挑选电子或手记中的任何一个。
pub async fn resolve_dispute(
    State(db): State<Db>,
    user: User,
    Path(id): Path<i64>,
    Json(req): Json<ResolveReq>,
) -> AppResult<Json<Value>> {
    user.require_role(&["chief", "admin"])?;
    if req.note.trim().is_empty() {
        return Err(AppError::bad_request("复核必须说明认定依据"));
    }
    if req.final_value <= 0.0 {
        return Err(AppError::bad_request("最终成绩必须为正数"));
    }
    let status: String = sqlx::query("SELECT review_status FROM results WHERE id = ?")
        .bind(id)
        .fetch_optional(&db)
        .await?
        .ok_or_else(|| AppError::not_found("成绩不存在"))?
        .get("review_status");

    if status != "pending" {
        return Err(AppError::bad_request(format!(
            "该成绩当前状态为 {status}，不存在待复核争议"
        )));
    }
    sqlx::query(
        "UPDATE results SET review_status = 'resolved', final_value = ?,
         resolved_by = ?, resolved_at = ?, note = TRIM(COALESCE(note,'') || ' ' || ?),
         confirmed_by = ?, confirmed_at = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(req.final_value)
    .bind(user.id)
    .bind(now_sql())
    .bind(&req.note)
    .bind(user.id)
    .bind(now_sql())
    .bind(now_sql())
    .bind(id)
    .execute(&db)
    .await?;
    Ok(Json(json!({ "ok": true, "review_status": "resolved", "final_value": req.final_value })))
}

// ---------------- 成绩更正（双人审核） ----------------

#[derive(Deserialize)]
pub struct CorrectionReq {
    new_value: f64,
    reason: String,
}

pub async fn request_correction(
    State(db): State<Db>,
    user: User,
    Path(id): Path<i64>,
    Json(req): Json<CorrectionReq>,
) -> AppResult<Json<Value>> {
    user.require_role(&["referee", "chief", "admin"])?;
    if req.reason.trim().is_empty() {
        return Err(AppError::bad_request("更正必须填写原因"));
    }
    if req.new_value <= 0.0 {
        return Err(AppError::bad_request("更正后的成绩必须为正数"));
    }
    let row = sqlx::query("SELECT review_status, final_value FROM results WHERE id = ?")
        .bind(id)
        .fetch_optional(&db)
        .await?
        .ok_or_else(|| AppError::not_found("成绩不存在"))?;
    let status: String = row.get("review_status");
    let old: f64 = row
        .get::<Option<f64>, _>("final_value")
        .ok_or_else(|| AppError::bad_request("该成绩尚未确认，无需更正，可直接修改录入"))?;
    if !["confirmed", "resolved"].contains(&status.as_str()) {
        return Err(AppError::bad_request("只有已确认的成绩才需要走更正流程"));
    }
    if (old - req.new_value).abs() < f64::EPSILON {
        return Err(AppError::bad_request("新成绩与原成绩相同，无需更正"));
    }
    let pending: i64 = sqlx::query(
        "SELECT COUNT(*) AS n FROM corrections WHERE result_id = ? AND status = 'pending'",
    )
    .bind(id)
    .fetch_one(&db)
    .await?
    .get("n");
    if pending > 0 {
        return Err(AppError::conflict("该成绩已有一条待审核的更正申请"));
    }

    let r = sqlx::query(
        "INSERT INTO corrections (result_id, old_value, new_value, reason, requested_by)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(old)
    .bind(req.new_value)
    .bind(req.reason.trim())
    .bind(user.id)
    .execute(&db)
    .await?;
    Ok(Json(json!({ "id": r.last_insert_rowid(), "status": "pending" })))
}

#[derive(Deserialize)]
pub struct ReviewReq {
    decision: String, // approved / rejected
    #[serde(default)]
    review_note: String,
}

pub async fn review_correction(
    State(db): State<Db>,
    user: User,
    Path(id): Path<i64>,
    Json(req): Json<ReviewReq>,
) -> AppResult<Json<Value>> {
    user.require_role(&["referee", "chief", "admin"])?;
    if !["approved", "rejected"].contains(&req.decision.as_str()) {
        return Err(AppError::bad_request("decision 只能是 approved 或 rejected"));
    }

    let mut tx = db.begin().await?;
    let row = sqlx::query("SELECT * FROM corrections WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::not_found("更正申请不存在"))?;
    let status: String = row.get("status");
    if status != "pending" {
        return Err(AppError::conflict("该更正申请已审核"));
    }
    let requested_by: i64 = row.get("requested_by");
    if requested_by == user.id {
        return Err(AppError::forbidden("更正必须由另一人审核，不能审核本人发起的申请"));
    }

    sqlx::query(
        "UPDATE corrections SET status = ?, reviewed_by = ?, review_note = ?, reviewed_at = ?
         WHERE id = ?",
    )
    .bind(&req.decision)
    .bind(user.id)
    .bind(&req.review_note)
    .bind(now_sql())
    .bind(id)
    .execute(&mut *tx)
    .await?;

    if req.decision == "approved" {
        let new_value: f64 = row.get("new_value");
        let result_id: i64 = row.get("result_id");
        // 只改 final_value；electronic / manual 原始计时记录原样保留
        sqlx::query(
            "UPDATE results SET final_value = ?, updated_at = ? WHERE id = ?",
        )
        .bind(new_value)
        .bind(now_sql())
        .bind(result_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "status": req.decision })))
}

pub async fn list_corrections(
    State(db): State<Db>,
    _user: User,
) -> AppResult<Json<Value>> {
    let rows = sqlx::query(
        r#"
        SELECT cr.*, u1.display_name AS requester_name, u2.display_name AS reviewer_name,
               s.name AS student_name, e.name AS event_name, rd.label AS round_label
        FROM corrections cr
        JOIN users u1 ON u1.id = cr.requested_by
        LEFT JOIN users u2 ON u2.id = cr.reviewed_by
        JOIN results r ON r.id = cr.result_id
        JOIN registrations reg ON reg.id = r.registration_id
        JOIN students s ON s.id = reg.student_id
        JOIN rounds rd ON rd.id = reg.round_id
        JOIN events e ON e.id = rd.event_id
        ORDER BY cr.id DESC
        "#,
    )
    .fetch_all(&db)
    .await?;
    Ok(Json(json!({ "corrections": rows_json(rows) })))
}

// ---------------- 排名与发布 ----------------

/// 取某轮次已确认、且检录到场（含补赛）的成绩。缺席者不参与排名。
async fn confirmed_items(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    round_id: i64,
) -> AppResult<(Vec<RankItem>, bool)> {
    let rows = sqlx::query(
        r#"
        SELECT reg.id AS registration_id, s.name AS student_name, s.grade, s.class, reg.bib,
               r.final_value AS score, c.makeup AS makeup, e.higher_better AS hb
        FROM results r
        JOIN registrations reg ON reg.id = r.registration_id
        JOIN students s ON s.id = reg.student_id
        JOIN checkins c ON c.registration_id = reg.id
        JOIN rounds rd ON rd.id = reg.round_id
        JOIN events e ON e.id = rd.event_id
        WHERE reg.round_id = ?
          AND r.review_status IN ('confirmed','resolved')
          AND c.status = 'present'
        ORDER BY reg.bib
        "#,
    )
    .bind(round_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut higher = false;
    let items = rows
        .iter()
        .map(|row| {
            higher = row.get::<i64, _>("hb") != 0;
            RankItem {
                registration_id: row.get("registration_id"),
                student_name: row.get("student_name"),
                grade: row.get("grade"),
                class: row.get("class"),
                bib: row.get("bib"),
                score: row.get("score"),
                makeup: row.get::<i64, _>("makeup") != 0,
            }
        })
        .collect();
    Ok((items, higher))
}

/// 当前实时排名（仅含已确认成绩；用于发布前预览，不进公示屏）
pub async fn current_ranking(
    State(db): State<Db>,
    _user: User,
    Path(round_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let mut tx = db.begin().await?;
    let (items, higher) = confirmed_items(&mut tx, round_id).await?;
    let ranked = rank(items, higher);
    Ok(Json(serde_json::to_value(ranked).unwrap()))
}

pub async fn publish(
    State(db): State<Db>,
    user: User,
    Path(round_id): Path<i64>,
) -> AppResult<Json<Value>> {
    user.require_role(&["chief", "admin"])?;
    let mut tx = db.begin().await?;

    let (items, higher) = confirmed_items(&mut tx, round_id).await?;
    if items.is_empty() {
        return Err(AppError::bad_request("还没有已确认的成绩，不能发布榜单"));
    }
    let ranked = rank(items, higher);

    let next_version: i64 = sqlx::query(
        "SELECT COALESCE(MAX(version), 0) + 1 AS v FROM publications WHERE round_id = ?",
    )
    .bind(round_id)
    .fetch_one(&mut *tx)
    .await?
    .get("v");

    let r = sqlx::query(
        "INSERT INTO publications (round_id, version, created_by) VALUES (?, ?, ?)",
    )
    .bind(round_id)
    .bind(next_version)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    let pub_id = r.last_insert_rowid();

    for item in &ranked {
        sqlx::query(
            "INSERT INTO publication_entries
             (publication_id, rank, student_name, grade, class, bib, score, makeup, tied)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(pub_id)
        .bind(item.rank)
        .bind(&item.student_name)
        .bind(&item.grade)
        .bind(&item.class)
        .bind(&item.bib)
        .bind(item.score)
        .bind(item.makeup as i64)
        .bind(item.tied as i64)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(Json(json!({
        "publication_id": pub_id,
        "version": next_version,
        "entries": serde_json::to_value(&ranked).unwrap(),
    })))
}

pub async fn list_publications(
    State(db): State<Db>,
    _user: User,
    Path(round_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let rows = sqlx::query(
        r#"
        SELECT p.*, u.display_name AS created_by_name,
               (SELECT COUNT(*) FROM publication_entries pe WHERE pe.publication_id = p.id) AS entry_count
        FROM publications p LEFT JOIN users u ON u.id = p.created_by
        WHERE p.round_id = ? ORDER BY p.version DESC
        "#,
    )
    .bind(round_id)
    .fetch_all(&db)
    .await?;
    Ok(Json(json!({ "publications": rows_json(rows) })))
}

pub async fn publication_detail(
    State(db): State<Db>,
    _user: User,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let head = sqlx::query(
        r#"
        SELECT p.*, e.name AS event_name, e.grp AS grp, e.unit, rd.label AS round_label
        FROM publications p
        JOIN rounds rd ON rd.id = p.round_id
        JOIN events e ON e.id = rd.event_id
        WHERE p.id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(&db)
    .await?
    .ok_or_else(|| AppError::not_found("榜单不存在"))?;
    let entries = sqlx::query(
        "SELECT * FROM publication_entries WHERE publication_id = ? ORDER BY rank, bib",
    )
    .bind(id)
    .fetch_all(&db)
    .await?;
    Ok(Json(json!({
        "publication": row_to_json(&head),
        "entries": rows_json(entries),
    })))
}

/// 公示屏：每个轮次只显示“最新一版已发布”榜单，未确认数据永远不会出现
pub async fn public_screen(State(db): State<Db>) -> AppResult<Json<Value>> {
    let boards = sqlx::query(
        r#"
        SELECT p.id AS publication_id, p.version, p.created_at,
               e.name AS event_name, e.grp AS grp, e.unit,
               rd.label AS round_label, rd.id AS round_id
        FROM publications p
        JOIN rounds rd ON rd.id = p.round_id
        JOIN events e ON e.id = rd.event_id
        WHERE p.version = (SELECT MAX(version) FROM publications WHERE round_id = p.round_id)
        ORDER BY e.name, rd.id
        "#,
    )
    .fetch_all(&db)
    .await?;

    let mut out = Vec::new();
    for b in boards {
        let pid: i64 = b.get("publication_id");
        let entries = sqlx::query(
            "SELECT rank, student_name, grade, class, bib, score, makeup, tied
             FROM publication_entries WHERE publication_id = ? ORDER BY rank, bib",
        )
        .bind(pid)
        .fetch_all(&db)
        .await?;
        out.push(json!({
            "board": row_to_json(&b),
            "entries": rows_json(entries),
        }));
    }
    Ok(Json(json!({ "boards": out })))
}

// ---------------- 工作台 ----------------

/// 突出：缺席、补赛、待复核（电子/手记不一致）、待审核更正、漏记、未确认成绩
pub async fn workbench(State(db): State<Db>, _user: User) -> AppResult<Json<Value>> {
    let absent = sqlx::query(
        r#"
        SELECT c.id AS checkin_id, c.note, reg.id AS registration_id, reg.bib,
               s.name AS student_name, s.grade, s.class,
               e.name AS event_name, e.grp AS grp, rd.id AS round_id, rd.label AS round_label
        FROM checkins c
        JOIN registrations reg ON reg.id = c.registration_id
        JOIN students s ON s.id = reg.student_id
        JOIN rounds rd ON rd.id = reg.round_id
        JOIN events e ON e.id = rd.event_id
        WHERE c.status = 'absent' AND c.makeup = 0
        ORDER BY rd.id, reg.bib
        "#,
    )
    .fetch_all(&db)
    .await?;

    let makeup = sqlx::query(
        r#"
        SELECT c.id AS checkin_id, reg.id AS registration_id, reg.bib,
               s.name AS student_name, s.grade, s.class,
               e.name AS event_name, e.grp AS grp, rd.id AS round_id, rd.label AS round_label,
               res.review_status, res.final_value
        FROM checkins c
        JOIN registrations reg ON reg.id = c.registration_id
        JOIN students s ON s.id = reg.student_id
        JOIN rounds rd ON rd.id = reg.round_id
        JOIN events e ON e.id = rd.event_id
        LEFT JOIN results res ON res.registration_id = reg.id
        WHERE c.makeup = 1
        ORDER BY rd.id, reg.bib
        "#,
    )
    .fetch_all(&db)
    .await?;

    let pending_review = sqlx::query(
        r#"
        SELECT res.id AS result_id, res.electronic, res.manual, res.note,
               reg.id AS registration_id, reg.bib,
               s.name AS student_name, s.grade, s.class,
               e.name AS event_name, e.grp AS grp, rd.id AS round_id, rd.label AS round_label
        FROM results res
        JOIN registrations reg ON reg.id = res.registration_id
        JOIN students s ON s.id = reg.student_id
        JOIN rounds rd ON rd.id = reg.round_id
        JOIN events e ON e.id = rd.event_id
        WHERE res.review_status = 'pending'
        ORDER BY res.id
        "#,
    )
    .fetch_all(&db)
    .await?;

    let pending_corrections = sqlx::query(
        r#"
        SELECT cr.id, cr.old_value, cr.new_value, cr.reason,
               u.display_name AS requester_name,
               s.name AS student_name, e.name AS event_name, rd.label AS round_label
        FROM corrections cr
        JOIN users u ON u.id = cr.requested_by
        JOIN results r ON r.id = cr.result_id
        JOIN registrations reg ON reg.id = r.registration_id
        JOIN students s ON s.id = reg.student_id
        JOIN rounds rd ON rd.id = reg.round_id
        JOIN events e ON e.id = rd.event_id
        WHERE cr.status = 'pending'
        ORDER BY cr.id
        "#,
    )
    .fetch_all(&db)
    .await?;

    let late = sqlx::query(
        r#"
        SELECT res.id AS result_id, res.review_status, res.final_value, res.note,
               reg.bib, s.name AS student_name, s.grade, s.class,
               e.name AS event_name, e.grp AS grp, rd.id AS round_id, rd.label AS round_label
        FROM results res
        JOIN registrations reg ON reg.id = res.registration_id
        JOIN students s ON s.id = reg.student_id
        JOIN rounds rd ON rd.id = reg.round_id
        JOIN events e ON e.id = rd.event_id
        WHERE res.late = 1
        ORDER BY res.id
        "#,
    )
    .fetch_all(&db)
    .await?;

    let unconfirmed = sqlx::query(
        r#"
        SELECT res.id AS result_id, res.review_status, res.electronic, res.manual,
               res.final_value, res.late,
               reg.bib, s.name AS student_name,
               e.name AS event_name, rd.id AS round_id, rd.label AS round_label
        FROM results res
        JOIN registrations reg ON reg.id = res.registration_id
        JOIN students s ON s.id = reg.student_id
        JOIN rounds rd ON rd.id = reg.round_id
        JOIN events e ON e.id = rd.event_id
        WHERE res.review_status NOT IN ('confirmed','resolved')
        ORDER BY res.id
        "#,
    )
    .fetch_all(&db)
    .await?;

    Ok(Json(json!({
        "absent": rows_json(absent),
        "makeup": rows_json(makeup),
        "pending_review": rows_json(pending_review),
        "pending_corrections": rows_json(pending_corrections),
        "late": rows_json(late),
        "unconfirmed": rows_json(unconfirmed),
    })))
}
