# energy-battery — zenSDK client

Direct control of Zendure SolarFlow batteries over the vendor's **local** HTTP API. No cloud, no
token, no TLS, no MQTT. Protocol facts and their evidence tags live in
`reports/energy-rs/zensdk.md`; this crate implements §2 (transport), §3 (report), §5 (write),
§6.1–6.3 (discovery) and §7.1 (capacity labels).

## Protocol summary

| Item | Value |
|---|---|
| Scheme / port | plain `http`, port **80** (mDNS SRV wins when present) |
| Auth | **none** — a 401 on HTTP means "local API disabled", a body `code: 401` means "serial mismatch" |
| Read | `GET /properties/report` → ≈2 KB JSON, 5 s timeout, 64 KB defensive cap |
| Write | `POST /properties/write`, body `{"sn","id","source":2,"properties":{…}}`, **≤ 512 bytes** |
| Ack | `success: true` **and** `code: 200` **and** `sn == ours`; HTTP 200 alone proves nothing |
| Rejections | `400` bad parameters, `401` serial mismatch, `413` receive buffer overflow (each typed) |
| Timing | report rate = whatever you poll; setpoint lands in ~1.8 s; power flows after ~8 s |

Report envelopes: `properties` may hold everything, or the packs may sit at the top level; `packData`
may be nested under `properties`. Parsing merges top-level scalars with `properties` (properties win),
looks for the pack array in either place, and ignores unknown keys (`net`, `aiState`, `Fansense`, …).

Unit decoding (§3.1), all in `report/decode.rs`:

| Raw | Real |
|---|---|
| `socSet`/`minSoc` | deci-percent: `1000 → 100.0 %`, `100 → 10.0 %` |
| `totalVol` | centivolts: `4810 → 48.10 V` |
| `maxVol`/`minVol`/`BatVolt` | 0.01 V: `321 → 3.21 V`, `4935 → 49.35 V` |
| `batcur` | signed 16-bit deci-amps: `0xFFFF → -0.1 A` |
| `hyperTmp`/`maxTemp` | 0.1 K: `(2993 − 2731)/10 = 26.2 °C` |
| `softVersion` | packed: `4355 (0x1103) → "1.1.3"` |
| `remainOutTime` | minutes; `59940` (999 h) is the invalid sentinel → `None` |
| `gridReverse` | 0 disabled, 1 export allowed, 2 export forbidden |
| `socLimit` | 0 none, 1 upper bound closed, 2 lower bound closed |

**There is no energy counter anywhere in a report** — no kWh, no daily totals. The only capacity
figure is the pack label, from `packData[].sn` prefix + `packType` (`AB2000 = 1.92 kWh`,
`I8000 = 8.0 kWh` with `packType 70`); everything else is "on Zendure's word" and is meant to be
learned from sessions (P6), never assumed.

## Write shapes

| Command | Properties |
|---|---|
| Charge | `{"smartMode":1,"acMode":1,"outputLimit":0,"inputLimit":W}` |
| Discharge | `{"smartMode":1,"acMode":2,"outputLimit":W,"inputLimit":0}` |
| Hold | `{"smartMode":1,"outputLimit":0,"inputLimit":0}` — **no `acMode`** |
| Trim | `{"smartMode":1,"inputLimit":W}` or `{"smartMode":1,"outputLimit":W}` |

`smartMode` is serialized first in every body. Charge/discharge zero the other limit in the same
body so a stale limit from the direction just left cannot linger.

## Pacing and flash rules

* **≥ 2 s between writes to one device, one command in flight.** The device drops a command that
  follows another within ~1.3 s (25 % lost at 1/s, none at 1 per 2 s), and a fast write loop has
  wedged `acMode` outright. `control/pacer.rs` owns the floor: the mutex is the in-flight slot and
  the completion instant is recorded when the claim drops.
* **`smartMode: 1` (RAM) on every control write.** `0` would persist to flash; the device resets the
  flag to 0 on reboot, so it is re-asserted rather than assumed. A hold is a RAM hold (~30 W idle);
  the release variant (`smartMode: 0`, ~5 W) is deliberately not implemented in P5.
* **The write is not the truth; the next report is.** `poll()` verifies the pending command against
  the new report: the limit must echo within **5 W** (the vendor tolerance), the direction must
  match when `acMode` is reported, and a hold must show both limits at 0. A mismatch is a typed
  `ControlFault`, not a retry loop. Only a report *newer* than the pre-write one counts: it must
  carry the device's serial and move `messageId` (and not the device clock) forward, so a repeated
  or pre-command report cannot "confirm" a hold whose 0 W limits it already showed. A duplicate
  report is no evidence at all and leaves the command in doubt.
* **One transaction per device.** A write takes the pacer's single in-flight slot *first*, then
  re-reads the state, re-clamps against the live maximum and transmits — waiting for the slot can
  take seconds, during which a poll can discover a lower limit, a restart or a wedge. The clamped
  command is installed as pending *before* the request leaves, and the readback (fetch + fold) never
  interleaves with a send. While a command is unresolved (no fresh report yet) the next write is
  refused: an unverified command is never silently replaced.
* **No report, no write.** `apply()` refuses until the device has been read at least once, and a
  device that comes back from a reboot is re-read instead of trusted.
