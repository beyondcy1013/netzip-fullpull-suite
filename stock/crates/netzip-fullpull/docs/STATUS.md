# Official 5188 status

## Current Relative-tail Gate: Passed

110115-18d31111735c6f55 completed20/20 consumer checks. Management read
the matrix report and verified SHA256
40ca0bc822591255cfcf0bcf5c5e39273f2b7fccfa0ec30aaccf084e3312297c;
current official_5188.rs matches report sourcea99b072ba43f03a2ea0cc1538a65353a2455d8242612b8ec50355e418cba13b1.
Together with the conditional baseline-tail controls and three-fixture
1886043-record equality report, the local regression gates are closed.
Semantic replay remains bound to2ba2d9f4; a99b072b is the test-format revision.
Terminal27 released decoder ownership and is assigned precise entry/commit
capture design; terminal29 owns existing-dump metadata binding. Both task
messages were confirmed submitted. No live business parity, deployment, or
GitHub update is implied; those remain separate work.

The following gate receipts are historical and superseded by this checkpoint.

## Latest gate receipts and bridge readiness

Management verified110111: fmt/7 Rust example tests/Clippy/release and50
native/Rust scalar controls passed. Direct-mode bridge is ready for explicitly
bound metadata experiments, not default projection or business acceptance.
Terminal29 metadata manifest4093a616 was hash-checked; actual symbol category/
unit/digits and256B snapshot identity remain open and assigned to that owner.

Terminal27 latest receipt:103611 passed19/20 entries; only fullpull-fmt failed.
Owner made test-format-only repair (sourcea99b072b, reported production prefix
unchanged) and queued110115-18d31111735c6f55. Owner reports128402 decoded files/
1886043 records identical across three fixtures,105419 full311B golden+Clippy
passed and104629 native15 tests passed. These owner results supersede earlier
pending comparisons below, but management has not yet independently read their
artifacts; full consumer gate stays pending until110115 is verified.

## Historical Relative-tail Gate Checkpoint (Superseded Above)

Terminal27 reports source2ba2d9f4 focused103306 passed12 Rust and14 native
tests: focused-pass/full-gates-pending, not business parity. Three-fixture
request103558-18d31111735c6f4d explicitly builds the release extractor first;
consumer matrix103611-18d31111735c6f4e is pending. Terminal27 retains shared
decoder single-writer ownership until both gates close. Matrix runner removes
cross-workspace CARGO_TARGET_DIR overrides with a regression, per owner report.

Latest owner receipt:103558 completed status0 at10:49:35; source/input/binary/
seed stable, all three old/new ledgers and manifests byte-identical. Owner also
independently compared184 clone decoded JSON files/3664 records, all identical.
Full-init402137 and close-window1480242 record-level comparisons await104044
comparator verification; consumer103611 remains pending in the received report.
These are attributed owner results, not a management re-read of the artifacts.
Keep full-gates-pending and do not infer business parity from unchanged output.

OEM bridge example103240 passed6 Rust tests and32 conditional native/Rust
mode2 cases. Follow-up38-case report includes mode1/category9/overflow and
matches packed integers/float32 bytes. No real-session parity is inferred.

## Historical Ownership Checkpoint (2026-09-08)

Terminal27 reports a conditional full311B baseline-tail priority discrepancy
and may own a minimal official_5188.rs fix after confirming no competing writer
with terminal29. Required controls: relative baseline preserves its tail,
absolute metadata selection, and missing-baseline contract; then focused tests,
three fixtures and consumer matrix. Reported evidence is not a passed fix yet.
Management currently edits only the isolated bridge example/Python tests.
Bridge request102729 passed; new request102954-18d31111735c6f49 verifies the
native mode1 invalid-conversion correction (i32::MIN plus separate flag).
Nine Python native tests pass, including unscaled mode2 readback controls.
These example gates do not validate terminal27's shared decoder changes.

## OEM volume downstream correction (2026-09-08)

Conditional execution of 0x4a7995..0x4a79c6 (including getter0x40bb10)
confirms unsigned-u32-to-float32 interpretation of the stored volume word.
Do not confuse signed intermediate low-dword storage with final negative OEM
volume. Seven native vectors pass; total bridge suite is now four tests.
The isolated Rust example now emits oem_volume_f32. Prior example request
101426 passed; this new revision is queued as 102028-18d31111735c6f42.
Default projection is not replaced yet: copy acceptance and live category/state
binding remain open. Detailed scope lives in the consumer's
docs/forensics/oem-bridge-scalar-implementation-20260908.md.

## Bridge scalar implementation checkpoint (2026-09-08)

Consumer example `official_5188_bridge_scalars` now implements isolated
0x49ad20 scalar slices: low-dword signed volume and UTC-day time normalization.
Python `scripts/test_oem_bridge_scalars.py` executes the pinned native
instructions at 0x49af5e..0x49af6a and 0x49b009..0x49b04d using Unicorn.
Two tests / ten input vectors passed, covering low-word loss, signed boundary,
negative volume, UTC 07:00/16:00/day rollover and u32::MAX. Rust validation is
submitted separately; no Rust pass is asserted here. These are conditional
instruction slices, not full-function or live callback parity. Category9,
eligibility filters, amount compression and downstream conversion remain open.
The example is noncanonical and does not replace the current public projection.
Current Rust projection uses full i64 volume and China-local-day close clamping;
the observed intermediate conversion differs, but downstream output must be
verified before replacing that public behavior.

## Current user authorization (2026-09-08)

