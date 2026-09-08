# Evidence index

## Relative baseline tail precedence

`../../../quoteNetzipRs/diagnostics/20260908-relative-tail-v1/native-relative-tail-full-record.json`
SHA256 `82d49f268c845f3370412d192144c53028f45258915d9363e167882dfbfccef6`
records two conditional311B controls: metadata byte+100=0, baseline markers
90/165 retained in native output. This is synthetic-state function evidence.
The report's pending Rust comparison field predates owner-reported105419
golden integration+Clippy pass; it is not historical session parity.

`../../../quoteNetzipRs/diagnostics/20260908-relative-tail-v1/decoded-byte-comparison.json`
SHA256 `ee68a67d28a4acf1c70a65a5154dc5a43ce48bf89535ead59ca25e8fe76fe4f1`
reports128402 identical files/1886043 records across three fixtures; source
6e3d0f03 versus2ba2d9f4. Management read and hashed both reports, not reran the
whole comparison. Later test-format revisiona99b072b is not the replay source.
Consumer retry110115 subsequently passed20/20 checks; report
`../../../quoteNetzipRs/diagnostics/20260908-relative-tail-v2-format/matrix/verification.json`
SHA256 `40ca0bc822591255cfcf0bcf5c5e39273f2b7fccfa0ec30aaccf084e3312297c`.
Management verified the report sourcea99b072b matches current decoder source.

## Direct-mode bridge build verified

Request110111-18d31111735c6f54 passed fmt,7 Rust tests,Clippy,release and50
conditional native/Rust scalar cases. Consumer report
`../../../quoteNetzipRs/diagnostics/20260908-oem-bridge-scalar-v1/native-rust-matrix-direct-v2.json`
SHA256 `858be92f111470b1d536f8c289ac3abcb650b875cf800fb4f25077676749a03b`.
Source and binary hashes were checked against the matching build log.
Separate native final-copy controls verify all256 bytes overwrite old state,
including zeros, with prior gates assumed; not captured-session business parity.

## Schedule and group-state gate contexts

`../../../quoteNetzipRs/diagnostics/20260908-oem-time-gates-v1/manifest.json`
SHA256 `6362207329fd7e6fa4f28cc5469c546c09bc26da2c655bd892ed4b69761acbc5`
pins four raw contexts including40c160 and4a2700. Consumer native test
`test_schedule_interval_clamping` passes11 synthetic ordinary/midnight
interval cases after calendar/category handling. This does not prove real
symbol schedule provenance or full copy acceptance. Details and remaining
scope are in the consumer OEM bridge scalar implementation report.
Additional native `test_preopen_interval_predicate` passes six interval
controls; caller clears an incremental-processing flag, not the final copy.
Real schedule and calendar metadata binding remains missing evidence.

## Time transition dispatch controls

Consumer `scripts/test_oem_bridge_scalars.py::test_time_transition_fast_path_bounds`
passed seven native endpoint controls at4a2753..4a278a. Fast-path delta range
is [-200,199] for tested timestamps; the slow branch is not rejection evidence.
See `../../../quoteNetzipRs/docs/forensics/oem-bridge-scalar-implementation-20260908.md`.
No captured-session, day-boundary or u32-wrap equivalence is claimed.

## Copy rejection side effects

Consumer native test `test_copy_volume_delta_gate_and_side_effect` passes six
controls at4a1dbb..4a1e28. Signed wrapping32 volume difference governs this gate;
symbol+1b7 amount difference is written before rejection, while old256B state
is not copied yet. Conditions and scope are in
`../../../quoteNetzipRs/docs/forensics/oem-bridge-scalar-implementation-20260908.md`.
Do not model every rejected update as having no side effects, or equate passing
this local gate to full-function acceptance.

## Native bridge entry predicate

Consumer `scripts/test_oem_bridge_scalars.py::test_bridge_entry_eligibility`
passes seven native branch cases at49ada7..49adee using the pinned executable.
Requires nonzero timestamp/reference and at least one of last/best bid/best ask
nonzero. Includes negative-price control to distinguish conversion eligibility
from business validity. Stops before symbol lookup, so lookup/copy acceptance
is not proved. Scope and instructions are recorded in the consumer report
`../../../quoteNetzipRs/docs/forensics/oem-bridge-scalar-implementation-20260908.md`.

