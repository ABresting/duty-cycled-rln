#!/usr/bin/env node
// Analytical model of Section 4 of the paper: extends the validated Waku latency
// model of Revuelta et al. (DLT 2024) with batching, byte accounting, GossipSub
// amplification, and the duty-cycle term.
//
// NOTATION (see 06 doc §1 — keep these three apart):
//   B = burst size, messages per wake (DEMAND)      [bench files call circuit capacity "N"!]
//   k = batch capacity, slots per proof = messages per fold step (CAPACITY)
//   m = ceil(B/k) proofs (or fold steps) per burst
//   Q = per-window quota (POLICY) — not modeled here beyond Omega
// Zero dependencies. Run:  node model.mjs   (from this dir). Outputs: out/*.csv, out/*.svg.
import { mkdirSync, writeFileSync } from 'node:fs';

// ---------- measured / cited constants (every value has a source) ----------
const C = {
  // measured on the reference platform (20-core x86 workstation), paper Sec. 7.1
  singleProveMs: 182.69,         // 100-sample criterion median 
  // L_g^batch(k): MEASURED medians (100 samples). Wall-clock is
  // SUBLINEAR in k on the 20-core reference box (parallel MSM); do not fit a line.
  batchProveMs: { 4: 201.08, 8: 277.98, 16: 312.69, 32: 358.76, 64: 357.06 },
  batchVerifyFit: { v0: 0.9, v1: 0.15 }, // L_v^batch(k), prepared vk, measured k=4..64 (2026-07-14)
  singleVerifyOursMs: 1.07,      // measured single-circuit verify, prepared vk
  kCap: 8,                       // largest batch capacity with built artifacts (k=16/32 = rebuild)
  // ours, Nova batched folding (k=8 per step) — folding measurements (post epoch-binding fix)
  foldProvePerMsgMs: 418.1,      // B=128, K=16 (8 chained steps): steady state — smaller B
                                 // is flattered by Nova's near-free base case (2026-07-14)
  foldVerifyMs: 85,              // 78-93 ms flat across K
  foldProofBytes: 11640,         // ~11.3-11.6 KB flat
  // published platform baselines and network parameters (Revuelta et al., DLT 2024)
  platforms: {
    // Ref20: the reference platform itself (20-core i7-13700H) — all values native;
    // batch/fold/verify tables are the C-level constants, injected below.
    Ref20: { gen: 182.69, verify: 1.07, native: 'self' },
    M1:   { gen: 85.7,  verify: 2.7  },
    DED4: { gen: 223.91, verify: 1.61,
            native: { single: 223.91, verifySingle: 1.61, foldPerMsg: 688.1,
                      batch: { 4: 256.45, 8: 345.08, 16: 425.35, 32: 709.12, 64: 1156.22 } } },
    SH4:  { gen: 406.86, verify: 4.19,
            native: { single: 406.86, verifySingle: 4.19,
                      batch: { 4: 479.02, 8: 658.72, 16: 991.36, 32: 1507.36, 64: 2828.86 } } },
    M4:   { gen: 39.46, verify: 0.52,
            native: { single: 39.46, verifySingle: 0.52, foldPerMsg: 361.2,
                      batch: { 4: 45.03, 8: 62.50, 16: 76.37, 32: 121.16, 64: 205.70 },
                      verifyBatch: { 4: 0.81, 8: 1.23, 16: 2.13, 32: 3.93, 64: 7.51 } } },
    RPi4: { gen: 766.8, verify: 18.7 },
    RPi5: { gen: 352.20, verify: 4.19,
            native: { single: 352.20, verifySingle: 4.19, foldPerMsg: 2293.2,
                      batch: { 4: 406.93, 8: 536.73, 16: 662.55, 32: 1145.83, 64: 1841.30 },
                      verifyBatch: { 4: 6.20, 8: 8.90, 16: 14.17, 32: 24.81, 64: 45.97 } } },
  },
  linkOneWayMs: 75,              // 150 ms RTT links
  bandwidthBps: 100e6 / 8,       // 100 Mbps, in bytes/s
  hops: 4, D: 6, nodes: 1000,
  maxMsgBytes: 150_000,          // TWN max message size
  // wire bytes — Groth16 BN254 proof + RateLimitProof fields (breakdown in 06 doc §4)
  perMsgOverheadB: 320,          // standalone msg: zerokit proof values EXACTLY (128 proof + 6x32 fields;
                                 // verified vs nwaku v0.38.1 codec). Wire adds +19 proto headers +103 framing
                                 // per standalone msg (omitted: conservative, favors standalone)
  perProofGroupB: 256,           // per group of k in the envelope: 128 Groth16 proof + 128 framing
  perMsgInEnvB: 64,              // (nullifier, y) per message in the envelope
  gSeconds: 20,
};