Decoder changes are permitted and expected when an error is established.
Unresolved hypotheses may be tested in isolated experiments; evidence-first
does not mean a blanket source freeze. Coordinate a single writer and preserve
baseline artifacts, then run focused regressions, fixtures and consumer checks.
Deployment and restart are permitted when needed for verification, through
webClx with a verified prior artifact, rollback and runtime checks. They are
not conditional on already achieving full business parity: isolated shadow
deployment can supply that evidence. Canonical publication remains a separate
business acceptance decision; publication is currently disabled.
Historical "no deploy", "source unchanged" and "read-only" entries below
describe those individual runs, not standing prohibitions. This section
supersedes older blanket restrictions; consult ACCEPTANCE for promotion gates.

## OEM bridge dispatch evidence (2026-09-08)

Pinned executable constructors install vtables 0x5c5694 (0x448ffa) and
0x5c5ea8 (0x49666a). Both slot+0x78 entries point to 0x49ad20.
The 2704 decoder 0x44aa30 itself loads that slot at 0x44ad8c and calls
it at 0x44ada3, passing this+0xa0130 and the record count. This establishes
the static dispatch route to the previously audited 311B-to-256B converter
when the receiver has either vtable. It is not proof of a captured session's
receiver identity or callback equality. The adjacent 0x44ade0 handler also
uses slot+0x78 at 0x44b139; do not confuse the two call sites.
Seven Python static-evidence tests pass. Decoder and runtime are unchanged.
Next gate: conditional native conversion regressions for scalar widths,
category9 scaling, time flooring and filtering, followed by session binding.
Raw evidence is indexed under OEM bridge virtual dispatch in EVIDENCE_INDEX.

Updated: 2026-09-07

Owner: `netzip-fullpull`

Consumers: `quoteNetzipRs`, `netzip_win`, `tdxRs/tdx-runtime`,
`netzip-supplement`, `stock-source-netzip`.

## Current verdict

- Authentication and endpoint selection: confirmed for authenticated 5188.
- Ten-slot initialization: structurally confirmed; same-version full P1-P7
  official/replica byte parity remains open.
- `0104` metadata and previous-close projection: confirmed for same-session
  metadata.
- `2a10` structure and P6/P7 ordering: confirmed offline; full same-window
  official parity remains open.
- `2704` NativeWineClamp: `mechanism-pass / business-fail`.
- Public publication: disabled.

NativeWineClamp reproduces the native EOF control flow in explicit opt-in
replay: clamped reads, unmatched-token zero, continued record loop, and 311-byte
commit. It does not establish business-field correctness.

Hashed close-window four-ledger (strict `extract/` vs
`extract-native-clamp-v1`, SHA256
`37bf0ac9724be1f152383fb50ad4e940986bdf305bb69279c0ba27655e0c5414`):
101,765 frames; strict 82,149/19,616 clean/error vs clamp 101,765/0;
completed 1,437,697 → 1,490,874 (+53,177); omitted 4,363×9,392 → 0;
`is_eof` 23,841×52,767; prefix-mismatch 22,995/101,765 (22.6%). Full-init
prefix-mismatch remains 4,896/26,722 (18.3%).

Exact-business-second callback comparison still has incomplete dynamic-field
parity. Overlapping 09:04 auction
(`extract-production-v3` × `callbacks/full-complete.jsonl`, OemState
`business-ts`) with the same-day 09:09 `0104` seed closes identity on this
window: name 25,862/25,862 and `last_close` 25,862/25,862. Price 7,296
(28.2%), amount 11,802 (45.6%), volume 10,186 (39.4%), book about 1%.
Hit counts include `0=0`. The previous-day `verified-4068` seed on the
same join yields `last_close` 198/17,074 (1.2%). An absolute accumulator
record for SZ000858 first writes a negative Rust internal `0x14` value before
public projection. Corrected frame 258/record 25 audit selects volume token
`M(raw=0x20f3f858)` at value-stream bits 1490..1525 and reproduces
`-520882088` exactly. The ordinary absolute chain ends at Rust's recorded
`prefix_values_end=1553`, with no unexplained tail. The v1 diagnostic used
payload-relative reads instead of skipping the six-byte envelope, read two
OHLC tokens instead of one, and selected the relative amount table. Its
`+22129`, 12-bit gap, and claimed native boundary agreement are withdrawn.
Native `0x448cb0` sign extension agrees with Rust for this M input; the missing
evidence is Wine's same-record reader position, token input and resulting state.

