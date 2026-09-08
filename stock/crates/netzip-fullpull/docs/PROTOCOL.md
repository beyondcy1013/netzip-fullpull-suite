# Promoted protocol rules

Only independently supported rules belong here. Exploratory interpretations
remain in `HYPOTHESES.md`.

## Relative baseline tail precedence (Conditional Native Evidence)

When a relative record receives an explicit311B baseline, native initialization
copies that baseline including its tail; separate metadata must not overwrite
the copied tail. Two controls hold metadata+0x100 at0 and retain baseline
markers90/165. Absolute and missing-baseline paths are separate contracts;
this rule does not authorize manufacturing a baseline. See EVIDENCE_INDEX.md,
"Relative baseline tail precedence", for pinned native artifacts and replay
scope. Verified function behavior is distinct from full consumer promotion
and live-session business parity, whose current gates remain in STATUS.md.
Relative-tail local regression promotion passed consumer20/20 in110115;
this does not close initial-state provenance or live business parity.

## Session identity

- Code, name, decimal scale, and previous close come from the current login's
  complete `0104` row.
- Join identity is session/flow scoped. Never reuse an index map across login or
  reconnect, and never use the six-byte identity residue in `2704` as canonical.
- Index-layer `uses_baseline` is not the value decoder's 311-byte baseline flag.
  Value baseline selection is controlled by the value mask semantics.

## Initialization

- Retain ten initialized 5188 sockets.
- Seven observed subscription slots send `2a10`; three initialized sockets must
  not receive invented subscriptions.
- `2a10` comparisons require the same `0104` version on both sessions.

## Native EOF behavior

For executable SHA256
`de712a8dde6d990e1c586f8afd4194575e35dffa2d0f81245fe29f6f8509bd29`:

- `0x448a10` clamps a read to remaining bits and returns zero without advancing
  after EOF.
- `0x448d10`/`0x448b90` return zero without consuming a prefix on token miss.
- `0x44aa30` continues through record count.
- `0x44ad7d` commits the 311-byte internal record.

This is a native mechanism rule, not proof that the current Rust token tables,
field semantics, or public projection are correct.

## Native Amount Adjustment Order

For the pinned executable, `0x44a10e` decodes the amount token with the raw
volume/last projection as previous (zero for mode0). Then `0x44a119..0x44a15f`
applies `(decoded + 1) * (u32_adjustment + 1)` for modes0/8 with nonzero
adjustment, in wrapping 64-bit arithmetic. Baseline amount is added afterward.
Conditional execution with historical frame258 record25 bytes, changing only
input metadata, confirms a nonzero token residual -34,751: mode0 adjustment2
yields -104,250, not -34,748. Mode8 projection -3,784,729,251,408 with the same
adjustment yields -11,354,187,858,474. Executable bytes are unmodified; these
metadata controls prove local function ordering, not live quote correctness.
Regression: `quoteNetzipRs/scripts/test_replay_official_5188_native_record.py`
`test_nonzero_amount_token_adjusted_after_decode`.

## Native Caller Baseline Selection

For the pinned executable, `0x44ac34..0x44ac3b` derives the relative flag
from value mask bit0. `0x44ac93` resolves market/index; `0x44ace5` then
resolves the returned code string and connection category (`input+0x2e8`).
An empty second lookup exits the frame loop before `0x449770`. A nonempty
lookup supplies the baseline only when the relative flag is set.
`0x4157b0` market/index lookup misses return an embedded fallback record
(lookup-object+0x5524e); `0x415820` code/category lookup returns null on miss
or category>=100. Neither inspected lookup function creates a record.
This static control flow does not establish the historical contents or
initialization policy of either map. A null baseline passed directly with
relative flag set is not a reproduction of this caller's successful path.

## Native Subscription Record Allocation

For the pinned executable, metadata loader `0x415890` uses the code map at
lookup-object+0x553bd, creates missing records through `0x415be0`, copies
the 68-byte metadata row to record+0xf3 and inserts market/index mappings.
This map is distinct from the category-specific decoder-state maps.

Subscription builder `0x49b550` selects the latter at
parent+0x79c95+category*0xe0 (parent+0x247f8 is the lookup object).
At `0x49b6fc` it resolves code in the metadata map; on success `0x49b725`
calls `0x49c1c0` with create=true before appending the six-byte subscription
entry at `0x49b750`. Packet initialization `0x49c420` uses type0x102a
(wire bytes 2a10). The create helper preserves an existing map entry;
on a miss it allocates through `0x49c280` and inserts through `0x416aa0`.
Both fresh and recycled allocations call `0x4183a0`, which clears exactly
0x137 bytes. This creates independent zero-initialized category state,
not a copy of the metadata record's business fields.

These are static construction rules, not proof that a particular historical
symbol was subscribed or remained unchanged until its first captured 2704.
Do not equate allocation with publication readiness or invent missing seeds.

## Native Baseline Ladder Control

For the pinned executable above, `0x44a360` sets layout continuation false
for move -1, -4 and ordinary moves; -2 retains continuation and -3 sets
merge flag 2 while retaining continuation. At `0x449c14..0x449c34`, a
non-null baseline causes a `0x5b291c` volume-mask token read regardless of
the continuation flag. `0x449c62` then handles volume decoding separately
from the optional layout call at `0x449c5c`.

Offline native execution of close-window frame 258 record 0 confirms the
-1 path with mask read 44..55, volume tokens through 102, trailing token
through 108. Its starting baseline is Rust-derived; this confirms conditional
function behavior, not live Wine state provenance or whole-protocol parity.

## Projection Rules

- Public projection uses same-session `0104` metadata.
- Publish timestamp is floored to local 15:00 after close.
- STAR lot conversion uses positive half-up `/100`; a priced zero-volume level
  displays one lot. Levels 6-10 are compatibility zeros and are not a decoder
  gate.
- Runtime candidate validation remains transactional and fail-closed.
