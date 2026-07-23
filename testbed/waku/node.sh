#!/usr/bin/env bash
#
# Waku (nwaku) node lifecycle manager for the local transport testbed.
#
# Builds nwaku from source into a native glibc binary (./build/wakunode2) — no
# Docker, no musl bundle. Build is one-time (~10-15 min; needs git/make/gcc/cargo;
# nwaku's Makefile bootstraps its own Nim). The binary is portable to similar
# Linux hosts (depends on glibc + libpq5/libstdc++ — standard packages).
#
# By default (WAKU_NET=local) nodes relay on a CUSTOM cluster-id with static peering
# (no RLN, no network deps) — good for controlled tests. Set WAKU_NET=twn to join
# The Waku Network (cluster 1) relay-only via discv5: real peers + real gossip, with
# NO RLN membership/funding (membership is only needed to *publish*); needs only a
# Linea Sepolia RPC for proof validation (WAKU_RPC, default a public endpoint).
# Pinned to nwaku v0.38.1 (github.com/logos-messaging/logos-delivery).
#
# Commands:
#   ./node.sh build         Build wakunode2 from source (one-time).
#   ./node.sh up [N]        Start N relay nodes (default 2), peer them, verify messaging.
#   ./node.sh message [TXT] Publish a message node0 -> node1 and verify receipt.
#   ./node.sh stop          Stop the node processes.
#   ./node.sh clean         Stop nodes and remove the node workspace (keeps the built binary).
#
# Exit status is non-zero on any failure, including a failed messaging self-test.

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/common.sh
. "$HERE/../lib/common.sh"

VERSION="${WAKU_VERSION:-v0.38.1}"
REPO="${WAKU_REPO:-https://github.com/waku-org/nwaku}"
CLUSTER="${WAKU_CLUSTER:-66}"            # any value != 1 (cluster 1 = TWN, mandates RLN)
SHARD=0
PUBSUB="/waku/2/rs/$CLUSTER/$SHARD"
WAKU_NET="${WAKU_NET:-local}"            # local = isolated custom cluster (static peering, publish self-test);
                                         # twn   = The Waku Network (cluster 1, discv5, relay-only, NO membership/funding)
RPC="${WAKU_RPC:-https://rpc.sepolia.linea.build}"   # Linea Sepolia RPC; only used in twn mode (RLN proof validation)
CONTENT_TOPIC="/redundancy-tax/1/test/proto"
RUN_DIR="$HERE/.run"
SRC="$RUN_DIR/nwaku"
BIN="$SRC/build/wakunode2"
N="${2:-2}"
TCP0=60000; REST0=8645; METRICS0=8008

require curl python3
rest_port()    { echo $((REST0 + $1)); }
metrics_port() { echo $((METRICS0 + $1)); }
api()          { echo "http://127.0.0.1:$(rest_port "$1")"; }
urlenc()       { python3 -c 'import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1],safe=""))' "$1"; }

build_source() {
  [ -x "$BIN" ] && return 0
  require git make gcc cargo
  info "building nwaku $VERSION from source (one-time, ~10-15 min; no docker)"
  mkdir -p "$RUN_DIR"; rm -rf "$SRC"
  git clone --depth 1 --branch "$VERSION" "$REPO" "$SRC" >/dev/null 2>&1 || die "git clone failed ($REPO $VERSION)"
  ( cd "$SRC" && make wakunode2 ) >"$RUN_DIR/build.log" 2>&1 \
    || { tail -n15 "$RUN_DIR/build.log" >&2; die "build failed — see $RUN_DIR/build.log"; }
  [ -x "$BIN" ] || die "build produced no binary at $BIN"
  info "built $BIN (glibc-native)"
}

stop_all() {
  local f
  for f in "$RUN_DIR"/node*.pid; do [ -f "$f" ] && kill "$(cat "$f")" 2>/dev/null; done
  [ -x "$BIN" ] && pkill -f "$BIN" 2>/dev/null   # absolute path: matches our nodes, not this script
  rm -f "$RUN_DIR"/node*.pid
  sleep 1
}

run_node() {  # $1 = index
  local i="$1" dir="$RUN_DIR/node$1"; mkdir -p "$dir"
  local -a flags
  if [ "$WAKU_NET" = twn ]; then
    flags=( --cluster-id=1 --relay=true
            --rln-relay-eth-client-address="$RPC"
            --discv5-discovery=true --discv5-udp-port=$((9000 + i)) )
  else
    flags=( --cluster-id="$CLUSTER" --shard="$SHARD" --relay=true --rln-relay=false
            --discv5-discovery=false --nat=extip:127.0.0.1 )
  fi
  flags+=( --rest=true --rest-address=127.0.0.1 --rest-port="$(rest_port "$i")" --rest-admin=true
           --metrics-server=true --metrics-server-address=127.0.0.1 --metrics-server-port="$(metrics_port "$i")"
           --tcp-port=$((TCP0 + i)) )
  ( cd "$dir" && exec "$BIN" "${flags[@]}" >"$RUN_DIR/node$i.log" 2>&1 ) &
  echo $! >"$RUN_DIR/node$i.pid"
}

wait_ready() {  # $1 = index
  local i="$1" pid t; pid="$(cat "$RUN_DIR/node$i.pid")"
  for t in $(seq 1 40); do
    kill -0 "$pid" 2>/dev/null || { tail -n10 "$RUN_DIR/node$i.log" >&2; die "node$i: process exited during startup"; }
    curl -fsS "$(api "$i")/debug/v1/info" >/dev/null 2>&1 && return 0
    sleep 1
  done
  tail -n10 "$RUN_DIR/node$i.log" >&2; die "node$i: REST not ready after 40s"
}

