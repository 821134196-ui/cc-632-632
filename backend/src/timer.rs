use std::time::{SystemTime, UNIX_EPOCH};

/// 本地模拟的电子计时/测量设备。
/// 现实中对应连接到跑道计时器或激光测距仪，这里用时间熵生成可信读数。
pub struct ElectronicTimer;

impl ElectronicTimer {
    /// 生成一次电子读数。
    /// - 径赛（秒）：12.5 ~ 16.5 秒，精度 0.01
    /// - 田赛（米）：3.20 ~ 5.80 米，精度 0.01
    pub fn read(higher_better: bool) -> f64 {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0)
            .wrapping_add(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            );
        // 混入线程本地不可预测量，避免连续两次读数相同
        let entropy = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let frac = (entropy % 10_000) as f64 / 10_000.0;

        let raw = if higher_better {
            3.20 + frac * 2.60
        } else {
            12.50 + frac * 4.00
        };
        (raw * 100.0).round() / 100.0
    }
}
