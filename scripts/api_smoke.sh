#!/usr/bin/env bash
# Contract smoke test for the energy-server HTTP API (docs/API.md).
#
# Read-only: it only issues GET requests. Usage:
#   scripts/api_smoke.sh                      # http://127.0.0.1:8099
#   BASE_URL=http://192.168.1.2:8099 scripts/api_smoke.sh
#   SMOKE_TIMEOUT=5 scripts/api_smoke.sh      # seconds per request (default 5)
#
# Exits non-zero on the first unexpected status or missing marker. `plan/current` may legitimately
# answer 404 before the engine has produced its first plan; that one route is reported but allowed.

set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8099}"
TIMEOUT="${SMOKE_TIMEOUT:-5}"
TMP_DIR="$(mktemp -d)"
FAILURES=0

cleanup() {
    rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

# fetch <path> <expected-status> <marker>
fetch() {
    local path="$1" expected="$2" marker="$3"
    local body="${TMP_DIR}/body.json"
    local status
    status="$(curl --silent --show-error --max-time "${TIMEOUT}" \
        --output "${body}" --write-out '%{http_code}' "${BASE_URL}${path}")" || {
        printf 'FAIL %-28s curl error\n' "${path}"
        FAILURES=$((FAILURES + 1))
        return
    }
    if [[ "${status}" != "${expected}" ]]; then
        printf 'FAIL %-28s status %s (expected %s)\n' "${path}" "${status}" "${expected}"
        FAILURES=$((FAILURES + 1))
        return
    fi
    if [[ -n "${marker}" ]] && ! grep --quiet -- "${marker}" "${body}"; then
        printf 'FAIL %-28s missing marker %s\n' "${path}" "${marker}"
        FAILURES=$((FAILURES + 1))
        return
    fi
    printf 'ok   %-28s %s\n' "${path}" "${status}"
}

printf 'energy-server smoke test against %s\n' "${BASE_URL}"

fetch "/api/v1/health" 200 '"status":"ok"'
fetch "/api/v1/site" 200 '"configured"'
fetch "/api/v1/packs" 200 '"packs"'
fetch "/api/v1/prices" 200 '"slots"'
fetch "/api/v1/plans?limit=5" 200 '"plans"'
fetch "/api/v1/dispatch?limit=5" 200 '"entries"'
fetch "/api/v1/savings?period=lifetime" 200 '"totals"'
fetch "/api/v1/settings" 200 '"schema"'
fetch "/api/v1/nope" 404 '"not_found"'

# A plan may not exist yet; report it without failing the run.
plan_status="$(curl --silent --show-error --max-time "${TIMEOUT}" \
    --output "${TMP_DIR}/plan.json" --write-out '%{http_code}' \
    "${BASE_URL}/api/v1/plan/current")" || plan_status="000"
case "${plan_status}" in
    200) printf 'ok   %-28s 200\n' "/api/v1/plan/current" ;;
    404) printf 'warn %-28s 404 (no plan yet)\n' "/api/v1/plan/current" ;;
    *)   printf 'FAIL %-28s status %s\n' "/api/v1/plan/current" "${plan_status}"
         FAILURES=$((FAILURES + 1)) ;;
esac

# SSE: the first event must be a state snapshot (the stream then keeps the connection open, so
# curl is stopped by the per-request timeout).
curl --silent --show-error --no-buffer --max-time "${TIMEOUT}" \
    "${BASE_URL}/api/v1/stream" > "${TMP_DIR}/stream.txt" 2>/dev/null || true
if grep --quiet '^event: state' "${TMP_DIR}/stream.txt"; then
    printf 'ok   %-28s first event\n' "/api/v1/stream"
else
    printf 'FAIL %-28s no SSE state event\n' "/api/v1/stream"
    FAILURES=$((FAILURES + 1))
fi

# No response may echo a credential name or a token-like key.
if grep --quiet --extended-regexp 'token|password|secret|refresh' "${TMP_DIR}"/*.json 2>/dev/null; then
    printf 'FAIL redaction: a response mentions a credential name\n'
    FAILURES=$((FAILURES + 1))
else
    printf 'ok   redaction check\n'
fi

if [[ "${FAILURES}" -ne 0 ]]; then
    printf '%s check(s) failed\n' "${FAILURES}"
    exit 1
fi
printf 'all checks passed\n'