## Built Rust OEM scalar comparison

`../../../quoteNetzipRs/diagnostics/20260908-oem-bridge-scalar-v1/native-rust-matrix.json`
records32 mode2 scale cases and6 mode1/category9/overflow cases, all equal on
packed integers and OEM float32 bits. Includes binary/source/verifier/native
test hashes; no live-session parity. Request103240 completed6 Rust tests,
Clippy/release and32 native comparisons. Subsequent38-case read-only execution
passed with binary/source stable; hashes alone are not build provenance.
Binary SHA182b8c05c32607a90da52f76de799d36d378be9be854ca594c8215fea0d627db.

## Conditional bridge scalar execution (2026-09-08)

Consumer implementation and scope report:
`../../../quoteNetzipRs/docs/forensics/oem-bridge-scalar-implementation-20260908.md`.
Three native Python tests passed (ten scalar vectors, five category9 controls
with ten book slots each). Includes executable hash pin and explicit instruction
endpoint assertions. This verifies intermediate conversion only, not final OEM
parity or captured-session category selection. Rust example validation request:
`101426-18d31111735c6f40`; Python results do not establish its outcome.

## Last-symbol hit audit (2026-09-08)

- Audit: `../../../quoteNetzipRs/diagnostics/20260908-last-symbol-hit-audit/README.md`.
  SHA256 `60a225d6a1bb022fe42d36267c32a6896fec81f40def5ac290190546bcafa57b`.
- Comparator: `../../../quoteNetzipRs/scripts/compare-official-5188-last-symbol-callback.py`.
  SHA256 `051bc04e813a3888258b48f847c6d78f2c128c093e8eb563fb395ef426c4c54e`.
- Tests: `../../../quoteNetzipRs/scripts/test_compare_official_5188_last_symbol_callback.py`.
  SHA256 `677658eaa0b38bb4a60cc6f89d5c2d34cea2bf2ca4b04776c70add1db9c0a037`.
- Independently rerun Python unittest discovery: 5 passed. Fixed false hits:
  unconditional zero amount, missing/null scalars coerced into matching zero,
  and truncated/missing book arrays accepted through prefix comparisons.
- Historical amount/book/full_record_hits from this comparator are suspended
  pending fixed-input recalculation. Independent Rust business-ts reports are
  not invalidated by this implementation-specific defect.
- No real-fixture recalculation is claimed. Even corrected hits compare only
  selected fields, not the complete 311B state, and do not establish same-session
  or exact-business-second parity. Metadata selection is not session-bound;
  numeric_pair still renders missing callback scalars as zero in ratio diagnostics.
- Recalculation ownership remains to be confirmed between terminals 26/29;
  attempted notification to quoteNetzipRs_26 returned terminal session not found.

## OEM bridge virtual dispatch (2026-09-08)

- Consumer evidence: `../../../quoteNetzipRs/diagnostics/20260908-oem-bridge-dispatch-v2/manifest.json`.
- Executable SHA256: `de712a8dde6d990e1c586f8afd4194575e35dffa2d0f81245fe29f6f8509bd29`.
- Full frame/dispatch context SHA256: `399abb5e9be66d189bfc753818bc345dc4ba23bd7cb5d48ff937a8832bc5e4d8`.
- Pointer locations: `../../../quoteNetzipRs/diagnostics/20260908-oem-311-256-bridge-v1/pointers.json`.
- Reproduce with consumer script `scripts/extract-oem-builder-evidence.py EXE NEW_OUTPUT --dispatch`.
- Verification: `python3 -m unittest discover -s scripts -p test_oem_builder_evidence.py -v` (7 passed).
- v2 supersedes v1's mid-instruction range start and adds the actual
  0x44aa30 dispatch site. Data-section asm contains raw bytes, not executable
  instructions; interpret the virtual entries as little-endian pointers.
- Scope: static constructors and conditional dispatch only; no live-session
  parity, new fixture acceptance, deployment or decoder changes.