multiaddr() {  # $1 = index -> dialable multiaddr
  curl -fsS "$(api "$1")/debug/v1/info" \
    | python3 -c 'import sys,json;a=[x for x in json.load(sys.stdin)["listenAddresses"] if "/tcp/" in x and "/p2p/" in x];print(a[0] if a else "")'
}

peer() {  # connect node $2 -> node $1
  local addr; addr="$(multiaddr "$1")"
  [ -n "$addr" ] || die "node$1: no dialable multiaddr"
  curl -fsS -X POST "$(api "$2")/admin/v1/peers" -H 'content-type: application/json' -d "[\"$addr\"]" >/dev/null 2>&1 \
    || die "node$2 -> node$1 peering failed"
}

roundtrip() {  # publish $1 on node0, return 0 iff node1 receives it
  local text="$1" enc payload t
  enc="$(urlenc "$PUBSUB")"; payload="$(printf '%s' "$text" | base64 -w0)"
  curl -fsS -X POST "$(api 1)/relay/v1/subscriptions" -H 'content-type: application/json' -d "[\"$PUBSUB\"]" >/dev/null 2>&1
  sleep 3
  curl -fsS "$(api 1)/relay/v1/messages/$enc" >/dev/null 2>&1
  curl -fsS -X POST "$(api 0)/relay/v1/messages/$enc" -H 'content-type: application/json' \
    -d "{\"payload\":\"$payload\",\"contentTopic\":\"$CONTENT_TOPIC\",\"timestamp\":$(date +%s%N)}" >/dev/null 2>&1 \
    || { error "publish failed on node0"; return 1; }
  for t in $(seq 1 15); do
    if curl -fsS "$(api 1)/relay/v1/messages/$enc" 2>/dev/null \
        | EXPECT="$text" python3 -c 'import sys,json,base64,os
try: msgs=json.load(sys.stdin)
except Exception: sys.exit(1)
sys.exit(0 if any(base64.b64decode(m.get("payload","")).decode("utf-8","ignore")==os.environ["EXPECT"] for m in msgs) else 1)'; then
      return 0
    fi
    sleep 1
  done
  return 1
}

write_inventory() {
  local f="$RUN_DIR/nodes.json" i first=1
  { echo '{"nodes":['
    for i in $(seq 0 $((N - 1))); do
      [ "$first" -eq 1 ] || echo ','; first=0
      printf '  {"name":"waku%s","pid":%s,"waku_rest":%s,"waku_metrics":%s}' \
        "$i" "$(cat "$RUN_DIR/node$i.pid")" "$(rest_port "$i")" "$(metrics_port "$i")"
    done
    echo; echo ']}'; } >"$f"
  info "inventory: $f"
}

self_test() {
  if [ "$WAKU_NET" = twn ]; then
    local t n
    for t in $(seq 1 24); do
      n=$(curl -fsS "$(api 0)/admin/v1/peers" 2>/dev/null \
          | python3 -c 'import sys,json;print(sum(1 for p in json.load(sys.stdin) if p.get("connected")=="Connected"))' 2>/dev/null || echo 0)
      [ "${n:-0}" -ge 1 ] && { info "self-test passed (node0 connected to $n real Waku Network peer(s))"; return 0; }
      sleep 5
    done
    die "self-test FAILED — no real Waku Network peers within 120s (check RPC reachability)"
  fi
  [ "$N" -ge 2 ] || return 0
  if roundtrip "self-test-$(date +%s)"; then
    info "self-test passed (node-to-node message delivered)"
  else
    die "self-test FAILED — message did not propagate node0 -> node1; do not proceed to measurement"
  fi
}

ensure_up() { curl -fsS "$(api 0)/debug/v1/info" >/dev/null 2>&1 && curl -fsS "$(api 1)/debug/v1/info" >/dev/null 2>&1 || N=2 cmd_up; }

cmd_up() {
  build_source; stop_all
  if [ "$WAKU_NET" = twn ]; then
    info "starting $N relay node(s) on The Waku Network (cluster 1, relay-only/no membership, RLN-validate via RPC)"
  else
    info "starting $N relay node(s) (cluster-id $CLUSTER, shard $SHARD, RLN off; native glibc)"
  fi
  local i
  for i in $(seq 0 $((N - 1))); do run_node "$i"; done
  for i in $(seq 0 $((N - 1))); do wait_ready "$i"; info "node$i ready (rest 127.0.0.1:$(rest_port "$i"))"; done
  [ "$WAKU_NET" = twn ] || for i in $(seq 1 $((N - 1))); do peer 0 "$i"; done   # twn finds peers via discv5
  self_test
  write_inventory
  info "$N node(s) running and verified"
}

cmd_message() {
  local text="${2:-hello}"
  ensure_up; peer 0 1
  info "publishing on node0: \"$text\""
  roundtrip "$text" && info "node1 received the message — relay delivery verified" || die "node1 did not receive the message"
}

case "${1:-}" in
  build)   build_source ;;
  up)      cmd_up ;;
  message) cmd_message "$@" ;;
  stop)    stop_all; info "stopped" ;;
  clean)   stop_all; rm -rf "$RUN_DIR"/node* "$RUN_DIR"/nodes.json "$RUN_DIR"/rootfs; info "stopped; node workspace cleaned (built binary kept; rm -rf $RUN_DIR to fully reset)" ;;
  *)       die "usage: $0 {build | up [N] | message [TXT] | stop | clean}" ;;
esac
