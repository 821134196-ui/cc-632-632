#!/usr/bin/env bash
# 端到端验证脚本：
#   1) 重复检录被拒绝（同一学生同一轮次）
#   2) 缺席者不参与排名，且不能录入成绩
#   3) 电子计时与手记一致 → 自动确认；分歧 → 总裁判复核，系统不直接选一个发布
#   4) 漏记补录、补赛轮次结果必须经总裁判确认
#   5) 成绩更正必须填原因、由另一人审核（本人审核被拒）
#   6) 更正后重新发布榜单，并列名次按 1,1,3 处理，旧榜单与原始计时永久保留可查
#   7) 公示屏只出现已确认并已发布的成绩
set -u
cd "$(dirname "$0")"

PORT=18099
BASE="http://127.0.0.1:${PORT}"
TMPDB="$(mktemp -d)/verify.db"
BIN=./target/debug/sports-meet
PASS=0; FAIL=0

H_TEACHER="X-User-Id: 1"
H_CLERK="X-User-Id: 2"
H_REF="X-User-Id: 3"
H_HEAD="X-User-Id: 4"
JSON="Content-Type: application/json"

ok()   { PASS=$((PASS+1)); printf '  \033[32m✔\033[0m %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); printf '  \033[31m✘ %s\033[0m\n' "$1"; }

req() { # METHOD PATH USER JSON_BODY  -> prints body, sets HTTP_CODE
  local method="$1" path="$2" user="$3" body="${4:-}"
  if [ -n "$body" ]; then
    curl -s -w $'\n%{http_code}' -X "$method" "$BASE$path" -H "$user" -H "$JSON" -d "$body"
  else
    curl -s -w $'\n%{http_code}' -X "$method" "$BASE$path" -H "$user"
  fi
}
split() { # stdin -> BODY var and HTTP var
  local input; input="$(cat)"
  HTTP="${input##*$'\n'}"
  BODY="${input%$'\n'*}"
}
assert_code() { # expected desc
  if [ "$HTTP" = "$1" ]; then ok "$2 (HTTP $HTTP)"; else bad "$2（期望 $1，实际 $HTTP）: $BODY"; fi
}
assert_jq() { # jq_filter expected desc
  local got; got="$(printf '%s' "$BODY" | python3 -c "import json,sys; print($1)" 2>/dev/null)"
  if [ "$got" = "$2" ]; then ok "$3 (= $got)"; else bad "$3（期望 $2，实际 $got）"; fi
}

echo "▶ 构建后端…"
source "$HOME/.cargo/env" 2>/dev/null || true
cargo build 2>&1 | tail -1

echo "▶ 启动临时服务 (port=$PORT, db=$TMPDB)"
DATABASE_URL="sqlite:${TMPDB}?mode=rwc" STATIC_DIR=/tmp PORT=$PORT "$BIN" >/tmp/verify-server.log 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null || true' EXIT
for i in $(seq 1 50); do curl -s "$BASE/api/users" >/dev/null 2>&1 && break; sleep 0.2; done

# 种子：event1=60米 决赛(round1) entries 1..5；entry_id 1..5 对应学生 1001..1005
echo; echo "── 1. 重复检录必须被拒绝 ──"
split < <(req POST /api/checkins "$H_CLERK" '{"entry_id":1,"round_id":1,"status":"present"}')
assert_code 201 "首次检录到场成功"
split < <(req POST /api/checkins "$H_CLERK" '{"entry_id":1,"round_id":1,"status":"present"}')
assert_code 409 "同一学生同一轮次重复检录被拒绝"
split < <(req POST /api/checkins "$H_CLERK" '{"entry_id":1,"round_id":1,"status":"absent"}')
assert_code 409 "对已检录学生改状态也算重复检录"
split < <(req POST /api/checkins "$H_CLERK" '{"entry_id":1,"round_id":3,"status":"present"}')
assert_code 400 "跨项目检录（60米报名 → 跳远轮次）被拒绝"

echo; echo "── 2. 缺席者不参与排名、不能录成绩 ──"
split < <(req POST /api/checkins "$H_CLERK" '{"entry_id":2,"round_id":1,"status":"absent","note":"病假"}')
assert_code 201 "检录缺席成功"
for e in 3 4 5; do
  split < <(req POST /api/checkins "$H_CLERK" "{\"entry_id\":$e,\"round_id\":1,\"status\":\"present\"}")
  [ "$HTTP" = 201 ] || bad "entry $e 检录失败: $BODY"
done
split < <(req POST /api/timing "$H_REF" '{"checkin_id":2,"source":"electronic","value":10.1}')
assert_code 400 "缺席学生录入成绩被拒绝"

echo; echo "── 3. 电子/手记一致自动确认；分歧进入总裁判复核 ──"
split < <(req POST /api/timing "$H_REF" '{"checkin_id":3,"source":"electronic","value":10.20}')
assert_code 200 "录入电子计时"
split < <(req POST /api/timing "$H_REF" '{"checkin_id":3,"source":"manual","value":10.30}')
assert_code 200 "录入手记（差值 0.1s ≤ 容差 0.2s）"
assert_jq "json.load(sys.stdin)['state']" "auto_confirmed" "两源一致 → 自动确认，采用电子计时"
split < <(req POST /api/timing "$H_REF" '{"checkin_id":4,"source":"electronic","value":11.00}')
split < <(req POST /api/timing "$H_REF" '{"checkin_id":4,"source":"manual","value":11.50}')
assert_jq "json.load(sys.stdin)['state']" "dispute" "两源差 0.5s > 容差 → 产生分歧复核单"
# 分歧期间成绩为 pending，公示屏没有该项目榜单
split < <(req GET /api/public-board "$H_REF")
assert_jq "[r for r in json.load(sys.stdin)['rounds'] if r['round_id']==1][0]['published']" "False" "分歧未裁决前公示屏不发布该项目"
split < <(req GET /api/review/queue "$H_HEAD")
DISPUTE_ID="$(printf '%s' "$BODY" | python3 -c "import json,sys; print(json.load(sys.stdin)['disputes'][0]['dispute_id'])")"
split < <(req POST "/api/review/disputes/$DISPUTE_ID" "$H_HEAD" '{"chosen_value":12.0,"basis":"x"}')
assert_code 400 "裁决值不能是电子/手记之外的第三个数"
split < <(req POST "/api/review/disputes/$DISPUTE_ID" "$H_HEAD" '{"chosen_value":11.0,"basis":""}')
assert_code 400 "总裁判裁决必须填写依据"
split < <(req POST "/api/review/disputes/$DISPUTE_ID" "$H_HEAD" '{"chosen_value":11.00,"basis":"核对终点影像，电子计时有效"}')
assert_code 200 "总裁判裁决后成绩才确认（系统不自动二选一）"
assert_jq "json.load(sys.stdin)['resolution']" "electronic" "裁决记录采用来源 electronic"

echo; echo "── 4. 漏记补录必须经总裁判确认 ──"
split < <(req POST /api/results/missing "$H_REF" '{"checkin_id":5,"value":10.30,"note":"手记表漏抄，赛后补录"}')
assert_code 201 "裁判漏记补录提交成功"
MISSING_RID="$(printf '%s' "$BODY" | python3 -c "import json,sys; print(json.load(sys.stdin)['result_id'])")"
assert_jq "json.load(sys.stdin)['state']" "pending_missing" "漏记补录为待确认状态（不直接进榜单）"
split < <(req POST "/api/review/results/$MISSING_RID/confirm" "$H_HEAD" '{"note":"核实原始手记凭证"}')
assert_code 200 "总裁判确认漏记补录成绩"

echo; echo "── 5. 补赛轮次结果也必须确认，未确认不上公示屏 ──"
split < <(req POST /api/rounds "$H_CLERK" '{"event_id":1,"name":"补赛","is_reshoot":true}')
assert_code 201 "创建补赛轮次"
RESHOOT_RID_ROUND="$(printf '%s' "$BODY" | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")"
split < <(req POST /api/checkins "$H_CLERK" "{\"entry_id\":1,\"round_id\":$RESHOOT_RID_ROUND,\"status\":\"present\"}")
assert_code 201 "补赛轮次可再次检录（不同轮次不算重复）"
RESHOOT_CID="$(printf '%s' "$BODY" | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")"
split < <(req POST /api/timing "$H_REF" "{\"checkin_id\":$RESHOOT_CID,\"source\":\"manual\",\"value\":10.05,\"note\":\"补赛跑手记\"}")
assert_code 200 "补赛录入手记"
split < <(req GET /api/review/queue "$H_HEAD")
RESHOOT_RESULT="$(printf '%s' "$BODY" | python3 -c "
import json,sys
d=json.load(sys.stdin)
print([p['result_id'] for p in d['pending_results'] if p['is_reshoot']][0])")"
split < <(req POST "/api/review/results/$RESHOOT_RESULT/confirm" "$H_HEAD" '{"note":"补赛过程合规"}')
assert_code 200 "补赛成绩经总裁判确认"

echo; echo "── 6. 发布榜单 v1，缺席/未确认者不入围 ──"
split < <(req POST /api/rounds/1/leaderboard/publish "$H_HEAD" '{"note":"首轮公示"}')
assert_code 201 "发布 round1 榜单 v1"
split < <(req GET /api/rounds/1/leaderboard/latest "$H_HEAD")
V1_ID="$(printf '%s' "$BODY" | python3 -c "import json,sys; print(json.load(sys.stdin)['version_id'])")"
python3 - "$BODY" <<'PY'
import json,sys
d=json.loads(sys.argv[1])
rows=[(r['rank'],r['student_name'],r['value']) for r in d['rows']]
assert rows==[(1,'黄三',10.2),(2,'吴天',10.3),(3,'周武',11.0)], rows
assert not any(r['student_name']=='林二' for r in d['rows']), '缺席者林二不得上榜'
print("OK")
PY
[ "$?" = 0 ] && ok "v1 排名 黄三10.2 < 吴天10.3 < 周武11.0，缺席林二不在榜" || bad "v1 排名内容不正确"

echo; echo "── 7. 成绩更正：强制原因 + 双人审核 ──"
split < <(req POST /api/corrections "$H_REF" '{"result_id":2,"new_value":10.30,"reason":""}')
assert_code 400 "无原因的更正被拒绝"
split < <(req POST /api/corrections "$H_REF" '{"result_id":2,"new_value":10.30,"reason":"终点摄像复核，电子计时板误差0.1秒"}')
assert_code 201 "裁判提交更正申请（黄三 10.20→10.30）"
CORR_ID="$(printf '%s' "$BODY" | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")"
split < <(req POST "/api/corrections/$CORR_ID/review" "$H_REF" '{"decision":"approved"}')
assert_code 403 "申请人本人审核被拒绝（必须另一人）"
split < <(req POST "/api/corrections/$CORR_ID/review" "$H_CLERK" '{"decision":"approved"}')
assert_code 200 "另一人（检录员）审核通过"
# 公示屏在新版本发布前仍显示旧成绩
split < <(req GET /api/public-board "$H_REF")
python3 - "$BODY" <<'PY' && ok "新榜单发布前公示屏仍为旧值 10.2" || bad "公示屏提前暴露了未发布的更正"
import json,sys
r=[r for r in json.loads(sys.argv[1])['rounds'] if r['round_id']==1][0]
huang=[x for x in r['rows'] if x['student_name']=='黄三'][0]
assert r['version']==1 and huang['value']==10.2
PY