mkdirSync(new URL('./out/', import.meta.url), { recursive: true });
const out = (name, s) => writeFileSync(new URL(`./out/${name}`, import.meta.url), s);

// ---------- model ----------
const ltMs = bytes => C.linkOneWayMs + (bytes / C.bandwidthBps) * 1000;
// platform transfer: scale our-box times by the platform/our-box single-proof ratio (flagged
// approximation in the paper; tightened by re-running the prover on a Pi).
// Ref20's native block is the reference constants themselves
C.platforms.Ref20.native = { single: C.singleProveMs, verifySingle: C.singleVerifyOursMs,
                             batch: C.batchProveMs, foldPerMsg: C.foldProvePerMsgMs };
const genScale = p => C.platforms[p].gen / C.singleProveMs;
const verScale = p => C.platforms[p].verify / C.singleVerifyOursMs;
// a burst of B at capacity kCap: m proofs, the last one possibly smaller
const split = B => {
  const k = Math.min(B, C.kCap);
  return { k, m: Math.ceil(B / k) };
};

function envelopeBytes(scheme, B, payloadB) {
  const { m } = split(B);
  const shared = scheme === 'batch'
    ? C.perProofGroupB * m                              // one Groth16 proof + framing per group of k
    : C.foldProofBytes + 128;                           // one Spartan proof regardless of B + framing
  return B * payloadB + shared + B * C.perMsgInEnvB;
}

function burstLatencyMs(scheme, B, payloadB, p) {
  if (scheme === 'permsg') {
    // B sequential proofs at the gateway; last message then crosses h hops.
    const bytes = payloadB + C.perMsgOverheadB;
    return B * C.platforms[p].gen + C.hops * (ltMs(bytes) + C.platforms[p].verify);
  }
  const env = envelopeBytes(scheme, B, payloadB);
  if (env > C.maxMsgBytes) return NaN; // exceeds TWN max message size
  const { k, m } = split(B);
  const prove = scheme === 'batch'
    ? m * proveBatchMs(p, k)
    : B * foldPerMsgMs(p);
  const verify = scheme === 'batch'
    ? m * (C.batchVerifyFit.v0 + C.batchVerifyFit.v1 * k) * verScale(p)
    : C.foldVerifyMs * verScale(p);
  return prove + C.hops * (ltMs(env) + verify);
}

const overheadPerMsgB = (scheme, B) =>
  scheme === 'permsg' ? C.perMsgOverheadB
                      : (envelopeBytes(scheme, B, 0)) / B;

// crossover B*: folded overhead/msg == per-message overhead/msg
const BSTAR = (C.foldProofBytes + 128) / (C.perMsgOverheadB - C.perMsgInEnvB);

const meshEdges = (C.nodes * C.D) / 2; // each mesh edge carries the message ~once
const netBytes = envBytes => envBytes * meshEdges;

// duty cycle: Omega = S/y (y = g); adversary sustained-rate advantage over honest average
const omega = S => S / C.gSeconds;

// wake-window feasibility: all m proofs bind the current epoch and relayers reject
// |now - epoch| > g, so proving + delivery must fit the window:
//   m * Lg_batch(k) + h * Lt  <=  g      =>   B_max = k * floor((g - h*Lt) / Lg_batch(k))
// To swap in platform-native measurements later: change C.platforms gen/verify and re-run.
const proveBatchMs = (p, k) => {
  const nat = C.platforms[p].native;
  const table = nat ? nat.batch : C.batchProveMs;
  const ks = Object.keys(table).map(Number).sort((a, b) => a - b);
  const kk = ks.reduce((best, x) => (Math.abs(x - k) < Math.abs(best - k) ? x : best), ks[0]);
  return nat ? table[kk] : table[kk] * genScale(p); // native if measured, else ratio transfer
};
const deliveryMs = C.hops * C.linkOneWayMs;
const gRequiredS = (p, B) => { const { k, m } = split(B); return (m * proveBatchMs(p, k) + deliveryMs) / 1000; };
const maxBurst = p => C.kCap * Math.floor((C.gSeconds * 1000 - deliveryMs) / proveBatchMs(p, C.kCap));
const foldPerMsgMs = p => C.platforms[p].native?.foldPerMsg ?? C.foldProvePerMsgMs * genScale(p);
const maxBurstFold = p => Math.floor((C.gSeconds * 1000 - deliveryMs) / foldPerMsgMs(p));