Native x86 function emulation now confirms record 25's last, volume, amount,
amount24 and offset2c, ending at 1553 with the same negative values. This uses
Rust-selected input metadata/boundary, not native live-session state. Starting
frame 258 from bit zero with Rust pre-frame states instead finds the first
divergence at record 0: scalar prefix agrees through 43, then native move -1
skips layout and reads `0x5b291c` at 44..55. Native record end is 108 versus
Rust 69. A source fix is pending verification: correct move continuation,
read the baseline volume mask independently, and decode volumes even when
layout is skipped. WebClx request `220343-18d2e6663e8874f4` passed 198 shared
library tests (one ignored) and built the release extractor. Three-fixture
replay is queued as `220649-18d2e6663e8874f5`; the complete consumer matrix is
queued as `220724-18d2e6663e8874f6`. Independent outputs are under
`quoteNetzipRs/diagnostics/20260907-coupled-ladder-v1/{replay,matrix}`.
Replay request completed with source unchanged: full-init 26,722 frames,
24,214 clean / 2,508 error (previously 3,946 errors), 402,137 completed
records, 396 omitted frames / 953 indexes; close-window 101,765 frames,
95,814 clean / 5,951 error (previously 19,616 errors), 1,480,242 completed
records, 474 omitted frames / 1,224 indexes; test-168 clone 184/184 clean,
3,664 completed records and no omissions. All are strict with zero EOF records;
extractor does not evaluate runtime rejections or prove business parity.
Updated conditional native frame 258 replay passes records 0..11; record 12
has matching bit boundaries (896..925) but amount native 3,648 vs Rust 15,808.
Evidence: `diagnostics/20260907-coupled-ladder-v1/frame258-native-prefix.json`.
Investigate amount prediction and supplied baseline provenance next; this is
still conditional emulation, not live-session Wine parity. Consumer matrix
remains running and has recorded fullpull fmt exit 1; preserve the source
until it completes, then fix formatting and rerun affected gates.
Follow-up native input tracing isolates record 12 (SZ000800): baseline volume
6, current volume 26, baseline amount 3,648, current last 0, reference price
608. At bit 920 the native amount token receives previous=0; Rust's reference
price fallback adds (26-6)*608=12,160, exactly explaining 15,808 vs 3,648.
Native `0x44a0ba` reads current last directly. Evidence is
`diagnostics/20260907-coupled-ladder-v1/frame258-native-amount-inputs.json`.
The native `0x44a119..0x44a15f` mode 0/8 adjustment also follows token decode,
whereas Rust applies it before decode; this second discrepancy still needs
a focused nonzero-token native regression. Shared source remains unchanged
while the consumer matrix awaits its callback. Next revision must remove the
reference-price fallback and verify adjustment ordering with native fixtures,
then rerun shared tests, all three replays and the complete consumer matrix.
This is not a verified release. Do not run the
old trace-only script, which deletes historical evidence and asserts old
strict counters. The new runner preserves history and records all failures.

## Open gates

### Timestamp subtraction revision (2026-09-08)

Independent scalar sampling `010039` passed 12 tests, Clippy/release and
preserved all pre-existing parity report fields and input/source hashes.
SH600419/index23990 is the first unique nonzero-callback candidate:
prefix1418/frame178/ordinal2, bits152..205, mask1; Rust price/volume/amount
are zero versus callback8.73/188/164124 (sequence9289). Uniqueness of the
callback candidate does not establish equal state provenance.
Conditional audit `010901` report:
`../../../quoteNetzipRs/diagnostics/20260908-timestamp-subtract-v1/auction/scalar-baseline-chain-v1.json`,
SHA256 `0b5a27db526da2809850c9f68a46a5d1dde61ed1bc7e82af7d59a6a6508b1fcd`.
Inputs are unchanged; native_session_parity remains false. First occurrence
0209/ordinal7 is missing_fresh with no committed state; native audit rejects
the missing explicit baseline. Later0739/ordinal6 and1418/ordinal2 match full
311B under the supplied Rust prior states and starting bits; trailing ends
are601 and205 (distinct from native record-function end). All three special
paths consume no scalar tokens. This does not support a local scalar-token
fix or prove that the diagnostic initial state equals Wine's live baseline.
Terminal quoteNetzipRs_26 next classifies initial-state provenance for other
unique candidates. Do not generalize this single chain to all parity failures.

Business-second follow-up queued as `005655-18d31111735c6f1c`: rebuild
extractor/parity examples and replay both auction seed controls, with outputs
under `../../../quoteNetzipRs/diagnostics/20260908-timestamp-subtract-v1/auction`.
Callback completed successfully with source/input hashes unchanged. Both seeds
retain 2,403 frames, 1,996 clean / 407 errors, 30,367 completed records and
60 omitted frames / 114 indexes. Same-day matched 27,712; joint scalar/time
12,107; nonzero callbacks 13,084 with joint hits 3,869. These aggregate counts
match the preceding flow-isolated report; no measured parity improvement is
established by this auction sample. Terminal quoteNetzipRs_26 owns bounded
nonzero scalar mismatch sampling on this new extract, with flow/ordinal/bit
provenance and ambiguity accounting. Existing inputs need not be re-extracted
for an observation-only parity tool change.
Recompute candidate populations and nonzero joint parity before interpreting
changes against historical reports. This request is offline only.
The user permits deployment/restart when needed for verification, through
webClx with rollback and runtime checks. This does not authorize canonical
publication or remove fail-closed/business-parity gates.

Native descriptor mode 2 (`0x44ac18`) selects subtraction at
`0x4499f6..0x449a1a`; `0x448b90` returns D/M tokens directly and otherwise
computes previous minus token. The shared 2704 timestamp call now passes
`subtract=true` instead of false. Table/mask and all other branches are
unchanged. Evidence: consumer-relative
`../../../quoteNetzipRs/diagnostics/20260907-shadow-deploy/vendor-timestamp-sign-v7.json`
and the local disassembly contexts in `20260907-disasm-evidence`.
WebClx `000941-18d31111735c6f19` passed six timestamp-focused tests and
all-target Clippy with warnings denied. Tests cover nonzero B, zero E,
absolute D, underflow, mask-class early return and a complete special record
with unchanged non-time bytes and expected bit consumption.
Source SHA256: `6e3d0f03f2af7f65f86844743e20eed00c401ba1f07c57b7099b4c64cf838785`.
Three-fixture replay and the full consumer matrix completed as
`001045-18d31111735c6f1a`, with `source_unchanged=true` and
`inputs_unchanged=true`. Full-init remained 26,722 frames (24,214 clean,
2,508 errors, 23 timestamp-delta stage errors); close-window remained 101,765
frames (95,814 clean, 5,951 errors, 20 timestamp-delta stage errors); the
168-clone remained 184/184 clean with 3,664 records. All 20 consumer checks,
including tests, strict Clippy, release builds, and the Windows target passed.
These counters are unchanged, so the timestamp fix is a semantic correction
whose business-second parity must be recomputed; it did not repair unrelated
ladder/accumulator errors. Passing focused tests and the matrix does not
establish OEM parity.
Strict remains default, publication disabled, no deployment or restart.

