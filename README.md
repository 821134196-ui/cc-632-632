# 小学运动会检录与成绩更正系统

现场检录、成绩录入、争议复核、成绩更正、榜单发布与公示一体化系统。

- **后端**：Rust + Axum + SQLx + SQLite
- **前端**：Svelte 4 + Vite
- **电子计时**：本地模拟（`backend/src/timer.rs`），读数与手记分别保存、永不覆盖
- **启动**：`./run.sh` 一条命令完成前端构建与后端启动

## 快速开始

```bash
./run.sh
# 打开 http://localhost:8080
# 公示屏（无需登录）：http://localhost:8080/#/screen
```

首次启动自动建库建表并写入演示数据（学生 / 项目 / 报名），数据库文件在 `data/meet.db`。
删除该文件可重置全部数据。

演示账号（密码均为 `123456`）：

| 账号 | 角色 | 职责 |
| --- | --- | --- |
| `teacher` | 王老师 | 学生报名 |
| `checker` | 李检录 | 到场/缺席检录、补赛 |
| `referee` | 张裁判 | 电子/手记成绩录入、确认、发起更正 |
| `referee2` | 刘裁判 | 审核他人发起的更正（双人控制） |
| `chief` | 总裁判 | 电子/手记争议复核、发布榜单 |

## 业务规则（已由集成测试覆盖）

1. **报名唯一**：同一学生在同一轮次只能报名一次（`UNIQUE(round_id, student_id)`）。
2. **检录唯一**：同一学生在同一轮次只有一条检录记录，重复检录返回 `409`；
   缺席者经「补赛检录」转为到场（标记 makeup），不算重复检录。
3. **缺席不排名**：排名与发布只纳入检录到场（含补赛）且成绩已确认的学生。
4. **双计时源**：电子计时与手记分别持久化。两者一致（误差 ≤0.005）由裁判确认；
   **不一致时系统不自动选择任何一方**，状态置为 `pending`，由总裁判人工认定最终成绩
   并必须填写依据；两份原始读数永久保留。
5. **漏记/补赛**：缺席→补赛→漏记补录（late 标记）的成绩同样必须经过确认/复核才能进入榜单。
6. **成绩更正**：仅已确认成绩可发起，必须填写原因；必须由**另一人**审核（服务端拒绝本人审核）。
   更正只改最终成绩，不改电子/手记原始记录。
7. **榜单版本化**：每次发布生成新版本，旧榜单以快照形式永久保留；公示屏只显示
   各轮次**最新已发布版本**，未确认数据永远不会出现在公示屏。
8. **并列名次**：采用标准竞赛排名（1-1-3），并列者标记 `tied`。

## API 概览

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| POST | `/api/auth/login` | 登录 |
| POST | `/api/registrations` | 报名（老师） |
| GET | `/api/rounds/{id}/roster` | 轮次名单（含检录/成绩状态） |
| POST | `/api/checkins` | 检录（检录员，重复 → 409） |
| POST | `/api/checkins/{id}/makeup` | 缺席转补赛 |
| POST | `/api/results/electronic` | 读取模拟电子计时 |
| POST | `/api/results/manual` | 录入手记（可标记漏记） |
| POST | `/api/results/{id}/confirm` | 裁判确认 |
| POST | `/api/results/{id}/resolve` | 总裁判争议复核（须填依据） |
| POST | `/api/results/{id}/corrections` | 发起更正（须填原因） |
| POST | `/api/corrections/{id}/review` | 另一人审核更正 |
| GET | `/api/rounds/{id}/ranking` | 已确认成绩实时排名（预览） |
| POST | `/api/rounds/{id}/publish` | 发布新版榜单（总裁判） |
| GET | `/api/rounds/{id}/publications` | 历史版本列表 |
| GET | `/api/publications/{id}` | 历史榜单快照 |
| GET | `/api/workbench` | 工作台：缺席/补赛/待复核/待审核更正/漏记/未确认 |
| GET | `/public/screen` | 公示屏数据（无需登录） |

## 测试

```bash
cd backend && cargo test
```

三个集成测试对应验收点：

- `duplicate_checkin_is_rejected`：重复检录被拒绝（409），含角色权限校验。
- `absent_makeup_late_and_dispute_review`：缺席不能录成绩 → 补赛 → 漏记补录 →
  电子/手记不一致须总裁判复核（裁判无权确认、复核须填依据、原始记录保留）。
- `correction_updates_board_and_keeps_history_with_ties`：并列名次 1-1-3、
  更正双人审核（本人被拒、原因必填）、更正后重新发布榜单更新、旧榜单快照可查、
  未确认补赛成绩不进入榜单/公示屏。

## 目录结构

```
backend/        Rust/Axum/SQLx 服务
  migrations/   SQLite 建表脚本（启动时自动执行）
  src/          auth/db/handlers/ranking/timer/error
  tests/        端到端集成测试（内存 SQLite）
frontend/       Svelte 4 前端
  src/pages/    Login/Workbench/Register/RoundPage/Corrections/Publications/Screen
run.sh          一键构建并启动
```
