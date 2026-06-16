#!/usr/bin/env bash
# CRM 片A+B 端到端 smoke：自起/自停 rise-server + 灌测试数据 + 29 项断言。
#
# 片A：数据域读/写隔离（销售仅本人名下）、越域 404 不泄露存在性、跟进 author 归属、
# 空内容 400、归属改派事务、幂等 assign 不增历史、改派后可见性翻转、归属历史 active 唯一、
# 幽灵 sales 400、超管令牌全量、finance 越权写拦截（crm.read.all 无 crm.write → 403）。
# 片B：代客开户（事务建 org+user+首条归属、手机号唯一）、代客充值（一步 Paid+入账+业绩归因、
# 越域 404、finance 无 crm.write 开户 403）。
#
# 前置：PostgreSQL 已起且迁移到位（make infra-up && make migrate）。
# 用法：bash backend/scripts/smoke_crm.sh   （退出码 0=全过，1=有失败/起服务失败）
#
# 依赖：curl、jq。server 读 RR_DATABASE_URL（默认本地 PG）；seed_builtins 启动时补 crm 权限点。
set -uo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
BACKEND=$(cd "$SCRIPT_DIR/.." && pwd)

PORT=8088
BASE="http://127.0.0.1:${PORT}"
TOK="smoke-admin-$$-${RANDOM}"          # 超管令牌（X-Admin-Token 逃生通道）
SEC="smoke-jwt-$$-${RANDOM}"            # JWT 签名密钥
DBURL=${RR_DATABASE_URL:-${DATABASE_URL:-postgres://rise:rise@localhost:5432/rise_router}}
LOG=$(mktemp -t rise_smoke_server.XXXXXX.log)

PASS=0
FAIL=0
SERVER_PID=""

cleanup() {
  [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
}
trap cleanup EXIT

# --- HTTP helper：设置全局 CODE / BODY ---
# 用法：req METHOD PATH AUTH_HEADER_LINE BODY_JSON   （AUTH/BODY 可为空字符串）
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
  [ "$BODY" = "$CODE" ] && BODY=""   # 无 body 时（如 204）out 仅一行
}

ok()  { PASS=$((PASS+1)); printf '  \033[32m✓\033[0m %s\n' "$1"; }
ng()  { FAIL=$((FAIL+1)); printf '  \033[31m✗\033[0m %s  (CODE=%s BODY=%s)\n' "$1" "$CODE" "${BODY:0:160}"; }
expect() { if [ "$CODE" = "$1" ]; then ok "$2"; else ng "$2"; fi; }       # 断言 HTTP 状态码
jqv() { printf '%s' "$BODY" | jq -r "$1" 2>/dev/null; }                    # 从 BODY 取字段

AUTH_ADMIN="X-Admin-Token: $TOK"
authj() { echo "Authorization: Bearer $1"; }                              # JWT 头

# ============ 1. 编译 + 起 server ============
echo "▶ 编译 rise-server ..."
( cd "$BACKEND" && cargo build -q -p rise-server ) || { echo "✗ 编译失败"; exit 1; }

# 端口预检：已占用则中止（避免对陌生进程跑断言）
req GET /healthz "" ""
if [ "$CODE" = "200" ]; then
  echo "✗ 端口 ${PORT} 已被占用（/healthz 返回 200）。请先停掉占用进程再跑。"
  exit 1
fi

echo "▶ 启动 server（端口 ${PORT}）..."
# exec：用 rise-server 替换子 shell，使 $! 精确指向 server 进程（否则 kill 杀的是子 shell，server 泄漏占端口）
( cd "$BACKEND" && exec env RR_ADMIN_TOKEN="$TOK" RR_JWT_SECRET="$SEC" RR_DATABASE_URL="$DBURL" \
    RR_LOG_LEVEL=warn RR_BIND_ADDR="0.0.0.0:${PORT}" ./target/debug/rise-server ) >"$LOG" 2>&1 &
SERVER_PID=$!

# 等就绪：轮询 readyz 直到 ready（"degraded" 不含子串 "ready"）
echo "▶ 等待 /readyz ..."
ready=0
for _ in $(seq 1 60); do
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then echo "✗ server 进程已退出"; cat "$LOG"; exit 1; fi
  req GET /readyz "" ""
  if printf '%s' "$BODY" | grep -qi 'ready'; then ready=1; break; fi
  sleep 1
done
[ "$ready" = 1 ] || { echo "✗ /readyz 未就绪：CODE=$CODE BODY=$BODY"; cat "$LOG"; exit 1; }

# 等 seed 完成：超管令牌列权限点直到含 crm.assign
echo "▶ 等待 seed_builtins ..."
seeded=0
for _ in $(seq 1 20); do
  req GET /api/identity/permissions "$AUTH_ADMIN" ""
  if printf '%s' "$BODY" | grep -q 'crm.assign'; then seeded=1; break; fi
  sleep 1
done
[ "$seeded" = 1 ] || { echo "✗ seed 未落地 crm 权限点：CODE=$CODE BODY=$BODY"; cat "$LOG"; exit 1; }
echo "✓ server 就绪"
echo ""

# ============ 2. 灌测试数据 ============
echo "▶ 准备数据（2 个销售 + 2 个客户 org）..."
TS=$(date +%s)
PHONE_A="13$(printf '%09d' $(( (TS + RANDOM) % 1000000000 )))"
PHONE_B="13$(printf '%09d' $(( (TS + RANDOM + 7) % 1000000000 )))"
PHONE_C="13$(printf '%09d' $(( (TS + RANDOM + 13) % 1000000000 )))"

login() {  # $1=phone -> 设全局 TOKEN/LUID（UID 是 bash 只读变量，勿用）
  req POST /api/identity/auth/send-code "" "{\"phone\":\"$1\"}"
  local code; code=$(jqv '.dev_code')
  req POST /api/identity/auth/login "" "{\"phone\":\"$1\",\"code\":\"$code\"}"
  TOKEN=$(jqv '.token'); LUID=$(jqv '.user.id')
}
login "$PHONE_A"; JWT_A=$TOKEN; UID_A=$LUID
login "$PHONE_B"; JWT_B=$TOKEN; UID_B=$LUID
login "$PHONE_C"; JWT_C=$TOKEN; UID_C=$LUID   # 作 finance：crm.read.all 但无 crm.write
[ -n "$UID_A" ] && [ "$UID_A" != "null" ] || { echo "✗ 销售A 登录失败"; exit 1; }

# 授角色（超管令牌）。JWT 不含权限，每次请求实时查 user_permissions，授后旧 JWT 即生效。
req POST "/api/identity/users/$UID_A/roles" "$AUTH_ADMIN" '{"role_slug":"sales"}'
req POST "/api/identity/users/$UID_B/roles" "$AUTH_ADMIN" '{"role_slug":"sales"}'
req POST "/api/identity/users/$UID_C/roles" "$AUTH_ADMIN" '{"role_slug":"finance"}'

# orgX：初始无归属（走 /assign 测事务+历史）；orgY：直接归 B（测越域）
req POST /api/identity/organizations "$AUTH_ADMIN" '{"name":"SMOKE-客户X","org_type":"Enterprise"}'
ORG_X=$(jqv '.id')
req POST /api/identity/organizations "$AUTH_ADMIN" "{\"name\":\"SMOKE-客户Y\",\"org_type\":\"Enterprise\",\"owner_sales_id\":$UID_B}"
ORG_Y=$(jqv '.id')
[ -n "$ORG_X" ] && [ "$ORG_X" != "null" ] || { echo "✗ 建客户org失败"; exit 1; }
echo "✓ salesA=$UID_A salesB=$UID_B  orgX=$ORG_X orgY=$ORG_Y"
echo ""

# ============ 3. 19 项断言 ============
echo "▶ 断言："

# 1) assign 归属：超管把 orgX 改派给 salesA（写第 1 条 active 历史）
req POST "/api/crm/customers/$ORG_X/assign" "$AUTH_ADMIN" "{\"sales_id\":$UID_A}"
[ "$CODE" = "200" ] && [ "$(jqv '.owner_sales_id')" = "$UID_A" ] && ok "01 assign orgX→A，owner_sales_id 更新" || ng "01 assign orgX→A"

# 2) 销售列表仅本人名下：salesA 见 orgX 不见 orgY
req GET /api/crm/customers "$(authj "$JWT_A")" ""
if printf '%s' "$BODY" | jq -e --argjson x "$ORG_X" --argjson y "$ORG_Y" 'any(.[];.id==$x) and (any(.[];.id==$y)|not)' >/dev/null; then ok "02 salesA 列表含 orgX 不含 orgY"; else ng "02 salesA 列表数据域"; fi

# 3) salesB 列表见 orgY 不见 orgX
req GET /api/crm/customers "$(authj "$JWT_B")" ""
if printf '%s' "$BODY" | jq -e --argjson x "$ORG_X" --argjson y "$ORG_Y" 'any(.[];.id==$y) and (any(.[];.id==$x)|not)' >/dev/null; then ok "03 salesB 列表含 orgY 不含 orgX"; else ng "03 salesB 列表数据域"; fi

# 4) 本人详情可见 + 钱包余额快照（无钱包=0）
req GET "/api/crm/customers/$ORG_X" "$(authj "$JWT_A")" ""
[ "$CODE" = "200" ] && [ "$(jqv '.balance')" = "0" ] && ok "04 salesA 看 orgX 详情 200，余额=0" || ng "04 salesA 看 orgX 详情"

# 5) 越域客户详情 → 404 不泄露存在性
req GET "/api/crm/customers/$ORG_Y" "$(authj "$JWT_A")" ""
expect 404 "05 salesA 看 orgY（越域）→ 404"

# 6) 跟进 author 归属：salesA 给 orgX 写跟进，author_id=本人
req POST "/api/crm/customers/$ORG_X/notes" "$(authj "$JWT_A")" '{"content":"  首次电话沟通  "}'
[ "$CODE" = "200" ] && [ "$(jqv '.author_id')" = "$UID_A" ] && [ "$(jqv '.content')" = "首次电话沟通" ] && ok "06 salesA 写跟进，author=本人+trim" || ng "06 salesA 写跟进"

# 7) 越域写跟进 → 404
req POST "/api/crm/customers/$ORG_Y/notes" "$(authj "$JWT_A")" '{"content":"越权写入"}'
expect 404 "07 salesA 给 orgY 写跟进（越域）→ 404"

# 8) 空内容 → 400
req POST "/api/crm/customers/$ORG_X/notes" "$(authj "$JWT_A")" '{"content":"   "}'
expect 400 "08 空白跟进内容 → 400"

# 9) 本人 notes 列表可见刚写的
req GET "/api/crm/customers/$ORG_X/notes" "$(authj "$JWT_A")" ""
[ "$CODE" = "200" ] && [ "$(jqv 'length')" -ge 1 ] && ok "09 salesA 列 orgX 跟进可见" || ng "09 salesA 列跟进"

# 10) 越域读 notes → 404
req GET "/api/crm/customers/$ORG_X/notes" "$(authj "$JWT_B")" ""
expect 404 "10 salesB 读 orgX 跟进（越域）→ 404"

# 11) 幂等 assign：orgX 已归 A，再 assign A → 200 no-op
req POST "/api/crm/customers/$ORG_X/assign" "$AUTH_ADMIN" "{\"sales_id\":$UID_A}"
expect 200 "11 幂等 assign orgX→A（已是A）→ 200"

# 12) 幂等不增历史：assignments 仍 1 条，且 1 条 active
req GET "/api/crm/customers/$ORG_X/assignments" "$AUTH_ADMIN" ""
if [ "$(jqv 'length')" = "1" ] && [ "$(jqv '[.[]|select(.active)]|length')" = "1" ]; then ok "12 幂等后历史仍 1 条（1 active）"; else ng "12 幂等历史计数"; fi

# 13) 改派 orgX → salesB
req POST "/api/crm/customers/$ORG_X/assign" "$AUTH_ADMIN" "{\"sales_id\":$UID_B}"
[ "$CODE" = "200" ] && [ "$(jqv '.owner_sales_id')" = "$UID_B" ] && ok "13 改派 orgX→B" || ng "13 改派 orgX→B"

# 14) 改派后可见性翻转：salesB 现可见 orgX
req GET "/api/crm/customers/$ORG_X" "$(authj "$JWT_B")" ""
expect 200 "14 改派后 salesB 可见 orgX"

# 15) 改派后 salesA 不再可见 orgX → 404
req GET "/api/crm/customers/$ORG_X" "$(authj "$JWT_A")" ""
expect 404 "15 改派后 salesA 看 orgX → 404"

# 16) 归属历史 2 条，仅 1 条 active，且 active 的 sales_id=B
req GET "/api/crm/customers/$ORG_X/assignments" "$AUTH_ADMIN" ""
if [ "$(jqv 'length')" = "2" ] && [ "$(jqv '[.[]|select(.active)]|length')" = "1" ] && [ "$(jqv '[.[]|select(.active)][0].sales_id')" = "$UID_B" ]; then ok "16 历史 2 条仅 1 active（=B）"; else ng "16 历史 2条1active"; fi

# 17) 幽灵 sales → 400（软引用校验）
req POST "/api/crm/customers/$ORG_X/assign" "$AUTH_ADMIN" '{"sales_id":99999999}'
expect 400 "17 改派到不存在的 sales → 400"

# 18) 超管令牌全量：列表含 orgX 和 orgY
req GET /api/crm/customers "$AUTH_ADMIN" ""
if printf '%s' "$BODY" | jq -e --argjson x "$ORG_X" --argjson y "$ORG_Y" 'any(.[];.id==$x) and any(.[];.id==$y)' >/dev/null; then ok "18 超管令牌列表含 orgX+orgY（全量）"; else ng "18 超管全量"; fi

# 19) 超管写跟进 author_id 为 null（无用户上下文）
req POST "/api/crm/customers/$ORG_X/notes" "$AUTH_ADMIN" '{"content":"超管代记"}'
[ "$CODE" = "200" ] && [ "$(jqv '.author_id')" = "null" ] && ok "19 超管写跟进 author_id=null" || ng "19 超管 author null"

# 20) finance（crm.read.all 无 crm.write）写跟进 → 403：require_scoped 必须先具 base_perm，防越权写
req POST "/api/crm/customers/$ORG_X/notes" "$(authj "$JWT_C")" '{"content":"finance 越权写入尝试"}'
expect 403 "20 finance 写跟进（无 crm.write）→ 403"

# 21) finance（crm.read.all）读任意客户 → 200：读全量不受 base_perm 修复影响
req GET "/api/crm/customers/$ORG_X" "$(authj "$JWT_C")" ""
expect 200 "21 finance 读任意客户（read.all 全量）→ 200"

# ---- 片B：代客开户 + 代客充值 ----
CUST_PHONE="13$(printf '%09d' $(( (TS + RANDOM + 21) % 1000000000 )))"

# 22) 销售A 代客开户 → 200，归属本人 + 建登录账号
req POST /api/crm/customers "$(authj "$JWT_A")" "{\"phone\":\"$CUST_PHONE\",\"name\":\"SMOKE-代客\",\"org_type\":\"Enterprise\"}"
ORG_NEW=$(jqv '.org.id')
{ [ "$CODE" = "200" ] && [ "$(jqv '.owner_sales_id')" = "$UID_A" ] && [ -n "$(jqv '.user_id')" ] && [ "$(jqv '.user_id')" != "null" ]; } && ok "22 销售A 代客开户 → 归属本人+建账号" || ng "22 代客开户"

# 23) 开户客户在销售A 名下可见
req GET "/api/crm/customers/$ORG_NEW" "$(authj "$JWT_A")" ""
expect 200 "23 开户客户在销售A 名下可见"

# 24) 开户建首条 active 归属（sales_id=A）
req GET "/api/crm/customers/$ORG_NEW/assignments" "$(authj "$JWT_A")" ""
{ [ "$(jqv 'length')" = "1" ] && [ "$(jqv '[.[]|select(.active)][0].sales_id')" = "$UID_A" ]; } && ok "24 开户建首条 active 归属(=A)" || ng "24 首条归属"

# 25) 重复手机号开户 → 400（一号不可两户）
req POST /api/crm/customers "$(authj "$JWT_A")" "{\"phone\":\"$CUST_PHONE\",\"name\":\"SMOKE-重复\",\"org_type\":\"Enterprise\"}"
expect 400 "25 重复手机号开户 → 400"

# 26) 销售A 代客充值 100 → 200：Paid + created_by=A + 入账余额=100
req POST "/api/crm/customers/$ORG_NEW/recharge" "$(authj "$JWT_A")" '{"amount":100}'
{ [ "$CODE" = "200" ] && [ "$(jqv '.order.status')" = "Paid" ] && [ "$(jqv '.order.created_by_sales_id')" = "$UID_A" ] && [ "$(printf '%s' "$BODY" | jq '.balance|tonumber==100')" = "true" ]; } && ok "26 代客充值 100 → Paid+归因A+入账" || ng "26 代客充值"

# 27) 充值后客户余额=100
req GET "/api/crm/customers/$ORG_NEW" "$(authj "$JWT_A")" ""
{ [ "$CODE" = "200" ] && [ "$(printf '%s' "$BODY" | jq '.balance|tonumber==100')" = "true" ]; } && ok "27 充值后客户余额=100" || ng "27 充值后余额"

# 28) 越域充值：销售A 给 orgY（归B）充值 → 404
req POST "/api/crm/customers/$ORG_Y/recharge" "$(authj "$JWT_A")" '{"amount":50}'
expect 404 "28 销售A 给 orgY 充值（越域）→ 404"

# 29) finance（无 crm.write）代客开户 → 403
CUST_PHONE2="13$(printf '%09d' $(( (TS + RANDOM + 31) % 1000000000 )))"
req POST /api/crm/customers "$(authj "$JWT_C")" "{\"phone\":\"$CUST_PHONE2\",\"name\":\"SMOKE-fin\",\"org_type\":\"Enterprise\"}"
expect 403 "29 finance 代客开户（无 crm.write）→ 403"

# ============ 汇总 ============
echo ""
echo "════════════════════════════════"
printf "  通过 %s / %s\n" "$PASS" "$((PASS+FAIL))"
echo "════════════════════════════════"
[ "$FAIL" -eq 0 ]