## OEM builder raw instructions (2026-09-08)

311B-to-256B bridge:
`../../../quoteNetzipRs/diagnostics/20260908-oem-311-256-bridge-v1/`.
0x49ad20 SHA256 `b4a7cdc35093ad3b2a2a6df98b5757bc5a8cac2c5e4ccfc9308f80b268325b9e`.
Reproduce with extraction script `--bridge`; live dispatcher remains unproven.

Copy path: `../../../quoteNetzipRs/diagnostics/20260908-oem-copy-path-v1/`.
0x4a4660 context SHA256
`f551d5f8e77c823a872196e7d21c9a4807daf1907f09d7454a4aea410ef8b98e`;
0x4a1cb0 context SHA256
`68eaa267e9285f6b15ff097283fb216b1d1e58cba38c2051ceb8fba3185191ed`.
Reproduce with extraction script `--copy-path`; conditional 256B copy evidence,
not proof of the upstream 311B conversion or live session binding.

Amount follow-up: `../../../quoteNetzipRs/diagnostics/20260908-oem-amount-raw-v1/`.
Pinned getter0x40acc0 SHA256
`34ea94d41cb28ff9d8bfdfd0fd953ff764867ad205756c74f8e999498b8c1d5f`;
writer-candidate0x40aeb0 SHA256
`2aab7d8e19373a991cdd243faf8fa50452ec5f7fe328d207b8ebc07406723173`.
Reproduce with the same extraction script and `--amount`. No captured state.

Consumer-relative directory:
`../../../quoteNetzipRs/diagnostics/20260908-oem-builders-raw-v1/`.
Manifest SHA256:
`9c75f4b575f8e042c8d044133a59b21f0235bad61f3821a2d5c9e25c8f1d1930`.
Six bounded objdump ranges include four builders and scalar helper contexts;
individual hashes and pinned executable SHA are in manifest.json. Reproduce
with consumer script `scripts/extract-oem-builder-evidence.py EXE NEW_OUTPUT`.
Ranges may include adjacent functions. Static evidence only, no session parity.

Evidence remains in its owning project; this file is the shared index.

