use serde::Serialize;

#[derive(Debug, Clone)]
pub struct RankItem {
    pub registration_id: i64,
    pub student_name: String,
    pub grade: String,
    pub class: String,
    pub bib: String,
    pub score: f64,
    pub makeup: bool,
}

#[derive(Debug, Serialize)]
pub struct RankedItem {
    pub registration_id: i64,
    pub student_name: String,
    pub grade: String,
    pub class: String,
    pub bib: String,
    pub score: f64,
    pub rank: i64,
    pub tied: bool,
    pub makeup: bool,
}

/// 计算名次（标准竞赛排名 1-2-2-4）：
/// 成绩相同者并列同名次，后续名次顺延跳过；缺席者不在输入中，即不参与排名。
pub fn rank(items: Vec<RankItem>, higher_better: bool) -> Vec<RankedItem> {
    let mut sorted: Vec<RankItem> = items;
    sorted.sort_by(|a, b| {
        if higher_better {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    });

    // 统计每个成绩的出现次数，用于标记并列
    let mut counts = std::collections::HashMap::new();
    for it in &sorted {
        *counts.entry(score_key(it.score)).or_insert(0u32) += 1;
    }

    let mut out = Vec::with_capacity(sorted.len());
    let mut prev: Option<f64> = None;
    for (idx, it) in sorted.iter().enumerate() {
        // 名次默认等于位置（1 起）；与上一名成绩相同时继承其名次 => 1-1-3
        let mut r = idx as i64 + 1;
        if let Some(p) = prev {
            if (it.score - p).abs() < f64::EPSILON {
                r = out.last().map(|x: &RankedItem| x.rank).unwrap_or(1);
            }
        }
        let tied = counts.get(&score_key(it.score)).copied().unwrap_or(0) > 1;
        out.push(RankedItem {
            registration_id: it.registration_id,
            student_name: it.student_name.clone(),
            grade: it.grade.clone(),
            class: it.class.clone(),
            bib: it.bib.clone(),
            score: it.score,
            rank: r,
            tied,
            makeup: it.makeup,
        });
        prev = Some(it.score);
    }
    out
}

/// 用固定精度把浮点成绩变成可比较的键（0.01 精度，规避二进制误差）
fn score_key(v: f64) -> i64 {
    (v * 100.0).round() as i64
}