Revision follow-up: removed the metadata reference-price fallback from amount
prediction and added a real-token regression for frame 258 record 12
(`c9 20`, value-stream bits 912..921). Native expected volume delta 20,
volume 26, amount24 -26, amount 3,648. WebClx request
`224521-18d31111735c6f06` completed successfully: formatting, 199 shared
library tests passed / one ignored, and release extractor built. Mode 0/8
adjustment ordering remains unchanged and open. Three-fixture replay is queued
as `224849-18d31111735c6f07`, and the complete consumer matrix as
`224850-18d31111735c6f08`. Independent reports are under
`quoteNetzipRs/diagnostics/20260907-zero-last-v1/{replay,matrix}`.
Source SHA256 for this revision:
`50078a77b5529c673c379583583736a0d87eb46f8abe90000be7bcc26379a173`.
Both callbacks completed with source unchanged. Replay counts remain unchanged
from the coupled-ladder revision: full-init 24,214 clean / 2,508 error;
close-window 95,814 clean / 5,951 error; test-168 clone 184 clean / zero error.
Omissions and completed-record counts are unchanged; all runs are strict with
zero EOF records. The zero-last fix changes field values, not these counters.
The complete consumer matrix passed all 20 checks, including locked tests,
Clippy, release builds and the Windows GNU target. This supersedes the earlier
matrix failures below for this revision, not the open runtime/product gates.
Conditional native frame 258 replay now matches all 31 records on OHLC,
volume, amount, amount24, offset2c, mask and bit spans, including record 12.
Report: `diagnostics/20260907-zero-last-v1/frame258-native-prefix.json`.
This uses Rust pre-frame states and native within-frame propagation; it does
not compare every byte of an independently captured Wine-session 311B state.
Follow-up conditional native experiment preserved historical frame258 record25
wire bytes and varied only mode/adjustment metadata. Amount token bits1535..1553
subtract 34,751 from the unadjusted prediction. Mode8 adjustment2 produces
-11,354,187,858,474; mode0 adjustment2 produces -104,250. These equal
`(decoded_token + 1) * 3`, proving post-token order for these inputs.
Shared code now applies the mode0/8 adjustment after token decoding and before
baseline addition, using native wrapping 64-bit arithmetic. Added real-token
mode controls and a separate baseline-addition-order regression; Python native
regression retains the historical boundary explicitly, not as a current quote.
WebClx `231215-18d31111735c6f09` completed successfully: formatting, six native
regressions, 201 shared library tests passed / one ignored, release extractor.
Revision source SHA256:
`4ee3d3702269c9440f29ece74435caf86b2335b1d9b63db0894809b3d86aa4dd`.
Three-fixture replay `231324-18d31111735c6f0a` completed with unchanged source:
all frame/error-stage/omission/completed/EOF counts match the zero-last revision.
Complete 20-check consumer matrix `231325-18d31111735c6f0b` passed every check
with unchanged source, including Windows target. Independent outputs:
`quoteNetzipRs/diagnostics/20260907-post-token-adjustment-v1/{replay,matrix}`.
Preserve decoder source until both callbacks arrive. Check source hashes,
frame/error/omitted/completed/EOF ledgers, then compare native fields from the
new replay. This newer source revision has not yet passed those full gates;
prior all-pass matrix applies only to the zero-last revision above.
Fresh conditional frame258 replay still matches all 31 records on the eight
reported fields, masks and bit spans. A separate full-byte comparison now finds
the first uncovered difference at record30: native/Rust i32 at 0xb0 is
-119/7,763; at 0xb8 it is 87/192 (differing bytes b0..b3 and b8). Records0..29
match all 311 supplied/output bytes. Metadata is supplied from Rust, so neither
comparison establishes independent Wine-session state parity.
Next investigate `merge_value_volumes` and native ladder-volume merge with
record30's baseline and equal-price slot selection. Do not assume bit
consumption is wrong: record30 boundaries and the other reported fields match.
Native `0x44a7f0` entry tracing confirms record30 merge inputs: delta266,
flags0, current prices -3..6, baseline prices -4..5. At current slot2 (-1),
the equal baseline slot3 is rejected because current slot3 has zero price:
Rust incorrectly added 7,882. At slot4 (1), baseline slot5 is on the other
side and must not merge: Rust incorrectly added 105. Native searches two
five-slot sides, stops at the first old price >= current, and requires the
current price at the matched old slot to be nonzero. Flag bit2 selects
positional addition; its path also reaches final trade-delta subtraction.
Shared `merge_value_volumes` now follows these controls. Added real record30
inputs plus flags2/3 delta regressions in Rust and conditional native tests.
Native replay now reports full-record equality/differing byte offsets and
stops on any 311B discrepancy, not only the eight selected fields.
WebClx `232240-18d31111735c6f0c` completed successfully: formatting, eight
native tests passed, 203 shared library tests passed / one ignored, extractor
built. Merge revision source SHA256:
`54b72db636b60e0014a6fe64da3ed2d605661200a492b7368fc1baf673f925dd`.
Three-fixture replay `232350-18d31111735c6f0d` completed with source unchanged:
all frame/error-stage/omitted/completed/EOF counters match the previous revision.
Complete matrix passed all20 checks with source unchanged as
as `232350-18d31111735c6f0e`, outputs under
`quoteNetzipRs/diagnostics/20260907-sided-volume-merge-v1/{replay,matrix}`.
Fresh conditional native comparison passes all 31 frame258 records and all
40 frame259 records on full 311B output, mask and bit boundaries. Reports:
`frame258-native-full-record.json` and `frame259-native-full-record.json`
under the new diagnostic root. Inputs still use Rust pre-frame state and
metadata; this is not independent live Wine-session parity.
Frame260 audit did not complete: ordinal27, bit2056, mask0xc1, marketSZ,
symbol_index86, timestamp1788763521 has no preceding state available to the
native audit tool. It raises `relative record needs an explicit preceding state`.
No successful frame260 report was produced. Investigate seed/baseline provenance
in the extractor versus audit before continuing; do not fabricate a baseline or
classify an unexecuted record as parity-pass. Keep source unchanged pending
consumer callback. The merge revision consumer gate is now passed; business
parity, lifecycle and publication gates remain open.
Frame260 trace shows ordinal27 is explicitly `missing_fresh`, stored metadata
present but committed state absent. Audit now has an explicit optional
`--allow-missing-baseline` (default remains rejection) to test a null pointer
without manufacturing zero-state memory. Native execution then fails at
`0x44990b` (`rep movs` copying the baseline), with index+9 relative flag set.
This proves the supplied flag/null combination is invalid, not that real Wine
uses it or crashes. Next inspect caller `0x44aa30` flag/record lookup setup;
do not promote Rust missing_fresh behavior as native-equivalent on this evidence.
Python regression request `233138-18d31111735c6f0f` completed: nine tests
passed. Shared decoder source is unchanged, so this is not a new shared revision.
Caller inspection: `0x44ac93` resolves market/index via `0x4157b0`, copies
metadata, then `0x44ace5` resolves code string plus connection category
(`input+0x2e8`) via `0x415820`. The caller leaves the loop if the latter is
null; otherwise mask bit0 selects that record pointer versus null at
`0x44acf8..0x44ad20`. Index lookup misses return an embedded fallback record
at lookup-object+0x5524e, not necessarily null. Code/category lookup rejects
category>=100 and returns null on map miss. No creation occurs in these two
lookup functions. Thus missing prior decoded records alone does not establish
the native baseline pointer. Next reconstruct the code/category map population
and same-session baseline provenance for SZ index86; do not seed a guessed
zero record or claim the direct null-pointer test represents the native caller.
Evidence: `diagnostics/20260907-post-token-adjustment-v1/frame258-native-merge-inputs.json`.
Strict/default and publication-disabled are unchanged. No deployment occurred.
Previous matrix `220724-18d2e6663e8874f6` completed with source unchanged:
shared tests/Clippy/release and Windows target passed, but fullpull fmt,
quoteNetzipRs Clippy, netzip_win Clippy and all three stock-source-netzip
checks failed. The latter require a lockfile update, not a decoder fix.
No release or business-parity acceptance is implied.

