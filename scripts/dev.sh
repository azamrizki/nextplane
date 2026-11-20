#!/usr/bin/env bash
# Runs the server (debug build, data in $DATA) and Vite together; Ctrl-C stops both.
set -euo pipefail
cd "$(dirname "$0")/.."
DATA=${DATA:-$PWD/.devdata}
LISTEN=${LISTEN:-127.0.0.1:12313}
WEB_PORT=${WEB_PORT:-15173}

[ -d web/node_modules ] || (cd web && npm ci)
cargo build -p nextplane

trap 'kill 0' EXIT INT TERM
./target/debug/nextplane --data "$DATA" serve --listen "$LISTEN" &
(cd web && NEXTPLANE_DEV_HUB="http://$LISTEN" WEB_PORT="$WEB_PORT" npx vite) &
echo "dev: web http://localhost:$WEB_PORT  (setup links point at the server port; swap it for $WEB_PORT)"
wait
