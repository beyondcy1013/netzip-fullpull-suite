# Hypothesis ledger

## Open

- For callback parity joining at the 09:25 auction boundary, exact callback
  business timestamp should remain a candidate filter, but ranking candidates
  primarily by callback batch wall-clock distance may outperform ranking after
  business timestamp with frame proximity and the live-leftover special case.
  A controlled timestamp-corrected auction replay supports this as a diagnostic
  join-policy effect: unchanged matched records, overall joint
  12,107/27,712 (43.69%) rose to 12,985/27,712 (46.86%), while
  callback-nonzero joint remained 3,869 and the callback-nonzero denominator
  fell from 13,084 to 11,500. This is one replay and does not establish Wine
  event semantics or decoder correctness. A second timestamp-corrected
  close-window replay is a negative control: both policies selected the same
  five callback sequences and had identical decoded/matched/field/trade
  metrics, including zero overall joint and zero nonzero joint. Therefore any
  future default must scope wall-time ranking to auction-open-like wall-time
  clustering or prove a broader mechanism; do not generalize the auction gain.
- The remaining nonzero-callback scalar gap is dominated by unproven snapshot
  or initialization state rather than callback candidate ambiguity: in the
  auction wall-time replay, 14/20 unique candidates have zero Rust
  price/volume/amount while Wine is nonzero, and 12/14 have no nonzero
  predecessor in the captured Rust flow. This is a state-provenance/category
  mapping hypothesis, not a decoder arithmetic or join-policy finding. Falsify
  with independently established native Wine state at the flow/category
  boundary.
- SZ000858 negative volume may originate in earlier record read-head drift,
  native/Rust state selection, or native token selection. Corrected v2 audit
  reproduces Rust's M token and the entire prefix boundary; only paired native
  reader/state evidence can distinguish these explanations.
- The coupled baseline ladder fix (move continuation + unconditional baseline
  volume mask + independent volume decode) corrects downstream alignment.
  Native frame 258 record 0 demonstrates the missing control flow, but the
  former mask-only experiment's failure counts are not evidence for this
  coupled change. Falsify using native record boundaries and all three replay
  fixtures with identical input state provenance.
- Native EOF commits may represent correct native state evolution, but their
  business values and downstream propagation still require Wine-state parity.
- Zero-valued sparse fields need an explicit presence model; byte-nonzero merge
  behavior may retain stale values.

## Rejected

- V1's `B(16)=22129` and unexplained 12-bit tail describe Rust record 25:
  rejected by v2's envelope-correct audit. The actual token is M at 1490..1525,
  raw `0x20f3f858`; signed 30-bit mantissa is `-520882088`, exponent zero.
  Prefix end matches 1553 exactly. The former rejection of pre-accumulator
  drift is withdrawn: the cited boundaries came from Rust output, not paired
  native execution. Do not treat Rust decoded-values as Wine memory.
- Missing fresh baseline alone explains dirty quotes: rejected by the 5,000
  trace symbol-group comparison.
- OEM levels 6-10 are required upstream truth: rejected; they are compatibility
  zeros and do not drive ladder decoding.
- NativeWineClamp zero frame errors imply decoder completion: rejected by
  dynamic OEM business-field parity and runtime rejection evidence.
- NativeWineClamp is a stateless tail repair with no stored-last bleed:
  rejected by close-window prefix-mismatch 22,995/101,765 frames and the
  168-clone SZ 2344 stored-last 1664→1612 follow-on frame.
- `0x44aa30` post-loop `vtable+0x78` (`0x44ad87`) converts 311-byte rows into
  OEM/callback quotes: rejected. Constructor `0x4495e0` installs vtable
  `0x5c591c`; slot `+0x78` is `0x44e180`, which zeros `eax` and `ret 8`.
- The callback projection path is instead category-dispatched:
  `0x496850 -> 0x496740 -> 0x4998c0 -> vtable+0x74`. Vtable `0x5c591c+0x74`
  is a null stub (`0x44e140`), while `0x5c59a0+0x74` is `0x423180`, routing to
  `0x496270 -> 0x496490 -> global+0x1e03b94 -> 0x443ca0 -> 0x46b5e0`.
  Reconstruct `0x46b5e0` and the 0x3a0 ring-entry field offsets next; do not
  infer quote semantics from the rejected +0x78 stub.
- The `0x46b5e0` receiver is specifically `owner+0x1e03b94`, and the concrete
  projection call is `0x46b5e0(this, key, state, request, ring_entry, 2)`,
  followed by `[this_vtable+0x1c](request, node, ring_entry)`. Runtime
  `this_vtable` resolution and its `+0x1c` implementation remain required
  before mapping ring-entry price/volume/amount fields.
