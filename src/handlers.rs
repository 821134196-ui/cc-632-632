use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};

use crate::auth::{require_role, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::ranking::{rank, RankItem};
use crate::AppState;
use std::collections::HashMap;

const TOL_SECONDS: f64 = 0.2;
const TOL_METERS: f64 = 0.02;

async fn audit(db: &SqlitePool, actor: i64, action: &str, detail: &str) {
    let _ = sqlx::query("INSERT INTO audit_log(actor_id, action, detail) VALUES (?,?,?)")
        .bind(actor)
        .bind(action)
        .bind(detail)
        .execute(db)
        .await;
}

// ---------- 基础数据 ----------

pub async fn list_users(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT id, username, display_name, role FROM users ORDER BY id")
        .fetch_all(&s.db)
        .await?;
    let users: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<i64,_>("id"),
                "username": r.get::<String,_>("username"),
                "display_name": r.get::<String,_>("display_name"),
                "role": r.get::<String,_>("role"),
            })
        })
        .collect();
    Ok(Json(json!(users)))
}

#[derive(Deserialize)]
pub struct StudentIn {
    name: String,
    grade: i64,
    class: String,
    bib_number: String,
}

pub async fn create_student(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Json(b): Json<StudentIn>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_role(&s, id, &["teacher", "head_referee"]).await?;
    if b.name.trim().is_empty() || b.bib_number.trim().is_empty() {
        return Err(ApiError::bad("姓名和号码布不能为空"));
    }
    let r = sqlx::query(
        "INSERT INTO students(name, grade, class, bib_number) VALUES (?,?,?,?)",
    )
    .bind(b.name.trim())
    .bind(b.grade)
    .bind(&b.class)
    .bind(b.bib_number.trim())
    .execute(&s.db)
    .await?;
    audit(&s.db, id, "student.create", &format!("新增学生 {}", b.name)).await;
    Ok((StatusCode::CREATED, Json(json!({ "id": r.last_insert_rowid() }))))
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct ListQ {
    q: Option<String>,
    event_id: Option<i64>,
    round_id: Option<i64>,
}

pub async fn list_students(
    State(s): State<AppState>,
    Query(q): Query<ListQ>,
) -> ApiResult<Json<Value>> {
    let rows = if q.q.as_ref().map(|x| !x.trim().is_empty()).unwrap_or(false) {
        let kw = format!("%{}%", q.q.unwrap().trim());
        sqlx::query(
            "SELECT id, name, grade, class, bib_number FROM students
             WHERE name LIKE ?1 OR bib_number LIKE ?1
             ORDER BY grade, class, bib_number",
        )
        .bind(kw)
        .fetch_all(&s.db)
        .await?
    } else {
        sqlx::query(
            "SELECT id, name, grade, class, bib_number FROM students
             ORDER BY grade, class, bib_number",
        )
        .fetch_all(&s.db)
        .await?
    };
    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64,_>("id"),
            "name": r.get::<String,_>("name"),
            "grade": r.get::<i64,_>("grade"),
            "class": r.get::<String,_>("class"),
            "bib_number": r.get::<String,_>("bib_number"),
        }))
        .collect::<Vec<_>>())))
}

pub async fn list_events(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        "SELECT e.id, e.name, e.group_name, e.unit, e.sort_rank,
                (SELECT COUNT(*) FROM entries en WHERE en.event_id=e.id) AS entry_count
         FROM events e ORDER BY e.id",
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64,_>("id"),
            "name": r.get::<String,_>("name"),
            "group_name": r.get::<String,_>("group_name"),
            "unit": r.get::<String,_>("unit"),
            "sort_rank": r.get::<i64,_>("sort_rank"),
            "entry_count": r.get::<i64,_>("entry_count"),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
pub struct EventIn {
    name: String,
    group_name: String,
    unit: Option<String>,
    sort_rank: Option<i64>,
}

pub async fn create_event(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Json(b): Json<EventIn>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_role(&s, id, &["teacher", "head_referee"]).await?;
    let unit = b.unit.unwrap_or_else(|| "seconds".into());
    if unit != "seconds" && unit != "meters" {
        return Err(ApiError::bad("unit 只能是 seconds 或 meters"));
    }
    let sort_rank = b.sort_rank.unwrap_or(1);
    let r = sqlx::query(
        "INSERT INTO events(name, group_name, unit, sort_rank) VALUES (?,?,?,?)",
    )
    .bind(b.name.trim())
    .bind(b.group_name.trim())
    .bind(&unit)
    .bind(sort_rank)
    .execute(&s.db)
    .await?;
    let event_id = r.last_insert_rowid();
    // 新项目自动建决赛轮次
    sqlx::query("INSERT INTO rounds(event_id, name, is_reshoot) VALUES (?, '决赛', 0)")
        .bind(event_id)
        .execute(&s.db)
        .await?;
    audit(&s.db, id, "event.create", &format!("新增项目 {} {}", b.name, b.group_name)).await;
    Ok((StatusCode::CREATED, Json(json!({ "id": event_id }))))
}

// ---------- 报名 ----------

pub async fn list_entries(
    State(s): State<AppState>,
    Query(q): Query<ListQ>,
) -> ApiResult<Json<Value>> {
    let rows = if let Some(eid) = q.event_id {
        sqlx::query(
            "SELECT en.id AS entry_id, en.event_id, en.student_id,
                    st.name AS student_name, st.grade, st.class, st.bib_number
             FROM entries en JOIN students st ON st.id=en.student_id
             WHERE en.event_id=? ORDER BY st.bib_number",
        )
        .bind(eid)
        .fetch_all(&s.db)
        .await?
    } else {
        sqlx::query(
            "SELECT en.id AS entry_id, en.event_id, en.student_id,
                    st.name AS student_name, st.grade, st.class, st.bib_number
             FROM entries en JOIN students st ON st.id=en.student_id
             ORDER BY en.event_id, st.bib_number",
        )
        .fetch_all(&s.db)
        .await?
    };
    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "entry_id": r.get::<i64,_>("entry_id"),
            "event_id": r.get::<i64,_>("event_id"),
            "student_id": r.get::<i64,_>("student_id"),
            "student_name": r.get::<String,_>("student_name"),
            "grade": r.get::<i64,_>("grade"),
            "class": r.get::<String,_>("class"),
            "bib_number": r.get::<String,_>("bib_number"),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
pub struct EntryIn {
    event_id: i64,
    student_id: i64,
}

pub async fn create_entry(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Json(b): Json<EntryIn>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_role(&s, id, &["teacher"]).await?;
    let r = sqlx::query("INSERT INTO entries(event_id, student_id, entered_by) VALUES (?,?,?)")
        .bind(b.event_id)
        .bind(b.student_id)
        .bind(id)
        .execute(&s.db)
        .await?;
    audit(&s.db, id, "entry.create", &format!("报名 event={} student={}", b.event_id, b.student_id)).await;
    Ok((StatusCode::CREATED, Json(json!({ "id": r.last_insert_rowid() }))))
}

// ---------- 轮次 ----------

#[derive(Deserialize)]
pub struct RoundIn {
    event_id: i64,
    name: String,
    is_reshoot: Option<bool>,
}

pub async fn create_round(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Json(b): Json<RoundIn>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_role(&s, id, &["clerk", "head_referee"]).await?;
    let r = sqlx::query("INSERT INTO rounds(event_id, name, is_reshoot) VALUES (?,?,?)")
        .bind(b.event_id)
        .bind(b.name.trim())
        .bind(b.is_reshoot.unwrap_or(false) as i64)
        .execute(&s.db)
        .await?;
    audit(&s.db, id, "round.create", &format!("新建轮次 event={} {} 补赛={}", b.event_id, b.name, b.is_reshoot.unwrap_or(false))).await;
    Ok((StatusCode::CREATED, Json(json!({ "id": r.last_insert_rowid() }))))
}

pub async fn list_rounds(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        "SELECT r.id, r.event_id, r.name, r.is_reshoot, e.name AS event_name,
                e.group_name, e.unit, e.sort_rank,
                (SELECT MAX(version) FROM leaderboard_versions v WHERE v.round_id=r.id) AS published_version
         FROM rounds r JOIN events e ON e.id=r.event_id
         ORDER BY r.event_id, r.id",
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64,_>("id"),
            "event_id": r.get::<i64,_>("event_id"),
            "name": r.get::<String,_>("name"),
            "is_reshoot": r.get::<i64,_>("is_reshoot") == 1,
            "event_name": r.get::<String,_>("event_name"),
            "group_name": r.get::<String,_>("group_name"),
            "unit": r.get::<String,_>("unit"),
            "sort_rank": r.get::<i64,_>("sort_rank"),
            "published_version": r.get::<Option<i64>,_>("published_version"),
        }))
        .collect::<Vec<_>>())))
}

