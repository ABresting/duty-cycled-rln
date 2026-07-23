#!/usr/bin/env python3
"""Maximum-envelope-size probe on the local nwaku cluster.

Requires: waku/node.sh up 5 (star topology on node0; RLN off, cluster 66).
Publishes single messages of growing payload size from node0 and checks whether
all leaf nodes receive them, then bisects the boundary to 1 KB. The largest
deliverable payload plus the measured framing constant is the network's maximum
message size, which bounds the one-envelope packaging of a burst.

Output: results/envelope-probe-<date>.csv (stdlib only, no dependencies).
"""
import base64, csv, datetime, json, os, secrets, time, urllib.request, urllib.parse

REST0, NODES = 8645, 5
PUBSUB = "/waku/2/rs/66/0"
CTOPIC = "/packaging/1/probe/proto"
ENC = urllib.parse.quote(PUBSUB, safe="")
LEAVES = list(range(1, NODES))


def api(i, path):
    return f"http://127.0.0.1:{REST0+i}{path}"


def get(url, timeout=5):
    with urllib.request.urlopen(url, timeout=timeout) as r:
        return r.read()


def post(url, data, timeout=15):
    req = urllib.request.Request(url, data=json.dumps(data).encode(),
                                 headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.status, r.read()


def subscribe_all():
    for i in range(NODES):
        post(api(i, "/relay/v1/subscriptions"), [PUBSUB])


def drain(i):
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


def probe(payload_bytes, wait_s=4.0):
    """Publish one message of the given payload size; return (post_status, n_leaves_delivered)."""
    marker = secrets.token_bytes(16)
    payload = marker + b"\x00" * (payload_bytes - 16)
    msg = {"payload": base64.b64encode(payload).decode(), "contentTopic": CTOPIC}
    for i in range(NODES):
        drain(i)
    try:
        status, _ = post(api(0, f"/relay/v1/messages/{ENC}"), msg)
    except urllib.error.HTTPError as e:
        return f"http{e.code}", 0
    except Exception as e:
        return type(e).__name__, 0
    got = set()
    deadline = time.time() + wait_s
    while time.time() < deadline and len(got) < len(LEAVES):
        for i in LEAVES:
            if i not in got and marker in drain(i):
                got.add(i)
        time.sleep(0.15)
    return f"http{status}", len(got)


def main():
    os.makedirs("results", exist_ok=True)
    out = f"results/envelope-probe-{datetime.datetime.now():%Y%m%d-%H%M}.csv"
    subscribe_all()
    time.sleep(1.0)
    rows = []

    def run(size):
        status, leaves = probe(size)
        ok = leaves == len(LEAVES)
        rows.append((size, status, leaves, int(ok)))
        print(f"payload {size:>7} B  post={status:<22} leaves={leaves}/{len(LEAVES)}"
              f"  {'DELIVERED' if ok else 'not delivered'}")
        return ok

    sweep = [1_000, 50_000, 100_000, 140_000, 145_000, 150_000, 155_000, 160_000, 200_000]
    results = {size: run(size) for size in sweep}

    lo = max((s for s, ok in results.items() if ok), default=0)
    hi = min((s for s, ok in results.items() if not ok), default=0)
    if lo and hi:
        while hi - lo > 1_000:
            mid = (lo + hi) // 2
            if run(mid):
                lo = mid
            else:
                hi = mid
        print(f"boundary: largest delivered payload {lo} B, smallest failing {hi} B")

    with open(out, "w", newline="") as f:
        w = csv.writer(f)
        f.write(f"# envelope-size probe, {NODES}-node local cluster, date={datetime.date.today()}\n")
        w.writerow(["payload_B", "post_status", "leaves_delivered", "delivered_all"])
        w.writerows(rows)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