1. Verify the baseline ladder control-flow fix against native execution,
   then replay full-init/close-window/168-clone and the entire consumer matrix.
   Recompute frame 258 from its beginning; record 25's old boundary is not a
   valid fixed-point target after earlier records change. Prove Wine-session
   311-byte state separately from conditional function emulation.
   `0x44aa30` `vtable+0x78` is a stub (`0x44e180`); follow the
   `this+0x247f8` map readers and accumulator calls in `0x449770` instead.
2. Prove NativeWineClamp per-symbol values against Wine, not only that
   EOF extras commit. Close-window already shows 22.6% prefix poisoning
   of later non-EOF records; keep strict as default.
3. Close same-code, exact-business-second dynamic OEM parity. Identity
   (`name`, `last_close`) is closed on the 09:04 auction window when the
   seed is the same-day `0104`; price/OHLC/volume/amount/book are not.
4. Close same-`0104`-version P1-P7 parity and per-slot coverage.
5. Validate reconnect/day-cut/freshness/silent-drop behavior across ten slots.
6. Complete each consumer's integration tests and product canary.
7. Publish no canonical quote until all earlier gates pass.

## Baseline Provenance Follow-up

Static investigation located category-state creation in the 2a10 builder
`0x49b550`, separate from metadata loading `0x415890`. Missing code entries
are allocated/reset before subscription entries are appended; existing entries
are preserved. Detailed controls are in PROTOCOL, disassembly hashes in
EVIDENCE_INDEX. Added a conditional native regression to execute the actual
311-byte reset on nonzero inputs with surrounding memory guards; verification
passed with all ten native regressions in webClx `234238-18d31111735c6f10`.
The native reset clears nonzero inputs and preserves both guard regions.
Shared decoder SHA256 remains the verified merge revision.

Next: join frame260 SZ/index86 to the same-session 0104 code and category's
2a10 entry, inspect prior writes/reset events, then supply only a provenance-
supported state to the native comparison. Subscription allocation explains
why absence of a prior Rust commit is insufficient to infer a null native
baseline; it does not establish the captured session's actual 311B contents.
Strict/default, missing-seed not-ready and publication-disabled remain intact.

Capture provenance check: frame260 SZ/index86 resolves to SZ000488 in the
Rust-supplied metadata. The prior260 frames of that flow contain no occurrence
of this key in either delta indexes or decoded records. They comprise252
2704, four0d04 and four5404 frames; the first frame is already2704.
The close-window seed-provenance.json reports baseline_core=null,
baseline_records=0 and32649 metadata seeds. The full-init replay manifest
has no matching client endpoint for this flow. Thus the current fixtures do
not establish a same-session subscription/reset-to-first-record chain for
SZ000488, and native zero allocation is not justification to invent its
mid-session baseline. Frame261 conditional audit also stops at the missing
preceding-state guard and produces no successful report. Corrected the
extractor comment that incorrectly described missing_fresh as a vendor path;
decoder behavior and the verified shared source are unchanged.

