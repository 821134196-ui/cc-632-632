use serde::Serialize;

#[derive(Debug, Clone)]
pub struct RankItem {
    pub student_id: i64,
    pub result_id: i64,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RankedRow {
    pub order_index: i64,
    pub rank: i64,
    pub student_id: i64,
    pub result_id: i64,
    pub value: f64,
}

/// 计算并列名次（竞赛排名 1,1,3 规则）：
/// 成绩相同者同名次，下一名次按实际位置顺延。
/// direction = 1  数值越小越好（径赛）；-1 数值越大越好（田赛）
pub fn rank(items: Vec<RankItem>, direction: i64) -> Vec<RankedRow> {
    let mut v = items;
    v.sort_by(|a, b| {
        (a.value * direction as f64)
            .partial_cmp(&(b.value * direction as f64))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out = Vec::with_capacity(v.len());
    let mut cur_rank: i64 = 1;
    for (i, item) in v.iter().enumerate() {
        if i > 0 && (item.value - v[i - 1].value).abs() > 1e-9 {
            cur_rank = i as i64 + 1;
        }
        out.push(RankedRow {
            order_index: i as i64,
            rank: cur_rank,
            student_id: item.student_id,
            result_id: item.result_id,
            value: item.value,
        });
    }
    out
}