echo; echo "── 8. 重新发布 v2：并列名次 1,1,3；v1 与原始记录保留可查 ──"
split < <(req POST /api/rounds/1/leaderboard/publish "$H_HEAD" '{"note":"更正后重新公示"}')
assert_code 201 "发布榜单 v2"
split < <(req GET /api/rounds/1/leaderboard/latest "$H_HEAD")
python3 - "$BODY" <<'PY' && ok "v2 并列名次正确：黄三=吴天=10.3 并列第1，周武11.0 第3（1,1,3）" || bad "并列名次计算错误"
import json,sys
rows=[(r['rank'],r['student_name'],r['value']) for r in json.loads(sys.argv[1])['rows']]
# 同分并列者内部顺序无关，按(名次,值)比较，姓名只检查分组
assert sorted((rk,val) for rk,_,val in rows)==[(1,10.3),(1,10.3),(3,11.0)], rows
tied={name for rk,name,val in rows if rk==1}
assert tied=={'黄三','吴天'}, tied
assert [name for rk,name,_ in rows if rk==3]==['周武']
PY
split < <(req GET "/api/leaderboard/$V1_ID" "$H_HEAD")
python3 - "$BODY" <<'PY' && ok "旧榜单 v1 仍可查询，黄三仍为 10.2（历史快照保留）" || bad "旧榜单未保留"
import json,sys
d=json.loads(sys.argv[1])
assert d['version']==1
huang=[r for r in d['rows'] if r['student_name']=='黄三'][0]
assert huang['value']==10.2
PY
split < <(req GET /api/results/2/trace "$H_HEAD")
python3 - "$BODY" <<'PY' && ok "成绩溯源：原始电子10.20/手记10.30 原样保留，更正链（原因/两人/新旧值）可查" || bad "原始计时或更正链丢失"
import json,sys
d=json.loads(sys.argv[1])
raw={r['source']:r['value'] for r in d['raw_results']}
assert raw=={'electronic':10.2,'manual':10.3}, raw
co=d['corrections'][0]
assert co['old_value']==10.2 and co['new_value']==10.3 and co['status']=='approved'
assert co['requester_name']=='赵裁判' and co['reviewer_name']=='李检录' and co['reason']
PY