## Accounting rules

Continuation audit completed as webClx `234910-18d31111735c6f11`: conditional
native full-record comparison for close-window frames262 and263, using the
verified sided-volume-merge replay. Outputs remain under that diagnostic root
as `frame262-native-full-record.json` / `frame263-native-full-record.json`
and separate `frame262-native-audit.log` / `frame263-native-audit.log`.
Missing preceding state still rejects execution; a failed invocation may
produce only its log. Callback success alone is not parity: inspect every
record's full-byte, mask and bit-span equality and tested/target counts.
No shared decoder behavior changed and no deployment was requested.
On a demonstrated semantic discrepancy, fix the decoder promptly with a
focused regression, then rerun all three fixtures and the full consumer matrix.
Callback status1 is frame262's missing preceding-state rejection, not a Rust
compile failure. Frame263 completed with all31 records equal on full311B,
mask and bit spans. This remains conditional, not independent Wine parity.
The audit tool now retains the completed prefix and structured missing-state
coordinates, and its CLI exits1 for blocked, incomplete or mismatching frame
comparisons (previously a reported mismatch could exit0). Native regression
and frame262 expected-rejection validation are queued as
`235037-18d31111735c6f12`; decoder semantics remain unchanged.
That callback passed all11 native regressions and the expected-rejection
check. Frame262 retains48 tested records out of60; ordinal48, bit3272,
mask0xc1, SZ/index440 lacks preceding state. It remains incomplete.

Dynamic OEM follow-up queued as `235348-18d31111735c6f13`: build the current
extractor/parity examples and actually run the 09-04 auction fixture twice.
The historical-seed control keeps the old decoder metadata input; the other
run uses same-day09:09 metadata for decoding. Both project with same-day
metadata and join exact business timestamps, with strict decoding and no
invented baseline. Outputs: `diagnostics/20260907-auction-current-decoder-v1`.
Runner records source/input hashes and separate replay/parity ledgers.
This is pending evidence, not business parity or same-session seed acceptance.
Request `235348-18d31111735c6f13` stopped before Cargo at the target-directory
guard (command status2); no replay ran. The project target symlink resolves
under `/data/cargo-target/`. Retry explicitly resolves that project path
instead of relying on the nested shell's inherited CARGO_TARGET_DIR.
Retry `235431-18d31111735c6f14` completed with source and input hashes
unchanged. Both strict auction runs:2403 frames,1996 clean/407 error,
30367 completed records,60 omitted frames/114 indexes,zero EOF; runtime
rejections not evaluated. Historical-seed control matched27711 records:
price13607,volume17073,amount17503. Same-day-seed run matched27712:
price13704,volume17067,amount17544; name/last_close27712 each.
The latter retains769 invalid records,3182 ambiguous business timestamps and
4 conflicting timestamp records. Counts include zero equality and carried
state; changed match populations prevent a paired per-record improvement
claim against the old report. Dynamic parity remains open, especially book
volumes (ask544/bid560 hits). Next isolate exact-record first divergences
from the new auction output with native state provenance.
Follow-up found the parity example shared OEM state by market/index across
all flows, while this auction manifest contains14 directed TCP flows. Thus
the preceding percentages are historical global-state results, not accepted
flow-isolated parity. The example now requires src/dst provenance and isolates
OEM state by both endpoints; failed projection also preserves the prior state
instead of deleting it before attempting the merge. Focused tests/Clippy/build
and dual-seed replay queued as `235801-18d31111735c6f16`, output
`diagnostics/20260907-auction-flow-isolated-v2`. Shared decoder unchanged.
TCP endpoint isolation alone does not establish category mapping or handle
reused tuples across uncaptured connection epochs; independent native-session
parity remains required.
Flow-isolation callback `235801-18d31111735c6f16` passed focused tests,
Clippy/build and both auction runs with unchanged source/input hashes.
Same-day matched27712:price14564,volume17601,amount17751; ask/bid volume
hits536/554. Structural ledgers unchanged. Joint price/volume/amount/time
and nonzero-callback accounting added to the parity example; verification
queued `000158-18d31111735c6f18`, report planned at
`diagnostics/20260907-auction-flow-isolated-v2/trade-joint-parity-v3.json`.
This measures cumulative quote updates, not individual transaction coverage.
No reduction of the six acceptance gates has been approved.
Joint-trade callback `000158-18d31111735c6f18` completed. Matched27712,
joint price/volume/amount/timestamp12107; callback-all-zero14628,
callback-nonzero13084, joint-nonzero3869. Thus joint parity is43.69% overall
and29.57% for nonzero callbacks. These are cumulative quote updates, not
transaction-level tick completeness. Report:
`diagnostics/20260907-auction-flow-isolated-v2/trade-joint-parity-v3.json`,
SHA256 `ad164f371aa6ea1f71b7005a136faefb44912b599a0ebb49d67d19e1846712bc`.
No pending build at this checkpoint. Next investigate first nonzero scalar
divergences in the flow-isolated auction, supplying only evidenced native
baselines; preserve strict default, no-seed not-ready and publication-disabled.
User asked whether excluding book output shortens delivery; no decision yet
between latest cumulative quote stream and true per-transaction tick data.
Do not silently reduce the original six gates. User requests self-terminal
`/compact` then `继续` via webClx HTTP API after major phases, after evidence
is persisted. Current callback terminal identity is s5486 / quoteNetzipRs_26.

Native audit complete-frame accounting now requires decoded/tested counts
to equal the payload's declared count and no extractor error, in addition
to no missing-state block. Empty/partial extracted prefixes cannot qualify
as complete. Callback `000026-18d31111735c6f17` passed12 native regressions.

