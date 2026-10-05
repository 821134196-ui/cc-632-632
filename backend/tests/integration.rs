use axum::body::Body;
use axum::Router;
use http::Request;
use http::StatusCode;
use http_body_util::BodyExt;
use meet_server::app;
use meet_server::db;
use serde_json::{json, Value};
use tower::ServiceExt;

fn post_json(path: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    req.body(Body::from(body.to_string())).unwrap()
}

fn get(path: &str, token: Option<&str>) -> Request<Body> {
    let mut req = Request::builder().method("GET").uri(path);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    req.body(Body::empty()).unwrap()
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn login(app: &Router, username: &str) -> String {
    let (status, v) = send(
        app,
        post_json(
            "/api/auth/login",
            None,
            json!({ "username": username, "password": "123456" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "登录失败: {v}");
    v["token"].as_str().unwrap().to_string()
}

async fn fresh_app() -> Router {
    let pool = db::init_memory().await;
    app(pool)
}

/// 场景一：同一学生在同一轮次不能重复检录
#[tokio::test]
async fn duplicate_checkin_is_rejected() {
    let app = fresh_app().await;
    let checker = login(&app, "checker").await;

    // 第一次检录：到场
    let (s, v) = send(
        &app,
        post_json(
            "/api/checkins",
            Some(&checker),
            json!({ "registration_id": 1, "status": "present" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "首次检录应成功: {v}");

    // 第二次检录：必须被拒绝（409）
    let (s, v) = send(
        &app,
        post_json(
            "/api/checkins",
            Some(&checker),
            json!({ "registration_id": 1, "status": "present" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "重复检录应返回 409，实际: {s} {v}");
    assert!(
        v["error"].as_str().unwrap_or("").contains("重复检录"),
        "错误信息应说明重复检录: {v}"
    );

    // 换状态再试同样被拒绝
    let (s, _) = send(
        &app,
        post_json(
            "/api/checkins",
            Some(&checker),
            json!({ "registration_id": 1, "status": "absent" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // 非检录员无权操作
    let teacher = login(&app, "teacher").await;
    let (s, _) = send(
        &app,
        post_json(
            "/api/checkins",
            Some(&teacher),
            json!({ "registration_id": 2, "status": "present" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

/// 场景二：缺席 → 补赛 → 漏记补录 → 电子/手记不一致由总裁判复核
#[tokio::test]
async fn absent_makeup_late_and_dispute_review() {
    let app = fresh_app().await;
    let checker = login(&app, "checker").await;
    let referee = login(&app, "referee").await;
    let chief = login(&app, "chief").await;

    // 学生2 检录为缺席
    let (s, _) = send(
        &app,
        post_json(
            "/api/checkins",
            Some(&checker),
            json!({ "registration_id": 2, "status": "absent", "note": "未按时到场" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // 工作台突出缺席
    let (_, wb) = send(&app, get("/api/workbench", Some(&checker))).await;
    assert!(
        wb["absent"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["registration_id"] == 2),
        "工作台应列出缺席项目: {wb}"
    );
    let checkin_id = wb["absent"][0]["checkin_id"].as_i64().unwrap();

    // 缺席状态不能直接录成绩
    let (s, v) = send(
        &app,
        post_json(
            "/api/results/manual",
            Some(&referee),
            json!({ "registration_id": 2, "value": 14.2, "late": true }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "缺席者录成绩应被拒绝: {v}");

    // 办理补赛检录
    let (s, v) = send(
        &app,
        post_json(
            &format!("/api/checkins/{checkin_id}/makeup"),
            Some(&checker),
            json!({}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "补赛检录应成功: {v}");

    // 补赛后仍不能重复普通检录
    let (s, _) = send(
        &app,
        post_json(
            "/api/checkins",
            Some(&checker),
            json!({ "registration_id": 2, "status": "present" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // 漏记补录：手记 20.00 与电子计时（12.5~16.5）必然不一致
    let (s, v) = send(
        &app,
        post_json(
            "/api/results/manual",
            Some(&referee),
            json!({ "registration_id": 2, "value": 20.0, "late": true, "note": "漏记补录" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "补赛后可补录成绩: {v}");
    let (s, v) = send(
        &app,
        post_json(
            "/api/results/electronic",
            Some(&referee),
            json!({ "registration_id": 2 }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "电子计时录入失败: {v}");
    let result_id = v["result_id"].as_i64().unwrap();

    // 状态应为待复核，裁判无权直接确认
    let (_, roster) = send(&app, get("/api/rounds/1/roster", Some(&referee))).await;
    let entry = roster["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["registration_id"] == 2)
        .unwrap();
    assert_eq!(entry["review_status"], "pending", "电子/手记不一致应待复核");
    assert!(entry["electronic"].as_f64().unwrap() < 17.0);
    assert_eq!(entry["manual"], 20.0, "原始手记记录必须保留");

    let (s, v) = send(
        &app,
        post_json(&format!("/api/results/{result_id}/confirm"), Some(&referee), json!({})),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::CONFLICT,
        "裁判不能绕过复核直接确认争议成绩: {v}"
    );

    // 裁判不能充当总裁判复核
    let (s, _) = send(
        &app,
        post_json(
            &format!("/api/results/{result_id}/resolve"),
            Some(&referee),
            json!({ "final_value": 14.25, "note": "手记笔误，以电子为准" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // 复核必须说明依据
    let (s, _) = send(
        &app,
        post_json(
            &format!("/api/results/{result_id}/resolve"),
            Some(&chief),
            json!({ "final_value": 14.25, "note": "   " }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // 总裁判复核认定 14.25（人工认定，不是系统二选一）
    let (s, v) = send(
        &app,
        post_json(
            &format!("/api/results/{result_id}/resolve"),
            Some(&chief),
            json!({ "final_value": 14.25, "note": "回看录像，手记多记，认定14.25" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "总裁判复核应成功: {v}");
    assert_eq!(v["review_status"], "resolved");

    // 原始电子/手记记录仍在，最终成绩为人认定值
    let (_, roster) = send(&app, get("/api/rounds/1/roster", Some(&chief))).await;
    let entry = roster["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["registration_id"] == 2)
        .unwrap();
    assert_eq!(entry["manual"], 20.0);
    assert!(entry["electronic"].as_f64().is_some());
    assert_eq!(entry["final_value"], 14.25);
    assert_eq!(entry["late"], 1);

    // 工作台漏记列表包含该成绩
    let (_, wb) = send(&app, get("/api/workbench", Some(&chief))).await;
    assert!(
        wb["late"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["result_id"] == result_id),
        "漏记补录应在工作台突出显示: {wb}"
    );

    // 未发布前公示屏没有任何该轮次数据
    let (s, screen) = send(&app, get("/public/screen", None)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        screen["boards"].as_array().unwrap().is_empty(),
        "未确认/未发布的数据不能出现在公示屏"
    );
}

/// 场景三：成绩更正双人审核 → 榜单更新、旧版保留、并列名次正确
#[tokio::test]
async fn correction_updates_board_and_keeps_history_with_ties() {
    let app = fresh_app().await;
    let checker = login(&app, "checker").await;
    let referee = login(&app, "referee").await;
    let referee2 = login(&app, "referee2").await;
    let chief = login(&app, "chief").await;

    // 1、3、4 到场，2 缺席（不参与排名）
    for reg in [1i64, 3, 4] {
        let (s, _) = send(
            &app,
            post_json(
                "/api/checkins",
                Some(&checker),
                json!({ "registration_id": reg, "status": "present" }),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
    let (s, _) = send(
        &app,
        post_json(
            "/api/checkins",
            Some(&checker),
            json!({ "registration_id": 2, "status": "absent" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // 手记成绩：1 与 3 并列 13.50，4 为 14.00（单一来源，裁判显式确认）
    let scores = [(1i64, 13.5), (3i64, 13.5), (4i64, 14.0)];
    let mut result_ids = std::collections::HashMap::new();
    for (reg, val) in scores {
        let (_, v) = send(
            &app,
            post_json(
                "/api/results/manual",
                Some(&referee),
                json!({ "registration_id": reg, "value": val }),
            ),
        )
        .await;
        let rid = v["result_id"].as_i64().unwrap();
        result_ids.insert(reg, rid);
        let (s, v) = send(
            &app,
            post_json(
                &format!("/api/results/{rid}/confirm"),
                Some(&referee),
                json!({ "use_single_source": "manual" }),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "确认成绩失败: {v}");
    }

    // 发布第一版
    let (s, v1) = send(
        &app,
        post_json("/api/rounds/1/publish", Some(&chief), json!({})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "发布失败: {v1}");
    assert_eq!(v1["version"], 1);
    let entries = v1["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "缺席者不参与排名");
    let find = |bib: &str| {
        entries
            .iter()
            .find(|e| e["bib"] == bib)
            .unwrap()
            .clone()
    };
    assert_eq!(find("0301")["rank"], 1);
    assert_eq!(find("0301")["tied"], true);
    assert_eq!(find("0303")["rank"], 1);
    assert_eq!(find("0303")["tied"], true);
    assert_eq!(find("0304")["rank"], 3, "并列后名次应顺延（1-1-3）");

    // 公示屏只显示已发布榜单
    let (_, screen) = send(&app, get("/public/screen", None)).await;
    let board = &screen["boards"][0];
    assert_eq!(board["entries"].as_array().unwrap().len(), 3);

    // 发起更正：0301 改为 14.50
    let rid1 = result_ids[&1];
    let (s, _) = send(
        &app,
        post_json(
            &format!("/api/results/{rid1}/corrections"),
            Some(&referee),
            json!({ "new_value": 14.5, "reason": "" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "更正必须填写原因");

    let (s, v) = send(
        &app,
        post_json(
            &format!("/api/results/{rid1}/corrections"),
            Some(&referee),
            json!({ "new_value": 14.5, "reason": "手记抄录错误，核对原始卡片" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "更正申请应成功: {v}");
    let correction_id = v["id"].as_i64().unwrap();

    // 不能审核本人发起的更正
    let (s, v) = send(
        &app,
        post_json(
            &format!("/api/corrections/{correction_id}/review"),
            Some(&referee),
            json!({ "decision": "approved" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "必须由另一人审核: {v}");

    // 另一裁判审核通过
    let (s, v) = send(
        &app,
        post_json(
            &format!("/api/corrections/{correction_id}/review"),
            Some(&referee2),
            json!({ "decision": "approved", "review_note": "已核对原始卡片" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "他人审核应通过: {v}");

    // 更正生效：最终成绩改变，但原始手记记录保留
    let (_, roster) = send(&app, get("/api/rounds/1/roster", Some(&chief))).await;
    let e1 = roster["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["registration_id"] == 1)
        .unwrap();
    assert_eq!(e1["final_value"], 14.5);
    assert_eq!(e1["manual"], 13.5, "原始计时记录不得被覆盖");

    // 公示屏在新版发布前仍显示旧版
    let (_, screen) = send(&app, get("/public/screen", None)).await;
    assert_eq!(screen["boards"][0]["board"]["version"], 1);

    // 发布第二版：0303 第1，0304 第2，0301 第3
    let (s, v2) = send(
        &app,
        post_json("/api/rounds/1/publish", Some(&chief), json!({})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "二次发布失败: {v2}");
    assert_eq!(v2["version"], 2);
    let entries2 = v2["entries"].as_array().unwrap();
    let find2 = |bib: &str| {
        entries2
            .iter()
            .find(|e| e["bib"] == bib)
            .unwrap()
            .clone()
    };
    assert_eq!(find2("0303")["rank"], 1);
    assert_eq!(find2("0303")["score"], 13.5);
    assert_eq!(find2("0304")["rank"], 2);
    assert_eq!(find2("0301")["rank"], 3);
    assert_eq!(find2("0301")["score"], 14.5);

    // 公示屏已更新到第二版
    let (_, screen) = send(&app, get("/public/screen", None)).await;
    assert_eq!(screen["boards"][0]["board"]["version"], 2);

    // 旧榜单永久保留且内容为更正前快照
    let (_, pubs) = send(&app, get("/api/rounds/1/publications", Some(&chief))).await;
    assert_eq!(pubs["publications"].as_array().unwrap().len(), 2);
    let v1_id = pubs["publications"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["version"] == 1)
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let (s, old) = send(
        &app,
        get(&format!("/api/publications/{v1_id}"), Some(&chief)),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let old_entries = old["entries"].as_array().unwrap();
    let old_0301 = old_entries
        .iter()
        .find(|e| e["bib"] == "0301")
        .unwrap();
    assert_eq!(old_0301["score"], 13.5, "历史榜单保留更正前成绩");
    assert_eq!(old_0301["rank"], 1, "历史榜单保留更正前名次");

    // 未确认成绩不会推到公示屏：给缺席者办理补赛、录成绩但不确认
    let (_, wb) = send(&app, get("/api/workbench", Some(&checker))).await;
    let ck2 = wb["absent"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["registration_id"] == 2)
        .unwrap()["checkin_id"]
        .as_i64()
        .unwrap();
    let (s, _) = send(
        &app,
        post_json(
            &format!("/api/checkins/{ck2}/makeup"),
            Some(&checker),
            json!({}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(
        &app,
        post_json(
            "/api/results/manual",
            Some(&referee),
            json!({ "registration_id": 2, "value": 12.88, "late": true }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    // 再发一版：未确认的 12.88 不能上榜
    let (s, v3) = send(
        &app,
        post_json("/api/rounds/1/publish", Some(&chief), json!({})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "发布失败: {v3}");
    assert_eq!(v3["version"], 3);
    assert_eq!(
        v3["entries"].as_array().unwrap().len(),
        3,
        "未确认的补赛成绩不得进入榜单: {v3}"
    );
    assert!(
        v3["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["bib"] != "0302"),
        "未确认者不应出现"
    );

    // 更正记录全程可查
    let (_, corrections) = send(&app, get("/api/corrections", Some(&chief))).await;
    let c = corrections["corrections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == correction_id)
        .unwrap();
    assert_eq!(c["status"], "approved");
    assert_eq!(c["old_value"], 13.5);
    assert_eq!(c["new_value"], 14.5);
    assert_eq!(c["requester_name"], "张裁判");
    assert_eq!(c["reviewer_name"], "刘裁判");
}