echo; echo "── 9. 公示屏最终状态：只显示已发布版本与已确认成绩 ──"
split < <(req GET /api/public-board "$H_REF")
BODY="$BODY" python3 - <<'PY' && ok "公示屏：round1 显示 v2；补赛轮次未发布 → published=false（确认≠发布）" || bad "公示屏状态错误"
import json,os
d=json.loads(os.environ['BODY'])
r1=[r for r in d['rounds'] if r['round_id']==1][0]
assert r1['published'] and r1['version']==2 and len(r1['rows'])==3
assert all(x['student_name']!='林二' for x in r1['rows'])
rs=[r for r in d['rounds'] if r['is_reshoot']][0]
assert rs['published']==False and rs['rows']==[]
PY
split < <(req POST "/api/rounds/$RESHOOT_RID_ROUND/leaderboard/publish" "$H_HEAD" '{"note":"补赛公示"}')
assert_code 201 "补赛轮次确认后可单独发布"

echo; echo "── 10. 无已确认成绩时禁止发布空榜单 ──"
split < <(req POST /api/rounds/2/leaderboard/publish "$H_HEAD" '{}')
assert_code 400 "全部未检录/未确认的轮次不允许发布"

echo; echo "── 11. 田赛排名方向：距离越大越好，并列同样按 1,1,3 ──"
# event3=跳远 round3，entries 10..13（学生 3001..3004）
cid=100
for e in 10 11 12 13; do
  split < <(req POST /api/checkins "$H_CLERK" "{\"entry_id\":$e,\"round_id\":3,\"status\":\"present\"}")
  [ "$HTTP" = 201 ] || bad "跳远 entry $e 检录失败: $BODY"