Keep frame status, error records, omitted frames/indexes, completed records,
EOF/clamp events, missing seeds, accepted candidates, and rejected candidates
separate. Required runtime conservation:

```text
input_records = accepted + rejected + missing_seed
```

Reject reason hits may overlap. If totals are presented as a partition, also
provide one mutually exclusive primary reason per rejected candidate.

Wall-time follow-up: the 09:25 auction wall-time join replay improved overall
joint from 12,107/27,712 to 12,985/27,712, but nonzero-callback joint stayed
3,869; the ratio gain was denominator-driven. The timestamp-corrected
close-window negative control produced identical metrics under both policies,
so wall-time ranking is not a universal default. State classification of 20
unique wall-time candidates found 14 decoder-record zero scalar sets with
nonzero Wine callbacks, 12 of them without any nonzero Rust predecessor; this
points to state provenance/category mapping, not candidate ambiguity or record
arithmetic. SH513130 conditional native audit matched Rust's full 311B output,
including last620/volume12403039/amount690664965 versus a materially different
callback, again falsifying local token arithmetic. Static callback-path
resolution now identifies the actual chain:
frame handler `0x496850` -> category dispatch `0x496740` -> `0x4998c0` ->
`vtable+0x74` (`0x5c591c+0x74 = 0x44e140`, a null stub) ->
`0x5c59a0+0x74 = 0x423180`, whose owner method routes to
`0x496270 -> 0x496490 -> global+0x1e03b94 -> 0x443ca0 -> 0x46b5e0`. Thus the
earlier `vtable+0x78 = 0x44e180` rejection did not falsify the projection
path; it used the wrong vtable object. Next reconstruct the `0x46b5e0`
dispatch and 0x3a0 ring-entry quote-field offsets with the SH513130/SH113640
values. Decoder, strict default, no-seed not-ready and publication-disabled
remain unchanged.

Dispatch object resolved: the callback projection receiver is
`global 0x5eaf98 + 0x1e03b94`. Direct callers include the category path at
`0x496490` and projection dispatch at `0x474700`. In `0x474700`, hash lookup
resolves the symbol record at `owner+0x2ac0`, allocates/attaches request
state, then calls at `0x4748d2`:

```text
0x46b5e0(this, key, state, request, entry_ptr, 2)
```

where `this = owner+0x1e03b94`, `state` is the resolved record handle, and
`entry_ptr` is the 0x3a0-byte ring entry. `0x46b5e0` builds a 0x1c-byte
request, resolves/clones a data node via `0x46b6d0`, invokes
`[this_vtable+0x1c](request, node, entry)`, and returns nonzero on success;
`0x474700` then registers ownership under `owner+0x1aac` and clears
`entry+0x860`. Next evidence target is the concrete runtime vtable of
`owner+0x1e03b94` and its `+0x1c` implementation, then mapping ring-entry
fields into Wine callback price/volume/amount. Static-only; no runtime,
decoder, default, or publication changes.

Runtime vtable resolved: constructor `0x468610` installs vtable `0x5c5af0`
into the projection object, allocates the 100-entry `0x3a0` ring at
`this+0x2ba0`, and reaches the same receiver as `owner+0x1e03b94`.
`0x5c5af0+0x1c = 0x469420`; thus `0x46b5e0` calls
`0x469420(request, node, ring_entry)`. The callback scalar reconstruction
target is now unambiguously `0x469420` and its callees. Strict/default and
publication-disabled remain unchanged.

`0x469420/0x469460` are preparation only. Scalar packing candidates are
narrowed to three output builders used before common publication through
`0x496490`: `0x4a67a0`, `0x4a7540`, and `0x4a6dc0`. Next disassemble those
three and map ring-entry offsets to Wine quote price/volume/amount.

`0x4a67a0` now reconstructs Wine callback price/OHLC from separate per-symbol
subrecords: price `+0x29e` (fallback `+0x11e`) divided by float32 `+0x98`,
open `+0x296/+0x136`, high `+0x29a/+0x13a`, low `+0x9d`, book count
`+0xe5+1`, and 12-byte levels at `+0xe6`. Thus callback scalars are not direct
311B decoder-record reads; SH513130 divergence is structurally expected.
Next validate these offsets against retained callbacks, then reconstruct
volume/amount builders `0x4a7540/0x4a6dc0`. No default/runtime/decoder change.

Volume/amount builders are reconstructed. `0x4a7540` selects subrecords by
integer `+0x1f2`, then `0x4a7890` projects volume `+0x1f2`,
`+0x122/+0x126/+0x12a`, `+0x206`, and level arrays
`+0x232/+0x272/+0x212/+0x252`, with scaled floats `+0x292/+0x1f6/+0x1fa/
0x1fe` divided by float32 `+0x98`. `0x4a6dc0` resolves code/market records
from `global 0x5eaf60` and projects the separate transaction/level stream.
Thus all three Wine callback scalar groups come from independent subrecord
fields, not the 311B decoder record. Next implement offset-level validation
against retained SH113640/SH513130 callbacks. Strict/default and
publication-disabled remain unchanged.

Fixture audit: existing `mem-t*.records.json` are 311-byte decoder-side
committed records, not callback projection subrecords; reconstructed offsets
`+0x1f2` and later do not exist in those buffers. Numeric validation of the
callback field map therefore requires a new read-only same-session memory
capture of `global 0x5eaf70` and `0x5eaf60` paired with retained callbacks.
No decoder, default, or publication change.

### OEM mapping evidence qualification

