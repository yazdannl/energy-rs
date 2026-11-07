#!/usr/bin/env bash
# Energy RS add-on entrypoint.
#
# `exec` matters: it replaces the shell so the Supervisor's SIGTERM reaches the binary directly and
# the shutdown path (ledger checkpoint once P6 lands) actually runs.
set -euo pipefail

cd /data
exec /usr/local/bin/energy-server