| Evidence | Location | Integrity / scope |
|---|---|---|
| Close-window pcap | [`official-5188-10m.pcap`](../../../quoteNetzipRs/captures/20260907-close-window-144507/official-5188-10m.pcap) | SHA256 `d057bc73944a38534bc0646c44ef830a798cfd3f7d42e2d87d192d326bf69924`; zero kernel drops |
| Native disassembly | [`20260907-disasm-evidence`](../../../quoteNetzipRs/diagnostics/20260907-disasm-evidence/) | Executable SHA256 in `PROTOCOL.md`; address artifacts retain individual hashes in the project forensic document |
| Consolidated native disassembly reference + full token-table dump | [`official-5188-wine-disasm-reference-20260908.md`](../../../quoteNetzipRs/docs/forensics/official-5188-wine-disasm-reference-20260908.md) and [`20260908-disasm-evidence`](../../../quoteNetzipRs/diagnostics/20260908-disasm-evidence/) | Authoritative executable SHA256 `de712a8dde6d990e1c586f8afd4194575e35dffa2d0f81245fe29f6f8509bd29`; 13-byte token entries decoded from `.data` `0x5b1d50..0x5b2a94` (primary 27 tables/141 entries plus secondary decoder tables; `official-5188-token-tables.json` SHA256 `f02b662f4d4ea1e571ec804bb79866297b6e765fc92359e5b56b5125e2be24bb`); volume/amount i64 dual-path, M-token `sign_ext[29:0]×16^[31:30]`, token-with-base `0x448c10` family documented in §7; cross-build diff limited to call displacements; mechanism evidence only, no business-parity claim |
| Full initialization | [`20260907-shadow-deploy`](../../../quoteNetzipRs/diagnostics/20260907-shadow-deploy/) | Strict and opt-in clamp replay |
| Monday 168 clone | [`20260907-sync-open`](../../../quoteNetzipRs/diagnostics/20260907-sync-open/) | Diagnostic account; 184-frame isolated lane |
| Shadow deployment audit | [`official-5188-shadow-deploy-20260907.md`](../../../quoteNetzipRs/docs/forensics/official-5188-shadow-deploy-20260907.md) | Product/runtime detail, not shared status authority |
| Windows dual capture | [`20260907-local-dual`](../../../netzip_win/diagnostics/20260907-local-dual/) | Windows client and structure evidence |
| Auction overlapping OemState `business-ts` | [`callback-parity-oem-business-ts-0909-seed-v1.json`](../../../quoteNetzipRs/diagnostics/20260904-live-rust/auction-0924/callback-parity-oem-business-ts-0909-seed-v1.json) | SHA256 `388239009b8ac0d20a422051001716efe02093528412df31394a15c4fbc58049`; same-day 09:09 `0104` seed |
| Cross-day seed control | [`callback-parity-oem-business-ts-production-v3.json`](../../../quoteNetzipRs/diagnostics/20260904-live-rust/auction-0924/callback-parity-oem-business-ts-production-v3.json) | SHA256 `12956b39ef5eb3f4d9b580e8224745e0bd0da9186a2448b0c5c32795786a3f5a`; 09-03 `verified-4068` seed |
| SZ000858 v1 replay (withdrawn) | [`sz000858-frame258-record25-token-replay-v1.json`](../../../quoteNetzipRs/diagnostics/20260907-close-window-144507/sz000858-frame258-record25-token-replay-v1.json) | Invalid stream origin, OHLC count and amount table; retained as historical evidence, not a native boundary source |
| SZ000858 corrected Rust token audit | [`sz000858-frame258-record25-token-replay-v2.json`](../../../quoteNetzipRs/diagnostics/20260907-close-window-144507/sz000858-frame258-record25-token-replay-v2.json) | SHA256 `aaead42f9315c0e9b935260069429bacc937dfbcd71f904c741ecc76864af82a`; same close-window payload, Rust-only field/boundary reproduction, not Wine parity; input/source hashes embedded |
| SZ000858 independent v3 reverification | [`sz000858-frame258-record25-v3-readonly-reverification.json`](../../../quoteNetzipRs/diagnostics/20260907-close-window-144507/sz000858-frame258-record25-v3-readonly-reverification.json) | SHA256 `80b60f063d4ad38f3cf96c024ad624075d1a451448c00734d136d5978bd9635e`; independently reproduces v2's envelope-relative M token and exact prefix boundary; still not live Wine-session parity |
| Corrected absolute audit and regression | [`audit-official-5188-absolute-record.py`](../../../quoteNetzipRs/scripts/audit-official-5188-absolute-record.py) | `test_audit_official_5188_absolute_record.py`: envelope, EOF boundary, signed b, M and real record regression; five tests pass |

Every promoted entry must include fixture/session scope, account class, time
window, producer version, and SHA256 where applicable.

## Native Conditional Replay, 2026-09-07

- Current-decoder auction dual-seed replay: request `235431-18d31111735c6f14`,
  `diagnostics/20260907-auction-current-decoder-v1/verification.json`, SHA256
  `3385f23f53812232fdb337a2b0abbb7e9e09ce021bf9b36a7fc8668ad020ebdf`.
  Includes capture/callback/metadata/source hashes and per-run accounting.
  Historical formal-account09-04 auction, external metadata, no independent
  native baseline. Successful execution does not pass business parity.

- Frame263 conditional full-record report:
  `diagnostics/20260907-sided-volume-merge-v1/frame263-native-full-record.json`,
  SHA256 `487388b7bfca59e8d7d25250c7477fc9f5e1c099dbc3a6aa20fea49f51ea2bd4`.
  Same historical formal-account close-window fixture and pinned executable;
  Rust pre-frame state, native within-frame propagation. All31 records match
  full311B, masks and bit spans. Request `234910-18d31111735c6f11` exits1
  because its separate frame262 audit rejects missing preceding state.
  Central log: `/home/bin/webclx/compile/runs/20260907T234915-3119698-29040/build-1.log`.

