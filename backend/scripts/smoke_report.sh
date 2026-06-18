#!/usr/bin/env bash
# M4 报表片A 端到端 smoke：自起/自停 rise-server + psql 灌 usage_logs + RLS/白名单/报表 CRUD 断言。
#
# 验证内核：
#  - 数据集列表/详情按权限可见；
#  - 查询引擎白名单（未知指标/维度 400、空指标 400）；
#  - **行级隔离(RLS)**：customer 仅见本组织（org_id 注入），admin/finance 全量；
#  - 角色无 rls_rule 分支 → 403（sales 查 usage）；
#  - 报表定义 CRUD + report.define 权限门禁。
#
# 测试行用 2099 年时间戳 + 时间窗查询隔离，避免共享库既有 usage_logs 干扰计数。
# 前置：PostgreSQL 已起且迁移到位（make infra-up && make migrate）；容器名 rise-postgres。
# 用法：bash backend/scripts/smoke_report.sh   （退出码 0=全过，1=有失败）
# 依赖：curl、jq、docker（psql 灌数）。
set -uo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
BACKEND=$(cd "$SCRIPT_DIR/.." && pwd)

PORT=8088
BASE="http://127.0.0.1:${PORT}"
TOK="smoke-rpt-$$-${RANDOM}"
SEC="smoke-jwt-$$-${RANDOM}"
DBURL=${RR_DATABASE_URL:-${DATABASE_URL:-postgres://rise:rise@localhost:5432/rise_router}}
PGC=${RISE_PG_CONTAINER:-rise-postgres}
LOG=$(mktemp -t rise_smoke_report.XXXXXX.log)
T0="2099-06-01T00:00:00Z"   # 测试行时间窗起
T1="2099-07-01T00:00:00Z"   # 止

PASS=0
FAIL=0
SERVER_PID=""

cleanup() {
  # 清理测试行（2099 行）+ 测试报表定义
  docker exec "$PGC" psql -U rise -d rise_router -tAc \
    "DELETE FROM usage_logs WHERE created_at >= '2099-01-01';" >/dev/null 2>&1
  [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
}
trap cleanup EXIT

req() {
  local method=$1 path=$2 auth=$3 data=$4
  local h=(-H 'Content-Type: application/json')
  [ -n "$auth" ] && h+=(-H "$auth")
  local out
  if [ -n "$data" ]; then
    out=$(curl -s -w $'\n%{http_code}' -X "$method" "${h[@]}" -d "$data" "$BASE$path")
  else
    out=$(curl -s -w $'\n%{http_code}' -X "$method" "${h[@]}" "$BASE$path")
  fi
  CODE=${out##*$'\n'}
  BODY=${out%$'\n'*}
  [ "$BODY" = "$CODE" ] && BODY=""
}

ok()  { PASS=$((PASS+1)); printf '  \033[32m✓\033[0m %s\n' "$1"; }
ng()  { FAIL=$((FAIL+1)); printf '  \033[31m✗\033[0m %s  (CODE=%s BODY=%s)\n' "$1" "$CODE" "${BODY:0:160}"; }
expect() { if [ "$CODE" = "$1" ]; then ok "$2"; else ng "$2"; fi; }
jqv() { printf '%s' "$BODY" | jq -r "$1" 2>/dev/null; }
AUTH_ADMIN="X-Admin-Token: $TOK"
authj() { echo "Authorization: Bearer $1"; }
pg() { docker exec "$PGC" psql -U rise -d rise_router -tAc "$1"; }

# ============ 1. 编译 + 起 server ============
echo "▶ 编译 rise-server ..."
( cd "$BACKEND" && cargo build -q -p rise-server ) || { echo "✗ 编译失败"; exit 1; }

req GET /healthz "" ""
[ "$CODE" = "200" ] && { echo "✗ 端口 ${PORT} 已被占用"; exit 1; }

echo "▶ 启动 server（端口 ${PORT}）..."
( cd "$BACKEND" && exec env RR_ADMIN_TOKEN="$TOK" RR_JWT_SECRET="$SEC" RR_DATABASE_URL="$DBURL" \
    RR_LOG_LEVEL=warn RR_BIND_ADDR="0.0.0.0:${PORT}" ./target/debug/rise-server ) >"$LOG" 2>&1 &
SERVER_PID=$!

echo "▶ 等待 /readyz + seed ..."
ready=0
for _ in $(seq 1 60); do
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then echo "✗ server 退出"; cat "$LOG"; exit 1; fi
  req GET /api/report/datasets/usage "$AUTH_ADMIN" ""
  if [ "$CODE" = "200" ]; then ready=1; break; fi
  sleep 1
done
[ "$ready" = 1 ] || { echo "✗ 报表数据集未 seed：CODE=$CODE BODY=$BODY"; cat "$LOG"; exit 1; }
echo "✓ server 就绪 + usage 数据集已 seed"
echo ""

# ============ 2. 灌测试数据 ============
echo "▶ 准备数据（1 客户 + 1 销售 + 另一组织 + usage_logs）..."
TS=$(date +%s)
PHONE_CUST="13$(printf '%09d' $(( (TS + RANDOM) % 1000000000 )))"
PHONE_SALES="13$(printf '%09d' $(( (TS + RANDOM + 7) % 1000000000 )))"

login() {
  req POST /api/identity/auth/send-code "" "{\"phone\":\"$1\"}"
  local code; code=$(jqv '.dev_code')
  req POST /api/identity/auth/login "" "{\"phone\":\"$1\",\"code\":\"$code\"}"
  TOKEN=$(jqv '.token'); LUID=$(jqv '.user.id'); LORG=$(jqv '.user.org_id')
}
login "$PHONE_CUST"; JWT_CUST=$TOKEN; UID_CUST=$LUID; ORG_CUST=$LORG
login "$PHONE_SALES"; JWT_SALES=$TOKEN; UID_SALES=$LUID
[ -n "$ORG_CUST" ] && [ "$ORG_CUST" != "null" ] || { echo "✗ 客户登录失败"; cat "$LOG"; exit 1; }

req POST "/api/identity/users/$UID_CUST/roles" "$AUTH_ADMIN" '{"role_slug":"customer"}'
req POST "/api/identity/users/$UID_SALES/roles" "$AUTH_ADMIN" '{"role_slug":"sales"}'

# 另一组织（其用量不应被 customer 看到）
req POST /api/identity/organizations "$AUTH_ADMIN" '{"name":"SMOKE-RPT-OTHER","org_type":"Enterprise"}'
ORG_OTHER=$(jqv '.id')

# usage_logs：customer 组织 2 行（model 7，charged 10/20），另一组织 1 行（model 9，charged 99）
INS="INSERT INTO usage_logs (org_id, api_key_id, model_id, channel_id, billing_unit, quantity, base_amount, charged_amount, is_stream, latency_ms, created_at) VALUES"
pg "$INS
  ($ORG_CUST, 1, 7, 3, 'token', '{\"input\":10,\"output\":5}', 10, 10, false, 100, '$T0'),
  ($ORG_CUST, 1, 7, 3, 'token', '{\"input\":20,\"output\":8}', 20, 20, false, 200, '$T0'),
  ($ORG_OTHER, 1, 9, 4, 'token', '{\"input\":99,\"output\":9}', 99, 99, false, 300, '2099-06-01T01:00:00Z');" >/dev/null
echo "✓ orgCust=${ORG_CUST} orgOther=${ORG_OTHER} (灌 3 行 usage)"
echo ""

# ============ 3. 断言 ============
echo "▶ 断言："
WIN="\"from\":\"$T0\",\"to\":\"$T1\""

# 1) 数据集列表（admin）含 usage
req GET /api/report/datasets "$AUTH_ADMIN" ""
if printf '%s' "$BODY" | jq -e 'any(.[];.slug=="usage")' >/dev/null; then ok "01 admin 列数据集含 usage"; else ng "01 数据集列表"; fi

# 2) 数据集详情含 metrics
req GET /api/report/datasets/usage "$AUTH_ADMIN" ""
[ "$CODE" = "200" ] && [ "$(printf '%s' "$BODY" | jq -e '.metrics|length>=3' >/dev/null; echo $?)" = "0" ] && ok "02 数据集详情含 metrics" || ng "02 数据集详情"

# 3) admin 全量（无 RLS）：时间窗内 calls=3 revenue=129
req POST /api/report/datasets/usage/query "$AUTH_ADMIN" "{\"metrics\":[\"calls\",\"revenue\"],$WIN}"
if [ "$CODE" = "200" ] && printf '%s' "$BODY" | jq -e '.rls_filtered==false and .rows[0].calls==3 and .rows[0].revenue==129' >/dev/null; then
  ok "03 admin 全量 calls=3 revenue=129（rls_filtered=false）"; else ng "03 admin 全量聚合"; fi

# 4) customer RLS：仅本组织 calls=2 revenue=30（rls_filtered=true）★核心
req POST /api/report/datasets/usage/query "$(authj "$JWT_CUST")" "{\"metrics\":[\"calls\",\"revenue\"],$WIN}"
if [ "$CODE" = "200" ] && printf '%s' "$BODY" | jq -e '.rls_filtered==true and .rows[0].calls==2 and .rows[0].revenue==30' >/dev/null; then
  ok "04 customer RLS 仅本组织 calls=2 revenue=30（rls_filtered=true）"; else ng "04 customer RLS 隔离"; fi

# 5) customer 维度查询：按 model 仅本组织（1 行 model_id=7 calls=2）
req POST /api/report/datasets/usage/query "$(authj "$JWT_CUST")" "{\"metrics\":[\"calls\"],\"dimensions\":[\"model_id\"],$WIN}"
if [ "$CODE" = "200" ] && printf '%s' "$BODY" | jq -e '(.rows|length)==1 and .rows[0].model_id=="7" and .rows[0].calls==2' >/dev/null; then
  ok "05 customer 维度查询仅本组织（model 7 calls=2）"; else ng "05 customer 维度 RLS"; fi

# 6) 未知指标 → 400
req POST /api/report/datasets/usage/query "$AUTH_ADMIN" "{\"metrics\":[\"calls\",\"hack\"],$WIN}"
expect 400 "06 未知指标 → 400"

# 7) 空指标 → 400
req POST /api/report/datasets/usage/query "$AUTH_ADMIN" "{\"metrics\":[],$WIN}"
expect 400 "07 空指标 → 400"

# 8) 维度不在白名单（org_id 不可作维度，防三角定位）→ 400
req POST /api/report/datasets/usage/query "$AUTH_ADMIN" "{\"metrics\":[\"calls\"],\"dimensions\":[\"org_id\"],$WIN}"
expect 400 "08 非白名单维度 org_id → 400"

# 9) sales 无 rls_rule 分支 → 403
req POST /api/report/datasets/usage/query "$(authj "$JWT_SALES")" "{\"metrics\":[\"calls\"],$WIN}"
expect 403 "09 sales（无 rls 分支）查 usage → 403"

# 10) customer 无 report.define → 创建报表 403
req POST /api/report/reports "$(authj "$JWT_CUST")" '{"dataset_slug":"usage","name":"我的用量"}'
expect 403 "10 customer 无 report.define 建报表 → 403"

# 11) admin 创建报表 → 200 + 列表含 + 详情 + 删除
req POST /api/report/reports "$AUTH_ADMIN" '{"dataset_slug":"usage","name":"SMOKE报表","visibility":"role","config":{"chart":"bar"}}'
RID=$(jqv '.id')
[ "$CODE" = "200" ] && [ -n "$RID" ] && [ "$RID" != "null" ] && ok "11 admin 建报表 200" || ng "11 admin 建报表"
req GET /api/report/reports "$AUTH_ADMIN" ""
printf '%s' "$BODY" | jq -e --argjson r "${RID:-0}" 'any(.[];.id==$r)' >/dev/null && ok "12 报表列表含新建" || ng "12 报表列表"
req GET "/api/report/reports/$RID" "$AUTH_ADMIN" ""
expect 200 "13 报表详情 200"
req DELETE "/api/report/reports/$RID" "$AUTH_ADMIN" ""
expect 200 "14 报表删除 200"

# 12) 基于不存在数据集建报表 → 400
req POST /api/report/reports "$AUTH_ADMIN" '{"dataset_slug":"nope","name":"X"}'
expect 400 "15 基于不存在数据集 → 400"

echo ""
echo "════════════════════════════════"
printf '  通过 \033[32m%d\033[0m / 失败 \033[31m%d\033[0m\n' "$PASS" "$FAIL"
[ "$FAIL" = 0 ] && echo "  ✅ M4 报表片A smoke 全过" || echo "  ❌ 有失败项"
echo "════════════════════════════════"
[ "$FAIL" = 0 ]
