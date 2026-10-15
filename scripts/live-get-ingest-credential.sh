#!/bin/bash
# 라이브 GET 대조 장치(macOS) — 이 PC에 연결된 기존 기기 토큰(mhd_)으로 허브의
# `GET /api/devices/self/ingest-credential`을 **정확히 1회만** 호출하고, 응답의 키 구조·타입을 출력한다.
#
# 안전 장치
#   - GET만 쓴다(`-X GET`). POST·rotate·DELETE를 부르는 코드가 이 파일에 없다.
#   - 토큰은 앱의 CLI 모드(`--hub-auth-header`, 플러그인 headersHelper와 같은 경로)로 받아 셸 변수에만 두고
#     curl 설정을 stdin(`-K -`)으로 넘긴다 — 출력·프로세스 인자·파일에 남지 않는다.
#   - 응답 값은 출력하지 않는다. 열거형(issue_policy 등) 외의 문자열은 길이만, id·token_prefix도 길이만 보인다.
#   - 응답 본문은 임시 파일에 잠깐 쓰고 바로 지운다.
# 사용: scripts/live-get-ingest-credential.sh   (허브에 연결된 앱이 설치돼 있어야 한다)
#
# 같은 목적의 Rust `#[ignore]` 테스트(`cargo test -- --ignored live_get_structure_probe --nocapture`)도 있으나,
# 테스트 바이너리는 서명된 앱이 아니라 키체인 접근 허용 대화상자가 뜰 수 있다. 이 스크립트가 기본 수단이다.
set +x
WORK=$(mktemp -d) || exit 1
trap 'cd /; rm -rf "$WORK"' EXIT
cd "$WORK" || exit 1
EXE=$(jq -r .exe "$HOME/Library/Application Support/com.malgnsoft.agent/cli-path.json")
HDR=$(CLAUDE_CODE_MCP_SERVER_URL=https://malgnai-hub.apiserver.kr/mcp "$EXE" --hub-auth-header 2>/dev/null) || { echo "헤더 도우미 실패"; exit 3; }
AUTH=$(printf '%s' "$HDR" | jq -r '.Authorization // empty')
[ -n "$AUTH" ] || { echo "토큰 없음"; exit 4; }
CODE=$(printf 'header = "Authorization: %s"\n' "$AUTH" | curl -sS -K - -X GET -o live_body.json -w '%{http_code}' --max-time 35 https://malgnai-hub.apiserver.kr/api/devices/self/ingest-credential)
echo "HTTP $CODE"
unset HDR AUTH
cat > mask.jq <<'JQ'
def safe: ["issue_policy","blocked_reason","status","revoke_reason","access","last_temporality"];
. as $root
| path(..) as $p
| ($root | getpath($p)) as $v
| ($v | type) as $t
| ($p | map(tostring) | join(".")) as $name
| ($p | last // "" | tostring) as $key
| "  $" + (if $name == "" then "" else "." + $name end) + ": " +
  (if $t == "object" then "object(keys=" + ($v|length|tostring) + ")"
   elif $t == "array" then "array(len=" + ($v|length|tostring) + ")"
   elif $t == "string" then (if (safe | index($key)) then "string = " + ($v|tojson) else "string(len=" + ($v|length|tostring) + ")" end)
   elif $t == "boolean" then "boolean = " + ($v|tostring)
   else $t end)
JQ
jq -r -f mask.jq live_body.json 2>&1 | head -80
rm -f live_body.json mask.jq