// ---------- sweeps ----------
const SCHEMES = ['permsg', 'batch', 'folded'];
const BS = [1, 2, 4, 8, 16, 32, 64];
const PAYLOADS = [100, 1000];

let csv = 'platform,payload_B,B,m_proofs,scheme,burst_latency_ms,per_msg_ms\n';
for (const p of Object.keys(C.platforms))
  for (const P of PAYLOADS)
    for (const B of BS)
      for (const s of SCHEMES) {
        const L = burstLatencyMs(s, B, P, p);
        csv += `${p},${P},${B},${s === 'permsg' ? B : split(B).m},${s},${L.toFixed(0)},${(L / B).toFixed(1)}\n`;
      }
out('burst-latency.csv', csv);

csv = 'B,permsg_B,batch_B,folded_B\n';
for (const B of BS.concat([128, 256]))
  csv += `${B},${overheadPerMsgB('permsg', B).toFixed(0)},${overheadPerMsgB('batch', B).toFixed(0)},${overheadPerMsgB('folded', B).toFixed(0)}\n`;
out('overhead.csv', csv);

csv = 'sleep_s,omega,advantage_single_window,advantage_dual_window\n';
for (const S of [60, 300, 900, 3600, 4 * 3600, 12 * 3600, 24 * 3600])
  csv += `${S},${omega(S).toFixed(0)},${omega(S).toFixed(0)},1\n`;
out('duty-cycle.csv', csv);

// feasibility table (paper Table 2): min g per (platform, B) + max burst at deployed g
const FEAS_BS = [8, 32, 64, 128, 256, 512];
csv = 'platform,' + FEAS_BS.map(B => `g_min_s_B${B}`).join(',') + ',maxB_at_g20_k8,fold_maxB_at_g20\n';
for (const p of Object.keys(C.platforms))
  csv += `${p},${FEAS_BS.map(B => gRequiredS(p, B).toFixed(1)).join(',')},${maxBurst(p)},${maxBurstFold(p)}\n`;
out('feasibility.csv', csv);

// ---------- minimal SVG line charts (no deps) ----------
function svgChart(title, xlab, ylab, series, logY = false) {
  const W = 640, H = 400, m = { l: 60, r: 130, t: 36, b: 44 };
  const xs = series.flatMap(s => s.pts.map(p => p[0]));
  const ys = series.flatMap(s => s.pts.map(p => p[1])).filter(Number.isFinite);
  const ty = v => (logY ? Math.log10(v) : v);
  const [x0, x1] = [Math.min(...xs), Math.max(...xs)];
  const [y0, y1] = [Math.min(...ys.map(ty)), Math.max(...ys.map(ty))];
  const X = x => m.l + ((x - x0) / (x1 - x0 || 1)) * (W - m.l - m.r);
  const Y = y => H - m.b - ((ty(y) - y0) / (y1 - y0 || 1)) * (H - m.t - m.b);
  const colors = ['#1f77b4', '#d62728', '#2ca02c', '#9467bd'];
  let s = `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" font-family="sans-serif" font-size="12">
  <rect width="${W}" height="${H}" fill="white"/>
  <text x="${W / 2}" y="18" text-anchor="middle" font-weight="bold">${title}</text>
  <text x="${(m.l + W - m.r) / 2}" y="${H - 8}" text-anchor="middle">${xlab}</text>
  <text x="14" y="${H / 2}" text-anchor="middle" transform="rotate(-90 14 ${H / 2})">${ylab}</text>
  <line x1="${m.l}" y1="${H - m.b}" x2="${W - m.r}" y2="${H - m.b}" stroke="black"/>
  <line x1="${m.l}" y1="${m.t}" x2="${m.l}" y2="${H - m.b}" stroke="black"/>\n`;
  for (const x of xs.filter((v, i, a) => a.indexOf(v) === i))
    s += `  <text x="${X(x)}" y="${H - m.b + 16}" text-anchor="middle">${x}</text>\n`;
  for (let i = 0; i <= 4; i++) {
    const yv = y0 + ((y1 - y0) * i) / 4;
    const lab = logY ? Math.round(10 ** yv) : Math.round(yv);
    s += `  <text x="${m.l - 6}" y="${H - m.b - ((H - m.t - m.b) * i) / 4 + 4}" text-anchor="end">${lab}</text>\n`;
  }
  series.forEach((sr, i) => {
    const pts = sr.pts.filter(p => Number.isFinite(p[1]));
    s += `  <polyline fill="none" stroke="${colors[i % 4]}" stroke-width="2" points="${pts.map(p => `${X(p[0]).toFixed(1)},${Y(p[1]).toFixed(1)}`).join(' ')}"/>\n`;
    s += `  <text x="${W - m.r + 8}" y="${m.t + 16 + 18 * i}" fill="${colors[i % 4]}">${sr.name}</text>\n`;
  });
  return s + '</svg>\n';
}