// ---------- 检录 ----------

/// 轮次花名册：报名者 + 检录/成绩状态
pub async fn round_roster(
    State(s): State<AppState>,
    Path(round_id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let round = sqlx::query(
        "SELECT r.*, e.name AS event_name, e.group_name, e.unit, e.sort_rank
         FROM rounds r JOIN events e ON e.id=r.event_id WHERE r.id=?",
    )
    .bind(round_id)
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(|| ApiError::not_found("轮次不存在"))?;

    let event_id = round.get::<i64,_>("event_id");
    let rows = sqlx::query(
        "SELECT en.id AS entry_id, st.id AS student_id, st.name AS student_name,
                st.bib_number, st.grade, st.class,
                c.id AS checkin_id, c.status AS checkin_status, c.note AS checkin_note,
                res.id AS result_id, res.confirmation, res.published_value, res.resolution
         FROM entries en
         JOIN students st ON st.id=en.student_id
         LEFT JOIN checkins c ON c.entry_id=en.id AND c.round_id=?
         LEFT JOIN results res ON res.checkin_id=c.id
         WHERE en.event_id=?
         ORDER BY st.bib_number",
    )
    .bind(round_id)
    .bind(event_id)
    .fetch_all(&s.db)
    .await?;

    // 附带原始计时
    let mut checkin_ids: Vec<i64> = vec![];
    for r in &rows {
        if let Some(cid) = r.get::<Option<i64>,_>("checkin_id") {
            checkin_ids.push(cid);
        }
    }
    let mut raw_map: HashMap<i64, Vec<Value>> = HashMap::new();
    if !checkin_ids.is_empty() {
        let placeholders = checkin_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT checkin_id, source, value, note, recorded_at FROM raw_results
             WHERE checkin_id IN ({placeholders}) ORDER BY source"
        );
        let mut q = sqlx::query(&sql);
        for cid in &checkin_ids {
            q = q.bind(cid);
        }
        for r in q.fetch_all(&s.db).await? {
            let cid = r.get::<i64,_>("checkin_id");
            raw_map.entry(cid).or_default().push(json!({
                "source": r.get::<String,_>("source"),
                "value": r.get::<f64,_>("value"),
                "note": r.get::<String,_>("note"),
                "recorded_at": r.get::<String,_>("recorded_at"),
            }));
        }
    }

    let roster: Vec<Value> = rows
        .iter()
        .map(|r| {
            let cid = r.get::<Option<i64>,_>("checkin_id");
            json!({
                "entry_id": r.get::<i64,_>("entry_id"),
                "student_id": r.get::<i64,_>("student_id"),
                "student_name": r.get::<String,_>("student_name"),
                "bib_number": r.get::<String,_>("bib_number"),
                "grade": r.get::<i64,_>("grade"),
                "class": r.get::<String,_>("class"),
                "checkin_id": cid,
                "checkin_status": r.get::<Option<String>,_>("checkin_status"),
                "checkin_note": r.get::<Option<String>,_>("checkin_note"),
                "result_id": r.get::<Option<i64>,_>("result_id"),
                "confirmation": r.get::<Option<String>,_>("confirmation"),
                "published_value": r.get::<Option<f64>,_>("published_value"),
                "resolution": r.get::<Option<String>,_>("resolution"),
                "raw_results": cid.and_then(|c| raw_map.get(&c)).cloned().unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(json!({
        "round": {
            "id": round_id,
            "event_id": event_id,
            "name": round.get::<String,_>("name"),
            "is_reshoot": round.get::<i64,_>("is_reshoot") == 1,
            "event_name": round.get::<String,_>("event_name"),
            "group_name": round.get::<String,_>("group_name"),
            "unit": round.get::<String,_>("unit"),
            "sort_rank": round.get::<i64,_>("sort_rank"),
        },
        "roster": roster
    })))
}

#[derive(Deserialize)]
pub struct CheckinIn {
    entry_id: i64,
    round_id: i64,
    status: String,
    note: Option<String>,
}

pub async fn create_checkin(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Json(b): Json<CheckinIn>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_role(&s, id, &["clerk"]).await?;
    if b.status != "present" && b.status != "absent" {
        return Err(ApiError::bad("检录状态只能是 present 或 absent"));
    }

    // 校验轮次存在，且报名记录确实属于该轮次的项目（防止跨项目检录）
    let round_event: Option<(i64,)> = sqlx::query_as(
        "SELECT r.event_id FROM rounds r
         JOIN entries en ON en.id=? AND en.event_id=r.event_id
         WHERE r.id=?",
    )
    .bind(b.entry_id)
    .bind(b.round_id)
    .fetch_optional(&s.db)
    .await?;
    if round_event.is_none() {
        return Err(ApiError::bad("该学生未报名本轮次所属项目，不能检录"));
    }

    // 核心规则：同一学生在同一轮次不能重复检录
    let existed = sqlx::query("SELECT id, status FROM checkins WHERE entry_id=? AND round_id=?")
        .bind(b.entry_id)
        .bind(b.round_id)
        .fetch_optional(&s.db)
        .await?;
    if let Some(row) = existed {
        return Err(ApiError::conflict(format!(
            "重复检录被拒绝：该学生在本轮次已有检录记录（状态：{}）",
            if row.get::<String,_>("status") == "present" { "到场" } else { "缺席" }
        )));
    }

    let r = sqlx::query(
        "INSERT INTO checkins(entry_id, round_id, status, clerk_id, note) VALUES (?,?,?,?,?)",
    )
    .bind(b.entry_id)
    .bind(b.round_id)
    .bind(&b.status)
    .bind(id)
    .bind(b.note.unwrap_or_default())
    .execute(&s.db)
    .await?;
    let cid = r.last_insert_rowid();

    if b.status == "present" {
        // 预建一条 pending 成绩；补赛轮次保持 pending 待总裁确认
        let is_reshoot: i64 = sqlx::query("SELECT is_reshoot FROM rounds WHERE id=?")
            .bind(b.round_id)
            .fetch_one(&s.db)
            .await?
            .get(0);
        sqlx::query("INSERT INTO results(checkin_id, confirmation) VALUES (?, 'pending')")
            .bind(cid)
            .execute(&s.db)
            .await?;
        audit(&s.db, id, "checkin.present", &format!("检录到场 checkin={cid} 补赛轮次={is_reshoot}")).await;
    } else {
        audit(&s.db, id, "checkin.absent", &format!("检录缺席 checkin={cid}（不参与排名）")).await;
    }
    Ok((StatusCode::CREATED, Json(json!({ "id": cid, "status": b.status }))))
}

// ---------- 裁判：计时 / 成绩 ----------

#[allow(dead_code)]
struct CheckinMeta {
    checkin_id: i64,
    checkin_status: String,
    is_reshoot: i64,
    unit: String,
    sort_rank: i64,
}

async fn checkin_meta(db: &SqlitePool, checkin_id: i64) -> ApiResult<CheckinMeta> {
    let row = sqlx::query(
        "SELECT c.id, c.status, r.is_reshoot, e.unit, e.sort_rank
         FROM checkins c
         JOIN rounds r ON r.id=c.round_id
         JOIN entries en ON en.id=c.entry_id
         JOIN events e ON e.id=en.event_id
         WHERE c.id=?",
    )
    .bind(checkin_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| ApiError::not_found("检录记录不存在"))?;
    Ok(CheckinMeta {
        checkin_id: row.get(0),
        checkin_status: row.get(1),
        is_reshoot: row.get(2),
        unit: row.get(3),
        sort_rank: row.get(4),
    })
}

/// 依据电子/手记两条记录决定状态：
/// 一致 → 自动确认；分歧且非补赛 → 生成复核单保持 pending；补赛轮次 → 一律待总裁确认
async fn reconcile_result(db: &SqlitePool, checkin_id: i64, meta: &CheckinMeta) -> ApiResult<Value> {
    let raws = sqlx::query("SELECT source, value FROM raw_results WHERE checkin_id=?")
        .bind(checkin_id)
        .fetch_all(db)
        .await?;
    let mut elec = None;
    let mut manual = None;
    for r in &raws {
        match r.get::<String,_>("source").as_str() {
            "electronic" => elec = Some(r.get::<f64,_>("value")),
            "manual" => manual = Some(r.get::<f64,_>("value")),
            _ => {}
        }
    }

    let ensure_result = || async move {
        let row = sqlx::query("SELECT id FROM results WHERE checkin_id=?")
            .bind(checkin_id)
            .fetch_optional(db)
            .await?;
        match row {
            Some(r) => Ok::<i64, ApiError>(r.get(0)),
            None => {
                let r = sqlx::query("INSERT INTO results(checkin_id, confirmation) VALUES (?,'pending')")
                    .bind(checkin_id)
                    .execute(db)
                    .await?;
                Ok(r.last_insert_rowid())
            }
        }
    };
    let result_id = ensure_result().await?;

    let tolerance = if meta.unit == "seconds" { TOL_SECONDS } else { TOL_METERS };

    match (elec, manual) {
        (Some(e), Some(m)) => {
            // 已有 open 复核单则不重复生成
            let existing: Option<(i64, String)> = sqlx::query_as(
                "SELECT id, status FROM timing_disputes WHERE result_id=?",
            )
            .bind(result_id)
            .fetch_optional(db)
            .await?;

            if meta.is_reshoot == 1 {
                // 补赛：即使两条都有，也必须总裁确认
                if existing.is_none() {
                    sqlx::query(
                        "INSERT INTO timing_disputes(result_id, electronic_val, manual_val, status)
                         VALUES (?,?,?, 'open')",
                    )
                    .bind(result_id).bind(e).bind(m)
                    .execute(db).await?;
                }
                Ok(json!({"state":"pending_reshoot","result_id":result_id,"electronic":e,"manual":m}))
            } else if (e - m).abs() <= tolerance {
                // 一致：自动采用电子计时，直接确认
                sqlx::query(
                    "UPDATE results SET published_value=?, resolution='auto',
                     confirmation='confirmed', confirmed_by=NULL,
                     confirmed_at=datetime('now') WHERE id=?",
                )
                .bind(e)
                .bind(result_id)
                .execute(db)
                .await?;
                if let Some((did, _)) = existing {
                    sqlx::query("UPDATE timing_disputes SET status='resolved' WHERE id=?")
                        .bind(did).execute(db).await?;
                }
                Ok(json!({"state":"auto_confirmed","result_id":result_id,"published_value":e,"electronic":e,"manual":m}))
            } else {
                // 分歧：不选任何一个发布，交总裁判复核
                match existing {
                    Some((did, st)) if st == "open" => {
                        sqlx::query(
                            "UPDATE timing_disputes SET electronic_val=?, manual_val=? WHERE id=?",
                        )
                        .bind(e).bind(m).bind(did)
                        .execute(db).await?;
                    }
                    _ => {
                        sqlx::query(
                            "INSERT INTO timing_disputes(result_id, electronic_val, manual_val, status)
                             VALUES (?,?,?, 'open')",
                        )
                        .bind(result_id).bind(e).bind(m)
                        .execute(db).await?;
                    }
                }
                sqlx::query(
                    "UPDATE results SET published_value=NULL, resolution='auto',
                     confirmation='pending' WHERE id=?",
                )
                .bind(result_id)
                .execute(db)
                .await?;
                Ok(json!({"state":"dispute","result_id":result_id,"electronic":e,"manual":m,"tolerance":tolerance}))
            }
        }
        (Some(e), None) => Ok(json!({"state":"waiting_manual","result_id":result_id,"electronic":e})),
        (None, Some(m)) => Ok(json!({"state":"waiting_electronic","result_id":result_id,"manual":m})),
        (None, None) => Ok(json!({"state":"empty","result_id":result_id})),
    }
}

#[derive(Deserialize)]
pub struct TimingIn {
    checkin_id: i64,
    source: String,
    value: f64,
    note: Option<String>,
}

pub async fn record_timing(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Json(b): Json<TimingIn>,
) -> ApiResult<Json<Value>> {
    require_role(&s, id, &["referee", "head_referee"]).await?;
    if b.source != "electronic" && b.source != "manual" {
        return Err(ApiError::bad("source 只能是 electronic 或 manual"));
    }
    if !b.value.is_finite() || b.value <= 0.0 {
        return Err(ApiError::bad("成绩必须是正数"));
    }
    let meta = checkin_meta(&s.db, b.checkin_id).await?;
    if meta.checkin_status != "present" {
        return Err(ApiError::bad("缺席学生不能录入成绩（不参与排名）"));
    }
    // 原始记录不可变，同一来源重复录入 → 冲突
    let dup = sqlx::query("SELECT id FROM raw_results WHERE checkin_id=? AND source=?")
        .bind(b.checkin_id)
        .bind(&b.source)
        .fetch_optional(&s.db)
        .await?;
    if dup.is_some() {
        return Err(ApiError::conflict(format!(
            "{}原始记录已存在且不可修改；如需变更请走“成绩更正”流程",
            if b.source == "electronic" { "电子计时" } else { "手记" }
        )));
    }
    sqlx::query(
        "INSERT INTO raw_results(checkin_id, source, value, recorded_by, note) VALUES (?,?,?,?,?)",
    )
    .bind(b.checkin_id)
    .bind(&b.source)
    .bind(b.value)
    .bind(id)
    .bind(b.note.unwrap_or_default())
    .execute(&s.db)
    .await?;
    audit(&s.db, id, "timing.record", &format!("{}={} checkin={}", b.source, b.value, b.checkin_id)).await;
    let out = reconcile_result(&s.db, b.checkin_id, &meta).await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct SimulateIn {
    checkin_id: i64,
    force_dispute: Option<bool>,
}

/// 本地模拟计时器：一次生成电子计时与手记两条原始记录
pub async fn simulate_timer(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Json(b): Json<SimulateIn>,
) -> ApiResult<Json<Value>> {
    require_role(&s, id, &["referee", "head_referee"]).await?;
    let meta = checkin_meta(&s.db, b.checkin_id).await?;
    if meta.checkin_status != "present" {
        return Err(ApiError::bad("缺席学生不能模拟计时"));
    }

    let base: f64 = if meta.unit == "seconds" {
        9.0 + rand::thread_rng().gen_range(0.0..4.0)
    } else {
        2.5 + rand::thread_rng().gen_range(0.0..2.5)
    };
    let tol = if meta.unit == "seconds" { TOL_SECONDS } else { TOL_METERS };

    let (elec, man) = if b.force_dispute.unwrap_or(false) {
        let delta = tol * (2.0 + rand::thread_rng().gen_range(0.0..2.0));
        let sign = if rand::thread_rng().gen_bool(0.5) { 1.0 } else { -1.0 };
        (base, ((base + delta * sign) * 100.0).round() / 100.0)
    } else {
        // 大部分情况落在容差内，偶尔接近边界
        let d = rand::thread_rng().gen_range(-tol..tol) * 0.8;
        (base, ((base + d) * 100.0).round() / 100.0)
    };
    let elec = (elec * 100.0).round() / 100.0;

    let mut tx = s.db.begin().await?;
    for (source, value) in [("electronic", elec), ("manual", man)] {
        sqlx::query(
            "INSERT INTO raw_results(checkin_id, source, value, recorded_by, note)
             VALUES (?,?,?,?, '本地模拟计时器')",
        )
        .bind(b.checkin_id).bind(source).bind(value).bind(id)
        .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    audit(&s.db, id, "timer.simulate", &format!("模拟计时 checkin={} 电子={elec} 手记={man}", b.checkin_id)).await;
    let out = reconcile_result(&s.db, b.checkin_id, &meta).await?;
    Ok(Json(json!({ "electronic": elec, "manual": man, "reconcile": out })))
}

#[derive(Deserialize)]
pub struct MissingIn {
    checkin_id: i64,
    value: f64,
    note: Option<String>,
}

/// 漏记补录：裁判事后补录，必须经总裁判确认
pub async fn record_missing(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Json(b): Json<MissingIn>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_role(&s, id, &["referee", "head_referee"]).await?;
    if !b.value.is_finite() || b.value <= 0.0 {
        return Err(ApiError::bad("成绩必须是正数"));
    }
    let meta = checkin_meta(&s.db, b.checkin_id).await?;
    if meta.checkin_status != "present" {
        return Err(ApiError::bad("缺席学生不存在漏记补录"));
    }
    let existing = sqlx::query("SELECT id FROM raw_results WHERE checkin_id=?")
        .bind(b.checkin_id)
        .fetch_optional(&s.db)
        .await?;
    if existing.is_some() {
        return Err(ApiError::conflict("该生已有原始记录，漏记补录仅适用于完全无记录的情况；请走成绩更正"));
    }
    let note = format!("【漏记补录】{}", b.note.unwrap_or_default());
    sqlx::query(
        "INSERT INTO raw_results(checkin_id, source, value, recorded_by, note)
         VALUES (?, 'manual', ?, ?, ?)",
    )
    .bind(b.checkin_id).bind(b.value).bind(id).bind(&note)
    .execute(&s.db).await?;

    let row = sqlx::query("SELECT id FROM results WHERE checkin_id=?")
        .bind(b.checkin_id)
        .fetch_optional(&s.db)
        .await?;
    let result_id = match row {
        Some(r) => r.get::<i64,_>(0),
        None => sqlx::query("INSERT INTO results(checkin_id, confirmation) VALUES (?,'pending')")
            .bind(b.checkin_id).execute(&s.db).await?.last_insert_rowid()
    };
    sqlx::query("UPDATE results SET confirmation='pending', published_value=NULL, resolution='missing' WHERE id=?")
        .bind(result_id).execute(&s.db).await?;
    audit(&s.db, id, "result.missing", &format!("漏记补录 result={result_id} value={}（待复核）", b.value)).await;
    Ok((StatusCode::CREATED, Json(json!({"state":"pending_missing","result_id":result_id,"value":b.value}))))
}

// ---------- 总裁判：复核 / 裁决 ----------

pub async fn review_queue(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
) -> ApiResult<Json<Value>> {
    require_role(&s, id, &["head_referee"]).await?;

    let disputes = sqlx::query(
        "SELECT d.id AS dispute_id, d.result_id, d.electronic_val, d.manual_val, d.status,
                st.name AS student_name, st.bib_number,
                e.name AS event_name, e.group_name, e.unit, ro.name AS round_name, ro.is_reshoot
         FROM timing_disputes d
         JOIN results res ON res.id=d.result_id
         JOIN checkins c ON c.id=res.checkin_id
         JOIN entries en ON en.id=c.entry_id
         JOIN students st ON st.id=en.student_id
         JOIN rounds ro ON ro.id=c.round_id
         JOIN events e ON e.id=en.event_id
         WHERE d.status='open' ORDER BY d.id",
    )
    .fetch_all(&s.db)
    .await?;
    let dispute_list: Vec<Value> = disputes.iter().map(|r| json!({
        "dispute_id": r.get::<i64,_>("dispute_id"),
        "result_id": r.get::<i64,_>("result_id"),
        "electronic_val": r.get::<f64,_>("electronic_val"),
        "manual_val": r.get::<f64,_>("manual_val"),
        "student_name": r.get::<String,_>("student_name"),
        "bib_number": r.get::<String,_>("bib_number"),
        "event_name": r.get::<String,_>("event_name"),
        "group_name": r.get::<String,_>("group_name"),
        "unit": r.get::<String,_>("unit"),
        "round_name": r.get::<String,_>("round_name"),
        "is_reshoot": r.get::<i64,_>("is_reshoot") == 1,
        "kind": if r.get::<i64,_>("is_reshoot") == 1 { "reshoot" } else { "dispute" },
    })).collect();

    // 漏记/补赛等待确认的成绩（没有 open 复核单的 pending 成绩）
    let pending = sqlx::query(
        "SELECT res.id AS result_id, res.resolution, c.id AS checkin_id,
                st.name AS student_name, st.bib_number,
                e.name AS event_name, e.group_name, e.unit, ro.name AS round_name, ro.is_reshoot,
                (SELECT value FROM raw_results rr WHERE rr.checkin_id=c.id AND rr.source='manual') AS manual_val,
                (SELECT value FROM raw_results rr WHERE rr.checkin_id=c.id AND rr.source='electronic') AS elec_val
         FROM results res
         JOIN checkins c ON c.id=res.checkin_id
         JOIN entries en ON en.id=c.entry_id
         JOIN students st ON st.id=en.student_id
         JOIN rounds ro ON ro.id=c.round_id
         JOIN events e ON e.id=en.event_id
         WHERE res.confirmation='pending'
           AND NOT EXISTS (SELECT 1 FROM timing_disputes d WHERE d.result_id=res.id AND d.status='open')
         ORDER BY res.id",
    )
    .fetch_all(&s.db)
    .await?;
    let pending_list: Vec<Value> = pending.iter().map(|r| {
        let reshoot = r.get::<i64,_>("is_reshoot") == 1;
        json!({
            "result_id": r.get::<i64,_>("result_id"),
            "checkin_id": r.get::<i64,_>("checkin_id"),
            "resolution": r.get::<String,_>("resolution"),
            "student_name": r.get::<String,_>("student_name"),
            "bib_number": r.get::<String,_>("bib_number"),
            "event_name": r.get::<String,_>("event_name"),
            "group_name": r.get::<String,_>("group_name"),
            "unit": r.get::<String,_>("unit"),
            "round_name": r.get::<String,_>("round_name"),
            "is_reshoot": reshoot,
            "manual_val": r.get::<Option<f64>,_>("manual_val"),
            "electronic_val": r.get::<Option<f64>,_>("elec_val"),
            "kind": if reshoot { "reshoot" } else { "missing" },
        })
    }).collect();

    let corrections = sqlx::query(
        "SELECT co.id, co.result_id, co.old_value, co.new_value, co.reason,
                co.requested_by, co.requested_at, u.display_name AS requester_name,
                st.name AS student_name, e.name AS event_name, e.group_name
         FROM corrections co
         JOIN users u ON u.id=co.requested_by
         JOIN results res ON res.id=co.result_id
         JOIN checkins c ON c.id=res.checkin_id
         JOIN entries en ON en.id=c.entry_id
         JOIN students st ON st.id=en.student_id
         JOIN events e ON e.id=en.event_id
         WHERE co.status='pending' ORDER BY co.id",
    )
    .fetch_all(&s.db)
    .await?;
    let corr_list: Vec<Value> = corrections.iter().map(|r| json!({
        "id": r.get::<i64,_>("id"),
        "result_id": r.get::<i64,_>("result_id"),
        "old_value": r.get::<f64,_>("old_value"),
        "new_value": r.get::<f64,_>("new_value"),
        "reason": r.get::<String,_>("reason"),
        "requested_by": r.get::<i64,_>("requested_by"),
        "requester_name": r.get::<String,_>("requester_name"),
        "requested_at": r.get::<String,_>("requested_at"),
        "student_name": r.get::<String,_>("student_name"),
        "event_name": r.get::<String,_>("event_name"),
        "group_name": r.get::<String,_>("group_name"),
    })).collect();

    Ok(Json(json!({
        "disputes": dispute_list,
        "pending_results": pending_list,
        "corrections": corr_list
    })))
}

#[derive(Deserialize)]
pub struct DecideIn {
    chosen_value: f64,
    basis: String,
}

pub async fn decide_dispute(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Path(dispute_id): Path<i64>,
    Json(b): Json<DecideIn>,
) -> ApiResult<Json<Value>> {
    require_role(&s, id, &["head_referee"]).await?;
    if b.basis.trim().is_empty() {
        return Err(ApiError::bad("必须填写裁决依据"));
    }
    let d = sqlx::query("SELECT id, result_id, electronic_val, manual_val, status FROM timing_disputes WHERE id=?")
        .bind(dispute_id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| ApiError::not_found("复核单不存在"))?;
    if d.get::<String,_>("status") != "open" {
        return Err(ApiError::conflict("该复核单已裁决"));
    }
    let result_id = d.get::<i64,_>("result_id");
    let (e, m) = (d.get::<f64,_>("electronic_val"), d.get::<f64,_>("manual_val"));
    if (b.chosen_value - e).abs() > 1e-9 && (b.chosen_value - m).abs() > 1e-9 {
        return Err(ApiError::bad("裁决值必须是电子计时或手记中的一个；其他数值请走成绩更正"));
    }
    let resolution = if (b.chosen_value - e).abs() <= 1e-9 { "electronic" } else { "manual" };

    let mut tx = s.db.begin().await?;
    sqlx::query(
        "UPDATE timing_disputes SET chosen_value=?, basis=?, decided_by=?,
         decided_at=datetime('now'), status='resolved' WHERE id=?",
    )
    .bind(b.chosen_value).bind(b.basis.trim()).bind(id).bind(dispute_id)
    .execute(&mut *tx).await?;
    sqlx::query(
        "UPDATE results SET published_value=?, resolution=?, confirmation='confirmed',
         confirmed_by=?, confirmed_at=datetime('now') WHERE id=?",
    )
    .bind(b.chosen_value).bind(resolution).bind(id).bind(result_id)
    .execute(&mut *tx).await?;
    tx.commit().await?;
    audit(&s.db, id, "dispute.decide", &format!("复核单{dispute_id} 采用{resolution}={} 依据：{}", b.chosen_value, b.basis)).await;
    Ok(Json(json!({"state":"confirmed","result_id":result_id,"published_value":b.chosen_value,"resolution":resolution})))
}

#[derive(Deserialize)]
pub struct ConfirmIn {
    note: Option<String>,
}

/// 总裁确认漏记/补赛成绩
pub async fn confirm_result(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Path(result_id): Path<i64>,
    Json(b): Json<ConfirmIn>,
) -> ApiResult<Json<Value>> {
    require_role(&s, id, &["head_referee"]).await?;
    let r = sqlx::query(
        "SELECT res.confirmation, c.id AS checkin_id
         FROM results res JOIN checkins c ON c.id=res.checkin_id WHERE res.id=?",
    )
    .bind(result_id)
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(|| ApiError::not_found("成绩不存在"))?;
    if r.get::<String,_>("confirmation") != "pending" {
        return Err(ApiError::conflict("只能确认待复核成绩"));
    }
    let checkin_id = r.get::<i64,_>("checkin_id");
    let val: Option<f64> = sqlx::query(
        "SELECT value FROM raw_results WHERE checkin_id=? ORDER BY CASE source WHEN 'electronic' THEN 0 ELSE 1 END LIMIT 1",
    )
    .bind(checkin_id)
    .fetch_optional(&s.db)
    .await?
    .map(|row| row.get(0));
    let val = val.ok_or_else(|| ApiError::bad("没有任何原始记录可供确认"))?;

    sqlx::query(
        "UPDATE results SET published_value=?, confirmation='confirmed', confirmed_by=?,
         confirmed_at=datetime('now'),
         resolution=CASE WHEN resolution='missing' THEN 'missing_confirmed'
                         WHEN resolution='auto' THEN 'reshoot_confirmed'
                         ELSE resolution END
         WHERE id=?",
    )
    .bind(val).bind(id).bind(result_id)
    .execute(&s.db)
    .await?;
    audit(&s.db, id, "result.confirm", &format!("总裁确认 result={result_id} value={val} {}", b.note.unwrap_or_default())).await;
    Ok(Json(json!({"state":"confirmed","result_id":result_id,"published_value":val})))
}

#[derive(Deserialize)]
pub struct RejectIn {
    reason: String,
}

pub async fn reject_result(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Path(result_id): Path<i64>,
    Json(b): Json<RejectIn>,
) -> ApiResult<Json<Value>> {
    require_role(&s, id, &["head_referee"]).await?;
    if b.reason.trim().is_empty() {
        return Err(ApiError::bad("驳回必须填写原因"));
    }
    sqlx::query("UPDATE results SET confirmation='rejected' WHERE id=? AND confirmation='pending'")
        .bind(result_id)
        .execute(&s.db)
        .await?;
    audit(&s.db, id, "result.reject", &format!("驳回 result={result_id} 原因：{}", b.reason)).await;
    Ok(Json(json!({"state":"rejected","result_id":result_id})))
}

// ---------- 成绩更正（双人） ----------

#[derive(Deserialize)]
pub struct CorrectionIn {
    result_id: i64,
    new_value: f64,
    reason: String,
}

pub async fn request_correction(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Json(b): Json<CorrectionIn>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_role(&s, id, &["referee", "head_referee", "clerk"]).await?;
    if b.reason.trim().is_empty() {
        return Err(ApiError::bad("成绩更正必须说明原因"));
    }
    if !b.new_value.is_finite() || b.new_value <= 0.0 {
        return Err(ApiError::bad("更正后的成绩必须是正数"));
    }
    let r = sqlx::query("SELECT confirmation, published_value FROM results WHERE id=?")
        .bind(b.result_id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| ApiError::not_found("成绩不存在"))?;
    if r.get::<String,_>("confirmation") != "confirmed" {
        return Err(ApiError::bad("只能对已确认的成绩申请更正"));
    }
    let old = r.get::<f64,_>("published_value");
    if (old - b.new_value).abs() < 1e-9 {
        return Err(ApiError::bad("新成绩与当前成绩相同，无需更正"));
    }
    // 已存在待审核更正则不重复提交
    let dup = sqlx::query("SELECT id FROM corrections WHERE result_id=? AND status='pending'")
        .bind(b.result_id).fetch_optional(&s.db).await?;
    if dup.is_some() {
        return Err(ApiError::conflict("该成绩已有待审核的更正申请"));
    }
    let rec = sqlx::query(
        "INSERT INTO corrections(result_id, old_value, new_value, reason, requested_by)
         VALUES (?,?,?,?,?)",
    )
    .bind(b.result_id).bind(old).bind(b.new_value).bind(b.reason.trim()).bind(id)
    .execute(&s.db)
    .await?;
    audit(&s.db, id, "correction.request", &format!("更正申请 result={} {old}->{} 原因：{}", b.result_id, b.new_value, b.reason)).await;
    Ok((StatusCode::CREATED, Json(json!({"id": rec.last_insert_rowid(), "old_value": old, "new_value": b.new_value}))))
}

#[derive(Deserialize)]
pub struct ReviewCorrectionIn {
    decision: String, // approved | rejected
}

pub async fn review_correction(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Path(cid): Path<i64>,
    Json(b): Json<ReviewCorrectionIn>,
) -> ApiResult<Json<Value>> {
    require_role(&s, id, &["head_referee", "referee", "clerk"]).await?;
    if b.decision != "approved" && b.decision != "rejected" {
        return Err(ApiError::bad("decision 只能是 approved 或 rejected"));
    }
    let c = sqlx::query("SELECT result_id, old_value, new_value, requested_by, status FROM corrections WHERE id=?")
        .bind(cid)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| ApiError::not_found("更正申请不存在"))?;
    if c.get::<String,_>("status") != "pending" {
        return Err(ApiError::conflict("该更正申请已处理"));
    }
    let requester = c.get::<i64,_>("requested_by");
    if requester == id {
        return Err(ApiError::forbidden("成绩更正必须由另一人审核，不能审核自己提交的申请"));
    }
    let result_id = c.get::<i64,_>("result_id");
    let new_val = c.get::<f64,_>("new_value");

    let mut tx = s.db.begin().await?;
    if b.decision == "approved" {
        sqlx::query(
            "UPDATE results SET published_value=?, resolution='corrected' WHERE id=?",
        )
        .bind(new_val).bind(result_id)
        .execute(&mut *tx).await?;
    }
    sqlx::query(
        "UPDATE corrections SET status=?, reviewed_by=?, reviewed_at=datetime('now') WHERE id=?",
    )
    .bind(&b.decision).bind(id).bind(cid)
    .execute(&mut *tx).await?;
    tx.commit().await?;
    audit(&s.db, id, "correction.review", &format!("更正{cid} 审核结果={} 审核人≠申请人", b.decision)).await;
    Ok(Json(json!({"state": b.decision, "correction_id": cid, "result_id": result_id})))
}

pub async fn list_corrections(
    State(s): State<AppState>,
    AuthUser { id: _id }: AuthUser,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        "SELECT co.*, u1.display_name AS requester_name, u2.display_name AS reviewer_name,
                st.name AS student_name, e.name AS event_name, e.group_name
         FROM corrections co
         JOIN users u1 ON u1.id=co.requested_by
         LEFT JOIN users u2 ON u2.id=co.reviewed_by
         JOIN results res ON res.id=co.result_id
         JOIN checkins c ON c.id=res.checkin_id
         JOIN entries en ON en.id=c.entry_id
         JOIN students st ON st.id=en.student_id
         JOIN events e ON e.id=en.event_id
         ORDER BY co.id DESC",
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(json!(rows.iter().map(|r| json!({
        "id": r.get::<i64,_>("id"),
        "result_id": r.get::<i64,_>("result_id"),
        "old_value": r.get::<f64,_>("old_value"),
        "new_value": r.get::<f64,_>("new_value"),
        "reason": r.get::<String,_>("reason"),
        "requested_by": r.get::<i64,_>("requested_by"),
        "requester_name": r.get::<String,_>("requester_name"),
        "requested_at": r.get::<String,_>("requested_at"),
        "reviewed_by": r.get::<Option<i64>,_>("reviewed_by"),
        "reviewer_name": r.get::<Option<String>,_>("reviewer_name"),
        "reviewed_at": r.get::<Option<String>,_>("reviewed_at"),
        "status": r.get::<String,_>("status"),
        "student_name": r.get::<String,_>("student_name"),
        "event_name": r.get::<String,_>("event_name"),
        "group_name": r.get::<String,_>("group_name"),
    })).collect::<Vec<_>>())))
}

// ---------- 榜单 ----------

#[derive(Deserialize)]
pub struct PublishIn {
    note: Option<String>,
}

/// 发布新一轮榜单（只含已确认且到场的成绩）；旧版本永久保留
pub async fn publish_leaderboard(
    State(s): State<AppState>,
    AuthUser { id }: AuthUser,
    Path(round_id): Path<i64>,
    Json(b): Json<PublishIn>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_role(&s, id, &["head_referee"]).await?;
    let round = sqlx::query(
        "SELECT r.id, e.sort_rank FROM rounds r JOIN events e ON e.id=r.event_id WHERE r.id=?",
    )
    .bind(round_id)
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(|| ApiError::not_found("轮次不存在"))?;
    let direction = round.get::<i64,_>("sort_rank");

    // 已确认成绩（缺席者根本不会有 confirmed 成绩；rejected/pending 自动排除）
    let data = sqlx::query(
        "SELECT res.id AS result_id, en.student_id, res.published_value
         FROM results res
         JOIN checkins c ON c.id=res.checkin_id
         JOIN entries en ON en.id=c.entry_id
         WHERE c.round_id=? AND c.status='present'
           AND res.confirmation='confirmed' AND res.published_value IS NOT NULL",
    )
    .bind(round_id)
    .fetch_all(&s.db)
    .await?;
    if data.is_empty() {
        return Err(ApiError::bad("当前轮次还没有任何已确认成绩，不能发布榜单"));
    }
    let items = data.iter().map(|r| RankItem {
        student_id: r.get::<i64,_>("student_id"),
        result_id: r.get::<i64,_>("result_id"),
        value: r.get::<f64,_>("published_value"),
    }).collect();
    let ranked = rank(items, direction);

    let next_ver: i64 = sqlx::query("SELECT COALESCE(MAX(version),0)+1 FROM leaderboard_versions WHERE round_id=?")
        .bind(round_id)
        .fetch_one(&s.db)
        .await?
        .get(0);

    let mut tx = s.db.begin().await?;
    let v = sqlx::query(
        "INSERT INTO leaderboard_versions(round_id, version, published_by, note) VALUES (?,?,?,?)",
    )
    .bind(round_id).bind(next_ver).bind(id).bind(b.note.unwrap_or_default())
    .execute(&mut *tx).await?;
    let version_id = v.last_insert_rowid();
    for row in &ranked {
        sqlx::query(
            "INSERT INTO leaderboard_rows(version_id, rank, order_index, student_id, result_id, value)
             VALUES (?,?,?,?,?,?)",
        )
        .bind(version_id).bind(row.rank).bind(row.order_index)
        .bind(row.student_id).bind(row.result_id).bind(row.value)
        .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    audit(&s.db, id, "leaderboard.publish", &format!("发布榜单 round={round_id} v{next_ver} 共{}人", ranked.len())).await;
    Ok((StatusCode::CREATED, Json(json!({"version_id":version_id,"version":next_ver,"rows":ranked.len()}))))
}

async fn leaderboard_payload(db: &SqlitePool, version_id: i64) -> ApiResult<Value> {
    let head = sqlx::query(
        "SELECT v.id, v.version, v.published_at, v.note, v.round_id,
                ro.event_id, ro.name AS round_name, e.name AS event_name, e.group_name, e.unit,
                u.display_name AS publisher_name
         FROM leaderboard_versions v
         JOIN rounds ro ON ro.id=v.round_id
         JOIN events e ON e.id=ro.event_id
         LEFT JOIN users u ON u.id=v.published_by
         WHERE v.id=?",
    )
    .bind(version_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| ApiError::not_found("榜单版本不存在"))?;

    let rows = sqlx::query(
        "SELECT lr.rank, lr.order_index, lr.student_id, lr.result_id, lr.value,
                st.name AS student_name, st.bib_number, st.grade, st.class
         FROM leaderboard_rows lr JOIN students st ON st.id=lr.student_id
         WHERE lr.version_id=? ORDER BY lr.order_index",
    )
    .bind(version_id)
    .fetch_all(db)
    .await?;
    Ok(json!({
        "version_id": head.get::<i64,_>("id"),
        "version": head.get::<i64,_>("version"),
        "published_at": head.get::<String,_>("published_at"),
        "note": head.get::<String,_>("note"),
        "round_id": head.get::<i64,_>("round_id"),
        "round_name": head.get::<String,_>("round_name"),
        "event_name": head.get::<String,_>("event_name"),
        "group_name": head.get::<String,_>("group_name"),
        "unit": head.get::<String,_>("unit"),
        "publisher_name": head.get::<Option<String>,_>("publisher_name"),
        "rows": rows.iter().map(|r| json!({
            "rank": r.get::<i64,_>("rank"),
            "student_id": r.get::<i64,_>("student_id"),
            "student_name": r.get::<String,_>("student_name"),
            "bib_number": r.get::<String,_>("bib_number"),
            "grade": r.get::<i64,_>("grade"),
            "class": r.get::<String,_>("class"),
            "result_id": r.get::<i64,_>("result_id"),
            "value": r.get::<f64,_>("value"),
        })).collect::<Vec<_>>()
    }))
}

pub async fn get_leaderboard(
    State(s): State<AppState>,
    Path(version_id): Path<i64>,
) -> ApiResult<Json<Value>> {
    Ok(Json(leaderboard_payload(&s.db, version_id).await?))
}

pub async fn round_leaderboard_latest(
    State(s): State<AppState>,
    Path(round_id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let v = sqlx::query("SELECT id FROM leaderboard_versions WHERE round_id=? ORDER BY version DESC LIMIT 1")
        .bind(round_id)
        .fetch_optional(&s.db)
        .await?;
    match v {
        Some(row) => Ok(Json(leaderboard_payload(&s.db, row.get::<i64,_>(0)).await?)),
        None => Ok(Json(json!({"version": null, "round_id": round_id, "rows": []}))),
    }
}

pub async fn round_leaderboard_versions(
    State(s): State<AppState>,
    Path(round_id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        "SELECT v.id, v.version, v.published_at, v.note,
                u.display_name AS publisher_name,
                (SELECT COUNT(*) FROM leaderboard_rows lr WHERE lr.version_id=v.id) AS row_count
         FROM leaderboard_versions v LEFT JOIN users u ON u.id=v.published_by
         WHERE v.round_id=? ORDER BY v.version DESC",
    )
    .bind(round_id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(json!(rows.iter().map(|r| json!({
        "version_id": r.get::<i64,_>("id"),
        "version": r.get::<i64,_>("version"),
        "published_at": r.get::<String,_>("published_at"),
        "note": r.get::<String,_>("note"),
        "publisher_name": r.get::<Option<String>,_>("publisher_name"),
        "row_count": r.get::<i64,_>("row_count"),
    })).collect::<Vec<_>>())))
}

// ---------- 原始记录可追溯 ----------

pub async fn result_trace(
    State(s): State<AppState>,
    Path(result_id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let head = sqlx::query(
        "SELECT res.id, res.confirmation, res.published_value, res.resolution,
                res.confirmed_at, c.id AS checkin_id, c.status AS checkin_status,
                st.name AS student_name, st.bib_number,
                e.name AS event_name, e.group_name, e.unit, ro.name AS round_name
         FROM results res
         JOIN checkins c ON c.id=res.checkin_id
         JOIN entries en ON en.id=c.entry_id
         JOIN students st ON st.id=en.student_id
         JOIN rounds ro ON ro.id=c.round_id
         JOIN events e ON e.id=en.event_id
         WHERE res.id=?",
    )
    .bind(result_id)
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(|| ApiError::not_found("成绩不存在"))?;

    let checkin_id = head.get::<i64,_>("checkin_id");
    let raws = sqlx::query(
        "SELECT rr.source, rr.value, rr.note, rr.recorded_at, u.display_name AS recorder
         FROM raw_results rr LEFT JOIN users u ON u.id=rr.recorded_by
         WHERE rr.checkin_id=? ORDER BY rr.recorded_at, rr.source",
    )
    .bind(checkin_id)
    .fetch_all(&s.db)
    .await?;
    let corrs = sqlx::query(
        "SELECT co.id, co.old_value, co.new_value, co.reason, co.status, co.requested_at, co.reviewed_at,
                u1.display_name AS requester_name, u2.display_name AS reviewer_name
         FROM corrections co
         JOIN users u1 ON u1.id=co.requested_by
         LEFT JOIN users u2 ON u2.id=co.reviewed_by
         WHERE co.result_id=? ORDER BY co.id",
    )
    .bind(result_id)
    .fetch_all(&s.db)
    .await?;

    Ok(Json(json!({
        "result": {
            "id": head.get::<i64,_>("id"),
            "confirmation": head.get::<String,_>("confirmation"),
            "published_value": head.get::<Option<f64>,_>("published_value"),
            "resolution": head.get::<String,_>("resolution"),
            "confirmed_at": head.get::<Option<String>,_>("confirmed_at"),
            "student_name": head.get::<String,_>("student_name"),
            "bib_number": head.get::<String,_>("bib_number"),
            "event_name": head.get::<String,_>("event_name"),
            "group_name": head.get::<String,_>("group_name"),
            "unit": head.get::<String,_>("unit"),
            "round_name": head.get::<String,_>("round_name"),
            "checkin_status": head.get::<String,_>("checkin_status"),
        },
        "raw_results": raws.iter().map(|r| json!({
            "source": r.get::<String,_>("source"),
            "value": r.get::<f64,_>("value"),
            "note": r.get::<String,_>("note"),
            "recorded_at": r.get::<String,_>("recorded_at"),
            "recorder": r.get::<Option<String>,_>("recorder"),
        })).collect::<Vec<_>>(),
        "corrections": corrs.iter().map(|r| json!({
            "id": r.get::<i64,_>("id"),
            "old_value": r.get::<f64,_>("old_value"),
            "new_value": r.get::<f64,_>("new_value"),
            "reason": r.get::<String,_>("reason"),
            "status": r.get::<String,_>("status"),
            "requested_at": r.get::<String,_>("requested_at"),
            "reviewed_at": r.get::<Option<String>,_>("reviewed_at"),
            "requester_name": r.get::<String,_>("requester_name"),
            "reviewer_name": r.get::<Option<String>,_>("reviewer_name"),
        })).collect::<Vec<_>>()
    })))
}

// ---------- 工作台 ----------

pub async fn workbench(State(s): State<AppState>, AuthUser { id: _id }: AuthUser) -> ApiResult<Json<Value>> {
    let absentees = sqlx::query(
        "SELECT c.id AS checkin_id, c.round_id, ro.name AS round_name,
                e.name AS event_name, e.group_name, st.name AS student_name, st.bib_number, c.note
         FROM checkins c
         JOIN entries en ON en.id=c.entry_id
         JOIN students st ON st.id=en.student_id
         JOIN rounds ro ON ro.id=c.round_id
         JOIN events e ON e.id=en.event_id
         WHERE c.status='absent' ORDER BY c.id DESC",
    )
    .fetch_all(&s.db).await?;
    let pending = sqlx::query(
        "SELECT res.id AS result_id, c.round_id, ro.name AS round_name,
                e.name AS event_name, e.group_name, st.name AS student_name, st.bib_number,
                res.resolution, ro.is_reshoot,
                EXISTS(SELECT 1 FROM timing_disputes d WHERE d.result_id=res.id AND d.status='open') AS has_dispute
         FROM results res
         JOIN checkins c ON c.id=res.checkin_id
         JOIN entries en ON en.id=c.entry_id
         JOIN students st ON st.id=en.student_id
         JOIN rounds ro ON ro.id=c.round_id
         JOIN events e ON e.id=en.event_id
         WHERE res.confirmation='pending' ORDER BY res.id DESC",
    )
    .fetch_all(&s.db).await?;
    let corr = sqlx::query("SELECT COUNT(*) AS n FROM corrections WHERE status='pending'")
        .fetch_one(&s.db).await?;
    let dispute_n = sqlx::query("SELECT COUNT(*) AS n FROM timing_disputes WHERE status='open'")
        .fetch_one(&s.db).await?;
    let reshoot_rounds = sqlx::query(
        "SELECT r.id, r.name, e.name AS event_name, e.group_name,
                (SELECT COUNT(*) FROM checkins c JOIN entries en ON en.id=c.entry_id
                 WHERE c.round_id=r.id AND c.status='present') AS present_n
         FROM rounds r JOIN events e ON e.id=r.event_id WHERE r.is_reshoot=1 ORDER BY r.id",
    )
    .fetch_all(&s.db).await?;

    Ok(Json(json!({
        "counts": {
            "absentees": absentees.len(),
            "pending_results": pending.len(),
            "open_disputes": dispute_n.get::<i64,_>("n"),
            "pending_corrections": corr.get::<i64,_>("n"),
        },
        "absentees": absentees.iter().map(|r| json!({
            "checkin_id": r.get::<i64,_>("checkin_id"),
            "round_id": r.get::<i64,_>("round_id"),
            "round_name": r.get::<String,_>("round_name"),
            "event_name": r.get::<String,_>("event_name"),
            "group_name": r.get::<String,_>("group_name"),
            "student_name": r.get::<String,_>("student_name"),
            "bib_number": r.get::<String,_>("bib_number"),
            "note": r.get::<String,_>("note"),
        })).collect::<Vec<_>>(),
        "pending_results": pending.iter().map(|r| json!({
            "result_id": r.get::<i64,_>("result_id"),
            "round_id": r.get::<i64,_>("round_id"),
            "round_name": r.get::<String,_>("round_name"),
            "event_name": r.get::<String,_>("event_name"),
            "group_name": r.get::<String,_>("group_name"),
            "student_name": r.get::<String,_>("student_name"),
            "bib_number": r.get::<String,_>("bib_number"),
            "resolution": r.get::<String,_>("resolution"),
            "is_reshoot": r.get::<i64,_>("is_reshoot") == 1,
            "has_dispute": r.get::<bool,_>("has_dispute"),
        })).collect::<Vec<_>>(),
        "reshoot_rounds": reshoot_rounds.iter().map(|r| json!({
            "id": r.get::<i64,_>("id"),
            "name": r.get::<String,_>("round_name"),
            "event_name": r.get::<String,_>("event_name"),
            "group_name": r.get::<String,_>("group_name"),
            "present_n": r.get::<i64,_>("present_n"),
        })).collect::<Vec<_>>(),
    })))
}

// ---------- 公示屏 ----------

pub async fn public_board(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let rounds = sqlx::query(
        "SELECT r.id, r.name, r.is_reshoot, e.name AS event_name, e.group_name, e.unit,
                (SELECT MAX(version) FROM leaderboard_versions v WHERE v.round_id=r.id) AS latest_version
         FROM rounds r JOIN events e ON e.id=r.event_id
         ORDER BY e.id, r.id",
    )
    .fetch_all(&s.db).await?;

    let mut out = vec![];
    for r in rounds {
        let latest = r.get::<Option<i64>,_>("latest_version");
        let mut payload = json!({
            "round_id": r.get::<i64,_>("id"),
            "round_name": r.get::<String,_>("name"),
            "is_reshoot": r.get::<i64,_>("is_reshoot") == 1,
            "event_name": r.get::<String,_>("event_name"),
            "group_name": r.get::<String,_>("group_name"),
            "unit": r.get::<String,_>("unit"),
            "published": false,
            "rows": [],
        });
        if let Some(_ver) = latest {
            let vid: i64 = sqlx::query(
                "SELECT id FROM leaderboard_versions WHERE round_id=? ORDER BY version DESC LIMIT 1",
            )
            .bind(r.get::<i64,_>("id"))
            .fetch_one(&s.db).await?
            .get(0);
            let full = leaderboard_payload(&s.db, vid).await?;
            payload["published"] = json!(true);
            payload["version"] = full["version"].clone();
            payload["published_at"] = full["published_at"].clone();
            payload["rows"] = full["rows"].clone();
        }
        out.push(payload);
    }
    Ok(Json(json!({ "rounds": out })))
}

// ---------- 审计 ----------

pub async fn audit_list(
    State(s): State<AppState>,
    AuthUser { id: _id }: AuthUser,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        "SELECT a.id, a.action, a.detail, a.created_at, u.display_name AS actor
         FROM audit_log a LEFT JOIN users u ON u.id=a.actor_id
         ORDER BY a.id DESC LIMIT 200",
    )
    .fetch_all(&s.db).await?;
    Ok(Json(json!(rows.iter().map(|r| json!({
        "id": r.get::<i64,_>("id"),
        "action": r.get::<String,_>("action"),
        "detail": r.get::<String,_>("detail"),
        "created_at": r.get::<String,_>("created_at"),
        "actor": r.get::<Option<String>,_>("actor"),
    })).collect::<Vec<_>>())))
}

// ---------- 重置（演示/测试用） ----------

pub async fn reset_db(State(s): State<AppState>, AuthUser { id }: AuthUser) -> ApiResult<Json<Value>> {
    require_role(&s, id, &["head_referee"]).await?;
    crate::db::reset(&s.db).await?;
    audit(&s.db, id, "db.reset", "重置为演示数据").await;
    Ok(Json(json!({"state":"reset"})))
}
