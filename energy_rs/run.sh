#!/usr/bin/env bash
# Energy RS add-on entrypoint (PLAN.md section 10).
#
# The Supervisor writes /data/options.json before every start and does not hot-reload it: changing
# an option needs an add-on restart. Only the log level is turned into an environment variable here;
# the binary reads the rest of the file itself (mode, price provider, manual batteries).
set -euo pipefail

options="${ENERGY_RS_DATA:-/data}/options.json"
log_level=""
if [ -r "$options" ]; then
  # No jq dependency in the runtime image: log_level is one flat string, so one sed expression is
  # enough. The key is unique in the manifest schema, and the trailing quote must be followed by a
  # colon, so an entity value like "sensor.log_level" cannot match.
  log_level="$(sed -n 's/.*"log_level"[[:space:]]*:[[:space:]]*"\([a-z]*\)".*/\1/p' "$options")"
fi
case "$log_level" in
  trace | debug | info | warn | error) ;;
  *) log_level=info ;;
esac

export LOG_LEVEL="${log_level}"
export ENERGY_RS_DATA="${ENERGY_RS_DATA:-/data}"

# `exec` replaces the shell so the Supervisor's SIGTERM reaches the binary directly and the shutdown
# path (stop dispatch, checkpoint the ledger, exit 0 within `timeout: 30`) actually runs.
exec /usr/local/bin/energy-server
