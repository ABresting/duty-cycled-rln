#!/usr/bin/env python3
"""One-envelope vs m-envelope packaging experiment on the local nwaku cluster.

Requires: waku/node.sh up 5 (star topology on node0; RLN off, cluster 66).
Publishes bursts in both packagings from node0 and measures, per repeat:
  - delivery latency until ALL leaf nodes hold the complete burst
  - per-node libp2p byte deltas (in/out) around the publication
Also measures per-message wire framing: single messages at several payload sizes;
the intercept of bytes-vs-payload is the framing constant of Eq. (4).

Output: results/packaging-<date>.csv (stdlib only, no dependencies).
"""
import base64, json, os, secrets, time, urllib.request, urllib.parse, datetime

REST0, MET0, NODES = 8645, 8008, 5
PUBSUB = "/waku/2/rs/66/0"
CTOPIC = "/packaging/1/test/proto"
ENC = urllib.parse.quote(PUBSUB, safe="")
LEAVES = list(range(1, NODES))

def api(i, path):
    return f"http://127.0.0.1:{REST0+i}{path}"

def get(url, timeout=5):
    with urllib.request.urlopen(url, timeout=timeout) as r:
        return r.read()

def post(url, data, timeout=10):
    req = urllib.request.Request(url, data=json.dumps(data).encode(),
                                 headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()

def subscribe_all():
    for i in range(NODES):
        post(api(i, "/relay/v1/subscriptions"), [PUBSUB])

def drain(i):
    """relay/v1/messages returns-and-clears; collect payload prefixes (markers)."""
    try:
        msgs = json.loads(get(api(i, f"/relay/v1/messages/{ENC}"), timeout=2))
    except Exception:
        return []
    out = []
    for m in msgs:
        try:
            out.append(base64.b64decode(m.get("payload", ""))[:16])
        except Exception:
            pass
    return out

def bytes_counters(i):
    txt = get(f"http://127.0.0.1:{MET0+i}/metrics").decode()
    vals = {}
    for line in txt.splitlines():
        if line.startswith('libp2p_network_bytes_total{direction="'):
            d = line.split('"')[1]
            vals[d] = float(line.rsplit(" ", 1)[1])
    return vals

def publish(payload: bytes):
    post(api(0, f"/relay/v1/messages/{ENC}"),
         {"payload": base64.b64encode(payload).decode(),
          "contentTopic": CTOPIC, "timestamp": time.time_ns()})

def run_case(payload_sizes):
    """Publish one burst (list of message sizes) from node0; wait for all markers
    on all leaves. Returns (latency_s, {node: {in,out} deltas})."""
    markers = [secrets.token_bytes(16) for _ in payload_sizes]
    payloads = [mk + bytes(max(0, sz - 16)) for mk, sz in zip(markers, payload_sizes)]
    for i in range(NODES):
        drain(i)  # clear queues
    before = {i: bytes_counters(i) for i in range(NODES)}
    t0 = time.monotonic()
    for p in payloads:
        publish(p)
    pending = {i: set(markers) for i in LEAVES}
    deadline = t0 + 30
    last_seen = None
    while time.monotonic() < deadline and any(pending.values()):
        for i in LEAVES:
            if pending[i]:
                for mk in drain(i):
                    if mk in pending[i]:
                        pending[i].discard(mk)
                        last_seen = time.monotonic()
        time.sleep(0.02)
    if any(pending.values()):
        return None, None
    time.sleep(1.0)  # let byte counters settle
    after = {i: bytes_counters(i) for i in range(NODES)}
    deltas = {i: {d: after[i][d] - before[i][d] for d in ("in", "out")} for i in range(NODES)}
    return last_seen - t0, deltas

def main():
    subscribe_all()
    time.sleep(2)
    os.makedirs("results", exist_ok=True)
    out = f"results/packaging-{datetime.datetime.now():%Y%m%d-%H%M}.csv"
    R = 5
    rows = ["experiment,B,form,repeat,latency_ms,node0_out_B,leaves_in_B_total"]

    # --- framing measurement: single messages, payload sweep ---
    for P in (100, 1000, 10000):
        for r in range(R):
            lat, d = run_case([P])
            if lat is None:
                rows.append(f"framing,{P},single,{r},TIMEOUT,,"); continue
            leaves_in = sum(d[i]["in"] for i in LEAVES)
            rows.append(f"framing,{P},single,{r},{lat*1000:.1f},{d[0]['out']:.0f},{leaves_in:.0f}")

    # --- packaging comparison: one envelope vs m envelopes (k=8 groups, P=100) ---
    for B in (8, 32, 64):
        m = B // 8
        env_one = [B * 100 + 256 * m + 64 * B]          # E(B): payloads + m proof groups + per-msg pairs
        env_m = [8 * 100 + 256 + 64 * 8] * m            # m single-group envelopes
        for form, sizes in (("one_envelope", env_one), ("m_envelopes", env_m)):
            for r in range(R):
                lat, d = run_case(sizes)
                if lat is None:
                    rows.append(f"packaging,{B},{form},{r},TIMEOUT,,"); continue
                leaves_in = sum(d[i]["in"] for i in LEAVES)
                rows.append(f"packaging,{B},{form},{r},{lat*1000:.1f},{d[0]['out']:.0f},{leaves_in:.0f}")

    with open(out, "w") as f:
        f.write("\n".join(rows) + "\n")
    print(f"wrote {out}")
    print("\n".join(rows[:1] + rows[1:6]))

if __name__ == "__main__":
    main()