- Baseline provenance static artifacts under
  `quoteNetzipRs/diagnostics/20260907-disasm-evidence/`, from the pinned
  executable above, no live session or account opened:
  `0x415890-metadata-map.asm` SHA256
  `f86bbb48889d4f0d27173592726efbf854510b19f960db690d13e6adb199f2b9`;
  `0x49b550-subscription-map.asm` SHA256
  `c04755d639c91f0f0ee3c05560d44d4900fba3490965aec4cc197554d2e2c018`;
  `0x49c1c0-subscription-allocation.asm` SHA256
  `47162965933afd6fa9cbe1a20f2360b7bf22f4a9f2f6c635ea91aea077c2c23a`;
  `0x4183a0-record-reset.asm` SHA256
  `1906030681294689839575ee854ff8eec16a97aaadb3b834bb2c66b9aca35757`.
  Reproduce with `objdump -d -Mintel --start-address=START --stop-address=END`
  using ranges 415890..415a63, 49b550..49b8ac, 49c1c0..49c4b1,
  4183a0..4183df (hex). Reset execution regression passed with all ten tests
  in callback `234238-18d31111735c6f10`; log:
  `/home/bin/webclx/compile/runs/20260907T234244-3047820-14537/build-1.log`.

- Sided-volume-merge revision replay, callback `232350-18d31111735c6f0d`:
  `diagnostics/20260907-sided-volume-merge-v1/replay/verification.json`, SHA256
  `660714b37e5889b4a73e56d0dda4db52c7d1cc1c4fd1b03986bf145731445f2a`.
  Source unchanged; all ledger/error-stage counts unchanged. Same historical
  formal-account close-window stream, conditional native executable scope:
  `diagnostics/20260907-sided-volume-merge-v1/frame258-native-full-record.json`,
  SHA256 `6674a8a9bf21ceb6b61bc52afe1547768a619db6085122f1c9ccea0e05f5667a`,
  all31 records full-byte/mask/bit-span equal;
  `diagnostics/20260907-sided-volume-merge-v1/frame259-native-full-record.json`,
  SHA256 `13df9955170e3b89ee63b1a44d041d4d136884645e2c6d7e6fbe28b8f1dc3e4a`,
  all40 records equal. Metadata and pre-frame state are Rust-supplied.
  Frame260 audit aborts at ordinal27 due to missing preceding state; no report
  or parity claim for that frame. Exact diagnostic coordinates are in STATUS.
  Consumer matrix callback `232350-18d31111735c6f0e` passed all20 checks;
  `diagnostics/20260907-sided-volume-merge-v1/matrix/verification.json`, SHA256
  `ccbe3e50cde9b646c5426926c7193ed9bdec1d9605b453daba5df892c1617e71`.
  Explicit null-baseline experiment on frame260 ordinal27 fails at native
  `0x44990b`, not a live-session result. Reproduce with that payload,
  `--ordinal 27 --allow-missing-baseline`; default audit still rejects missing
  state before execution. Regression captures the exact fault in
  `test_frame260_explicit_null_baseline_is_not_native_parity`.
- Record30 full-byte/merge-input trace:
  `diagnostics/20260907-post-token-adjustment-v1/frame258-native-merge-inputs.json`,
  SHA256 `70cb3ac030afa3bb35c469b0a93dad0a75b80bde70b8c49d301b07fd7690368a`.
  Same pinned executable and historical formal-account close-window fixture;
  Rust pre-frame state, native within-frame state. First complete-byte mismatch
  is record30. Native entry `0x44a7f0` records exact premerge current/baseline
  311B bytes, delta266 and flags0. This exposes same-side matching and a
  current-record zero-price guard absent in the previous Rust implementation.
  Post-token revision matrix `diagnostics/20260907-post-token-adjustment-v1/matrix/verification.json`,
  SHA256 `f3cf0cb62131795042f27dfa0f349036571e6d752a21e4157aa509e118a1b75a`:
  all 20 checks passed, unchanged source; does not verify the newer merge fix.