out('burst-latency-rpi4.svg', svgChart(
  'Burst delivery latency, RPi4 gateway, 100 B payloads (k=8)', 'B (messages per burst)', 'ms (log)',
  SCHEMES.map(sch => ({ name: sch, pts: BS.map(B => [B, burstLatencyMs(sch, B, 100, 'RPi4')]) })), true));
out('overhead.svg', svgChart(
  `Wire overhead per delivered message (B* = ${BSTAR.toFixed(0)})`, 'B (messages per burst)', 'bytes (log)',
  SCHEMES.map(sch => ({ name: sch, pts: BS.concat([128, 256]).map(B => [B, overheadPerMsgB(sch, B)]) })), true));
out('duty-cycle.svg', svgChart(
  'Adversary sustained-rate advantage vs sleep period (y = g = 20 s)', 'sleep S (hours)', 'advantage x (log)',
  [{ name: 'single window', pts: [1, 4, 8, 12, 24].map(h => [h, omega(h * 3600)]) },
   { name: 'dual window', pts: [1, 4, 8, 12, 24].map(h => [h, 1]) }], true));

// ---------- sanity asserts (fail loudly) ----------
for (const p of Object.keys(C.platforms))
  for (const B of BS.filter(b => b >= 2))
    if (!(burstLatencyMs('batch', B, 100, p) < burstLatencyMs('permsg', B, 100, p)))
      throw new Error(`batching not winning at B=${B} on ${p}`);
for (let i = 1; i < BS.length; i++)
  if (!(overheadPerMsgB('folded', BS[i]) < overheadPerMsgB('folded', BS[i - 1])))
    throw new Error('folded overhead not monotone decreasing');
if (!(BSTAR > 20 && BSTAR < 300)) throw new Error(`BSTAR out of expected range: ${BSTAR}`);

// ---------- summary ----------
const f = (s, B, p) => burstLatencyMs(s, B, 100, p);
console.log('=== model summary (100 B payloads, k=8 capacity) ===');
console.log(`RPi4 gateway, B=8 burst:   per-msg ${(f('permsg', 8, 'RPi4') / 1000).toFixed(1)} s | ` +
  `batched ${(f('batch', 8, 'RPi4') / 1000).toFixed(1)} s | folded ${(f('folded', 8, 'RPi4') / 1000).toFixed(1)} s`);
console.log(`RPi4 gateway, B=64 burst:  per-msg ${(f('permsg', 64, 'RPi4') / 1000).toFixed(1)} s | ` +
  `batched (m=8 proofs) ${(f('batch', 64, 'RPi4') / 1000).toFixed(1)} s | folded ${(f('folded', 64, 'RPi4') / 1000).toFixed(1)} s`);
console.log(`M1 gateway,   B=8 burst:   per-msg ${f('permsg', 8, 'M1').toFixed(0)} ms | ` +
  `batched ${f('batch', 8, 'M1').toFixed(0)} ms | folded ${f('folded', 8, 'M1').toFixed(0)} ms`);
console.log(`overhead/msg at B=8: per-msg ${overheadPerMsgB('permsg', 8)} B | batched ${overheadPerMsgB('batch', 8).toFixed(0)} B | folded ${overheadPerMsgB('folded', 8).toFixed(0)} B`);
console.log(`folded-vs-permsg bandwidth crossover B* = ${BSTAR.toFixed(1)}  (the batched envelope is cheaper than both up to the envelope cap)`);
console.log(`network bytes per B=8 burst (mesh x${meshEdges}): batched env ${(netBytes(envelopeBytes('batch', 8, 100)) / 1e6).toFixed(1)} MB | ` +
  `folded env ${(netBytes(envelopeBytes('folded', 8, 100)) / 1e6).toFixed(1)} MB | ` +
  `8 separate msgs ${(netBytes(8 * (100 + C.perMsgOverheadB)) / 1e6).toFixed(1)} MB`);
console.log(`Omega at S=1h: ${omega(3600)} (single window advantage) -> ~1 with dual window (+29-44% constraints)`);
console.log('feasibility (k=8): ' + Object.keys(C.platforms)
  .map(p => `${p} maxB@g20=${maxBurst(p)} (fold ${maxBurstFold(p)})`).join(' | '));
console.log('wrote out/*.csv, out/*.svg — all sanity asserts passed');