* **Wedge / restart.** `acMode` outside `{0, 1, 2}` (`127`/`251`/`255` seen in the field) is a
  `Wedged` fault: nonzero writes are refused until a report shows a usable direction again. A hold
  still goes out, at most `MAX_EMERGENCY_HOLDS` (3) times per wedge episode and logged, because it
  is the one write the verified shape allows there (it carries no `acMode`) and it takes the pack
  off the AC side instead of leaving it at the last setpoint. A `messageId` that jumps backwards
  across the 8-bit ring, or a device clock that goes back, is a `Restarted` fault: the pending
  command is dropped, the next decision is made from the fresh report, and the pacer is pushed out
  by `FAULT_BACKOFF` (10 s) so "back off, re-read" is enforced, not suggested.
* **Clamping.** Requested watts are clamped to the tightest known ceiling: `min(config,
  chargeMaxLimit / inverseMaxPower, hard limit)`. A reported or configured `0` means "unknown",
  never "no power" and never "unlimited": an `Unknown` ceiling is its own case and a nonzero
  command is refused (`BatteryError::NoCeiling`) instead of being transmitted unbounded. The
  fallback is a validated, model-specific `HardLimit` (nonzero, at most 4000 W); with no trusted
  ceiling at all, no power is the safe answer.

## Discovery

`browse_mdns(window)` browses `_zendure._tcp.local.` and maps `zendure-<product>-<sn>.local` to
`(product, sn)` (the serial must look like one: 8+ alphanumerics, which rejects meter suffixes such
as `3ct`). Manual IP + SN entry is first class. `DiscoveryRegistry` merges everything by serial: a
manual address outranks mDNS, model hints are filled in from either source, and repeat announcements
change nothing.

## Modules

```
transport/   Endpoint, HttpTransport, WriteRequest/WriteProperties/WriteAck, DeviceIo trait
report/      tolerant parser, unit decoders, Properties/PackReport/PackLabel, SoC/export state
control/     Command, ControlLimits + clamp, Pacer, echo verify, wedge/restart watch, DeviceController
discovery/   mDNS browse, hostname pattern, DiscoveryRegistry (merge + dedupe)
fleet/       FleetClient (staggered polls, cache, weighted SoC, fan-out), snapshots/outcomes
fake/        in-process device on 127.0.0.1:0: echo, dropped writes <1.3 s, wedge, 401/400/413
learning/    capacity from AC per SoC point + round trip from closed cycles, recency-weighted,
             MAD-trimmed, banded to ±25 % of the label and capped at the reported capacity
protection/  latching low-cell guard (cutoff/resume/stale, persisted), weekly + manual full charge
             in local Europe/Amsterdam time, DST-safe
```

## Tests

`cargo test -p energy-battery` — 134 tests, no hardware:

* report parsing (both envelopes, every conversion, unknown keys, capacity labels, pack sums);
* exact JSON payloads for charge/discharge/hold/trim, the 512-byte cap and ack validation;
* clamping against config, device, hard and unknown ceilings (a nonzero command with no ceiling is
  refused);
* pacing (second write waits, a queued writer cannot replace an unresolved command), the pending
  command installed before the request leaves, re-clamping after the slot wait, echo confirmation
  inside/outside 5 W, duplicate/foreign reports refused as evidence, hold-without-`acMode`, wedge
  plus the bounded emergency hold, restart and its back-off;
* discovery hostname parsing and registry merge/dedupe;
* fleet aggregation, staggered polling, dead-device partial failure and fan-out outcomes;
* learning: exact capacity/round-trip recovery from synthetic sessions, recency dominance, MAD
  rejection, band and reported-capacity clamps, refusals, kept-previous on thin data, profile
  persistence;
* protection: the full latch matrix (cutoff/resume/stale/missing/reboot), the emergency power
  minimum, full-charge hold/release, idempotency, run expiry and both DST transitions;
* the fake device end to end over real HTTP: charge, discharge, hold, dropped writes, wedge, and the
  400/401/413 mappings.

The live test is `#[ignore]`d and read-only (no write is ever sent from a test):

```bash
ENERGY_RS_LIVE_HOST=192.168.1.31 ENERGY_RS_LIVE_SN=EOC1NLN9N4XXXXXX \
  cargo test -p energy-battery --test live -- --ignored --nocapture
```

## Still unverified on hardware `[U]`

* Whether an 800 Plus honours `inputLimit` alone (a trim) without `acMode=1`; the reference
  implementation trims like that on a 3000 Mix only.
* Whether firmware 2.0.59 writes flash even with `smartMode: 1` (a field report suspects it does).
  Mitigation is the pacing floor and few, on-change writes — not a proof.
* The write-rate ceiling before the AC path wedges, and whether a power-cycle is always enough to
  recover.
* Whether `smartMode` is echoed back as `1` after a write on every firmware — the controller
  therefore re-asserts it instead of verifying it.
* `writeRsp` meaning, and whether it is ever a usable per-write ack.
* Whether the write ack's `messageId` is the post-write counter: it is used only as an extra lower
  bound for confirmation when it is ahead of the last report, and ignored otherwise.
* Whether `messageId` advances on *every* report on every firmware. Confirmation freshness and
  restart detection both rest on the 8-bit counter, so a report that repeats it is treated as "no
  news" — a firmware that keeps the counter still would keep a command in doubt forever (the safe
  failure: no further writes).
* The restart heuristic: no uptime field exists, so a wrap and a reboot can be confused; the
  `messageId` window (half the 8-bit ring) plus the device clock is the best signal available.
* Reversal dead time on the 800 Plus (measured on an SF3000 only).
* Multi-pack hosts: whether `electricLevel` averages packs and how one limit splits across them.
* TLS/IPv6: never observed; the client speaks plain HTTP on IPv4 or a hostname only.