- Post-token adjustment replay callback `231324-18d31111735c6f0a`:
  `diagnostics/20260907-post-token-adjustment-v1/replay/verification.json`, SHA256
  `a6a6c04ed3b210965e8e0486e1cf697a709dcdb9c57088f87012fadb1f18729f`.
  Source unchanged; ledger counts and error stages unchanged from zero-last.
  `diagnostics/20260907-post-token-adjustment-v1/frame258-native-prefix.json`,
  SHA256 `44f7dac2f0d27233f9ba0ccf07b0fd5251d8c9b0d19cf2e7e72a84e22f458916`.
  Same historical formal-account payload and conditional native scope below.
  Compare `records[].native_record_hex` against corresponding decoded-values
  `record` bytes: ordinals0..29 equal; ordinal30 differs at b0..b3 and b8.
  Eight-field report equality does not include those ladder-volume slots.
- Zero-last revision, source SHA256
  `50078a77b5529c673c379583583736a0d87eb46f8abe90000be7bcc26379a173`:
  `diagnostics/20260907-zero-last-v1/frame258-native-prefix.json`, SHA256
  `44f7dac2f0d27233f9ba0ccf07b0fd5251d8c9b0d19cf2e7e72a84e22f458916`.
  Same formal-account historical close-window payload and pinned executable;
  all 31 records match the eight reported fields, masks and bit spans.
  Conditional Rust pre-frame state only, not independent Wine-session or
  full-311B parity. Replay report `diagnostics/20260907-zero-last-v1/replay/verification.json`,
  SHA256 `5277d881decaffee07a94ea08c9b78894409e8fe57100c70adc5ed0dc217c0ef`,
  completed callback `224849-18d31111735c6f07`; matrix report
  `diagnostics/20260907-zero-last-v1/matrix/verification.json`, SHA256
  `4b90dff0e7164a5873f6037a06eb682b57cd2b1831e74307c0edc3544ae7ff6a`,
  completed callback `224850-18d31111735c6f08`, all 20 checks passed.
  Both reports confirm unchanged shared source. Replay error/omission counts
  did not change; test-168 remains structural evidence only.
- Amount-input follow-up:
  `diagnostics/20260907-coupled-ladder-v1/frame258-native-amount-inputs.json`,
  SHA256 `76c128b19dd295169a10ad9efa41b93a4dd6e514ca3ca6b872996c98c89cfb47`.
  Same formal-account historical close-window fixture and pinned executable;
  conditional emulation only. Records 0..11 match; record 12's native amount
  token previous is zero despite reference price 608. Baseline amount 3,648
  and volume delta 20 explain Rust's extra 12,160 from reference-price fallback.
  Input, baseline and manifest hashes are embedded; no live Wine parity claim.
- Tool: `quoteNetzipRs/scripts/replay-official-5188-native-record.py`, using
  Unicorn 2.1.4 and pefile 2024.8.26; executable SHA256 is checked before any
  emulation. No process attach, network activity, patching, or native function
  replacement is used. Execution has instruction and time limits.
- Record 25: `diagnostics/20260907-close-window-144507/`
  `sz000858-frame258-record25-native-emulation-v1.json`, SHA256
  `051c7cfc9bf404ddcdecbc601e256640828498adb0ab73b18e1748148f13add4`.
- Frame prefix: same directory, `frame258-native-prefix-v1.json`, SHA256
  `64b0be6bd49096a2ea4968af1a5420021b83863dd533615f33280318216ac0a5`.
  Manifest, target row and preceding Rust-state file hashes are embedded.
- Scope: existing formal-account close-window fixture; emulation opens no
  account session. Initial record metadata/state comes from Rust extracts,
  not a Wine memory dump. Stop at the first differing record, not after
  treating incorrect subsequent symbol/boundary associations as truth.
- Verification: five tests in `test_replay_official_5188_native_record.py`
  passed locally with the private fixture. The initial queue submission
  returned API unavailable before a request ID; local Python fallback was
  announced. Earlier diagnostic tests passed webClx `214938-18d2e6663e8874f3`.
- Reproduce from quoteNetzipRs using the isolated environment and explicit
  `NETZIP_NATIVE_AUDIT_BINARY` pointing to the pinned executable:
  `python -m unittest discover -s scripts -p test_replay_official_5188_native_record.py -v`.
  CLI accepts executable and payload paths; add `--frame` for prefix/state
  propagation, or `--ordinal 25` for the conditional single-record run.