done
# checkin 主键自增，直接从花名册取 checkin_id 与期望成绩配对
FIELD_JSON="$(req GET /api/rounds/3/roster "$H_CLERK" | head -n -1)"
i=0
for v in 3.10 3.50 3.50 2.90; do
  CID="$(printf '%s' "$FIELD_JSON" | python3 -c "
import json,sys
d=json.load(sys.stdin)['roster']
print([r['checkin_id'] for r in d if r['checkin_status']=='present'][ $i ])")"
  req POST /api/timing "$H_REF" "{\"checkin_id\":$CID,\"source\":\"electronic\",\"value\":$v}" >/dev/null
  split < <(req POST /api/timing "$H_REF" "{\"checkin_id\":$CID,\"source\":\"manual\",\"value\":$v}")
  [ "$HTTP" = 200 ] || bad "跳远计时失败: $BODY"
  i=$((i+1))
done
split < <(req POST /api/rounds/3/leaderboard/publish "$H_HEAD" '{"note":"跳远榜单"}')
assert_code 201 "发布跳远榜单"
split < <(req GET /api/rounds/3/leaderboard/latest "$H_HEAD")
python3 - "$BODY" <<'PY' && ok "田赛 3.50 并列第1、3.10 第3、2.90 第4（距离越大越好）" || bad "田赛排名方向或并列错误"
import json,sys
rows=[(r['rank'],r['value']) for r in json.loads(sys.argv[1])['rows']]
assert rows==[(1,3.5),(1,3.5),(3,3.1),(4,2.9)], rows
PY

echo
echo "============================================="
echo "  通过: $PASS   失败: $FAIL"
echo "============================================="
[ "$FAIL" = 0 ]