311B-to-256B bridge located at0x49ad20: input stride0x137, output stride0x100,
then call0x4a4660 at0x49b083. It rejects zero time/reference and records with
zero last and both central book prices zero, resolves input+0xe3 via0x440110,
copies input OHLC/time into distinct output offsets, stores input volume's
low dword at output+0x20, and calls0x5847a0 for amount into output+0xe0.
Resolved category9 additionally adjusts source quantities by division100.
This establishes a static conversion path, not proof of the current live
dispatch: no direct call to0x49ad20 was found in this objdump scan; investigate
indirect/vtable references. Do not implement low-word truncation or category
scaling in public projection without matching object identity and branch tests.
Raw bridge and conversion helper: consumer diagnostics/20260908-oem-311-256-bridge-v1.
Six pinned raw-evidence tests pass, including stride/call-site assertions.

Confirmed conditional copy path: 0x4a4660 traverses inputs at stride0x100,
calls0x40aeb0 at0x4a477a, then0x4a1cb0 at0x4a4790. On its accepting path,
0x4a1cb0 computes destination=arg1+0x1e6 at0x4a1d28 and copies arg3 there
with ecx=0x40 dwords at0x4a232e..0x4a2339 (256 bytes). Thus the stored
subrecord has an explicit upstream copy source; independent storage is not
evidence of independent data origin. Guards and live entry remain relevant.
The input is not yet proven to be derived from 311B; next locate the producer
of these 256-byte records and callers of0x4a4660. Do not transplant 311B
offsets into this layout. Raw contexts: diagnostics/20260908-oem-copy-path-v1
in the consumer, indexed in EVIDENCE_INDEX.

Amount getter address translation confirmed: 0x40bb70 supplies this=source+0x1e6,
so 0x40acc0's relative +0x24/+0x20/+0x18/+0xc4 correspond to source
+0x20a/+0x206/+0x1fe/+0x2aa. These are the existing supplement formula's
current-volume, volume, close and mode fields, not a new direct raw-float
interpretation. Adjacent0x40aeb0 writes mode bits and compressed +0x24 using
floating +0xe0 and volume +0x20; classify as a compression-writer candidate
until caller/object provenance is verified. Its input argument is not yet
identified as a 311B row. Raw getter/helper/writer ranges are indexed; next
trace calls to0x40aeb0 and the source+0x2c6 (+0x1e6+0xe0) writer.

Pinned builder audit also resolves the eligibility predicate: at0x4a76bd
getter0x40b5b0 is followed by test-eax and a zero-result skip; qualifying
records call0x4a7890 and advance output by0x1f4. It is a nonzero-time filter,
not a volume threshold. Getter0x40bb70 passes source+0x1e6 to0x40acc0;
the amount is decoded through that helper, not merely read as a raw float.
Existing disk exporter logic is the first reuse target. Five Python raw-evidence
tests passed (`scripts/test_oem_builder_evidence.py` in the consumer), covering
artifact hashes, scalar stores, volume getter, bid-volume base and byte flags.
These tests verify pinned static evidence only, not historical live callbacks.

New pinned raw disassembly is indexed in EVIDENCE_INDEX under OEM builder
raw instructions. Direct corrections: `0x4a7a80` writes buy volumes to
output+0x128+4*i, not base0x130. `0x4a6bfb` writes one BYTE to output+0xab,
not a float low; adjacent flag stores are also bytes. Withdraw the earlier
low/float identification and its inferred overlap. `0x4a791d` writes the
0x40b5b0 result to output+0x58; `0x4a79c1` writes converted volume to +0x7c;
`0x4a79d1` stores getter0x40bb70's floating result to +0x80. These agree with
the historical exporter/header mapping but do not supply live state binding.

Historical reuse lead: consumer document
`../../../quoteNetzipRs/docs/forensics/wine-realtime-dat-rust-validation-20260901.txt`
names the same executable SHA `de712a8dde6d990e1c586f8afd4194575e35dffa2d0f81245fe29f6f8509bd29`
and exporter `0x4a7890`. It identifies source+0x1f2 as time, +0x206 as
volume, +0x1e6 as compressed amount. Existing `netzip-supplement/src/lib.rs`
uses these offsets and retains representative slot tests. This contradicts
the newer narrative's name "volume" for source+0x1f2 and is a concrete reuse
lead, not new verification of the historical binaries or September4 session.
Reuse the disk exporter tests and conversion rules before proposing a new
projection algorithm. Still establish the live writer/copy path, actual output
type, producer version and captured-state/callback binding independently.

The preceding builder descriptions are reported static mappings, not a closed
explanation of scalar mismatches. This review has not verified corresponding
raw builder assembly hashes, independently captured SH113640/SH513130
subrecord bytes, output type, or same-session callback binding. Different
storage addresses do not exclude copying from the 311B decoder state. Trace
the writer/copy path and selector/divisor provenance before promoting these
claims. Callback values alone cannot validate internal offsets; synthetic
object tests establish arithmetic only. OEM mapping ownership is not yet
confirmed by this terminal; coordinate before duplicate disassembly or edits.
The zero-last and post-token fixes are already validated history, not new
candidates. Request070448 reuses extract data; request070829 invokes an existing
extractor. Source stability alone does not prove that binary's build origin.

Offset validation on the cold-start fixture (5,510 volume-gated callback
pairs) proved price/open/high/low/last_close callback values equal the 311B
committed-record integers under per-symbol decimal scale (4,657 scale100 and
853 scale1000; zero ambiguous). Amount does not match the committed `+0x1c`
value beyond float32 skew for most pairs and still requires the separate
transaction table (`0x5eaf60`). Next rework the parity projection to use the
same committed-record integer/scaling domain, then re-run auction/close
comparisons; decoder remains unchanged.
