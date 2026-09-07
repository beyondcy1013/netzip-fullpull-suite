# Official 5188 status

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
commit. It does not establish business-field correctness. Exact-business-second
callback comparison still has near-zero dynamic-field parity, and an absolute
accumulator record for SZ000858 first writes a negative internal `0x14` value
before public projection.

## Open gates

1. Reconstruct absolute accumulator/token semantics and prove internal 311-byte
   volume/amount/OHLC values against Wine.
2. Prove NativeWineClamp state propagation per symbol; keep strict as default.
3. Close same-code, exact-business-second dynamic OEM parity.
4. Close same-`0104`-version P1-P7 parity and per-slot coverage.
5. Validate reconnect/day-cut/freshness/silent-drop behavior across ten slots.
6. Complete each consumer's integration tests and product canary.
7. Publish no canonical quote until all earlier gates pass.

## Accounting rules

Keep frame status, error records, omitted frames/indexes, completed records,
EOF/clamp events, missing seeds, accepted candidates, and rejected candidates
separate. Required runtime conservation:

```text
input_records = accepted + rejected + missing_seed
```

Reject reason hits may overlap. If totals are presented as a partition, also
provide one mutually exclusive primary reason per rejected candidate.