- Runtime vtable resolved: constructor `0x468610` installs `0x5c5af0`, and
  `0x5c5af0+0x1c = 0x469420`. Therefore the static projection chain terminates
  in `0x469420(request, node, ring_entry)`. Reconstruct `0x469420` field reads
  next; this is the decisive callback scalar mapping site.
- `0x469420` delegates to `0x469460` (ring identity/preparation only). The
  scalar packing candidates are the three output builders called by
  `0x49d7b0` before common publishing through `0x496490`: `0x4a67a0`,
  `0x4a7540`, and `0x4a6dc0`. Map their ring-entry reads next.
- Price/OHLC mapping reconstructed in `0x4a67a0`: price uses signed int at
  subrecord `+0x29e` (fallback `+0x11e`) divided by float32 at `+0x98`;
  open/high use `+0x296/+0x11e` and `+0x29a/+0x13a`; low comes from `+0x9d`;
  levels use count `+0xe5+1` and 12-byte records at `+0xe6`. These are
  separate subrecord fields, not direct 311B decoder record scalars. Validate
  SH513130/SH113640 through these offsets next.
- Volume/amount mapping reconstructed: eligible subrecords are selected by
  `+0x1f2`; volume fields include integer `+0x1f2`, `+0x122/+0x126/+0x12a`,
  `+0x206`, and level arrays `+0x232/+0x272/+0x212/+0x252`; scaled floats use
  divisor `+0x98` with sources `+0x292/+0x1f6/+0x1fa/+0x1fe`, while float
  `+0x1e6` and `cvtsi2ss +0x17d` feed auxiliary outputs. Therefore callback
  price/volume/amount are separate subrecord projections, not 311B decoder
  record scalars. Falsify by offset-level replay against retained callbacks.
- Existing `mem-t*.records.json` cannot falsify this map: they are 311-byte
  decoder-side committed records (`ts/open/high/low/last/volume/amount` at
  `0x0..0x1c`, `last_close` at `0x12b`) and lack the `0x1f4/0x3a0` projection
  subrecords. Numeric validation requires a new read-only capture of the
  `global 0x5eaf70` and `0x5eaf60` tables in the same session as callbacks.
- Numeric validation on the cold-start fixture proved the price/OHLC domain
  instead: across 5,510 volume-gated callback/committed-record pairs,
  price/open/high/low/last_close match exactly (4,657 scale100 and 853
  scale1000 codes, zero ambiguous). Therefore the Wine callback price/OHLC
  equals the 311B committed-record integers under per-symbol decimal scaling.
  The remaining parity gap is the parity tool's projection/scaling of those
  committed records and the amount subrecord domain, not callback arithmetic.
- Index-stable cross-day `0104` supplies OEM `last_close`: rejected by the
  09:04 auction control (`verified-4068` 198/17,074 versus same-day 09:09
  seed 25,862/25,862).

Each hypothesis promotion requires a falsifying test, evidence path, and a
focused regression test.
- Applying the validated committed-record mapping to the 20 auction retained
  samples separates three residual classes: SH113640/SH113659 already project
  price exactly (volume/amount-only mismatches); zero-record snapshot-gap
  codes are predicted by the mapping because Wine projects its own snapshot;
  and nonzero-record price mismatches (SH603500/SH688093 class) need the
  transaction/subrecord domain. Verify the volume/amount mapping with a
  `0x5eaf60` capture; decoder change still not justified.
- Callback volume has two distinct scopes confirmed on the cold-start fixture:
  cumulative session volume appears in the main quote callback (matching the
  311B committed record when snapshots align), while per-update volume deltas
  (SH113640's value 30) appear in a separate rolling-query callback event.
  The parity example currently projects cumulative committed state but joins
  against both scopes, inflating volume mismatches. A dual-scope projection
  or a callback-source discriminator is required; verify by classifying
  callback event types in the auction capture.
- Dual-scope callback volume proved: each code receives exactly two callback
  events (first=per-update delta, second=cumulative snapshot). The second
  callback volume equals the 311B committed record in 5,465/6,106 cases (first
  never matches). The parity tool must discriminate scope before comparing;
  using the second callback (or the `entry+0x860` flag) as the snapshot source
  and the first as the delta source should resolve the volume mismatch class.
  Falsify by implementing scope-aware projection and re-running auction parity.
- Close-window verification confirms cumulative callback volume: across 2,203
  codes with ≥2 callbacks, volume is monotonically non-decreasing (19,256
  increases, 31 decreases, 57,797 unchanged). The correct parity projection
  is: use the latest callback per code as the cumulative snapshot. The cold-
  start dual-scope pattern (first=delta, second=snapshot) was an artifact of
  only having 2 callbacks per code where the first preceded the trading
  session. Falsify by implementing latest-callback projection on close-window
  and measuring volume parity improvement over wall-time-nearest.
