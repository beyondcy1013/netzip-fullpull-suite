# Acceptance gates

## Protocol change

1. Focused fixture test.
2. Full-init, close-window, and 168-clone replay.
3. Frame/record/omitted/runtime accounting conservation.
4. Per-symbol state propagation comparison and first divergence.
5. No token or mask change justified only by public hit rate.

## Consumer matrix

Every material shared-crate change must verify all direct consumers:

Enumerate the current set before relying on this table:

```bash
../scripts/list-direct-consumers.sh
```

| Consumer | Required check |
|---|---|
| `netzip-fullpull` | fmt, full tests, Clippy `-D warnings`, release build |
| `quoteNetzipRs` | workspace tests/Clippy/release; shadow API and conservation |
| `netzip_win/netzip-driver-hub` | driver lifecycle and Windows target build |
| `netzip_win/netzip-service` | API/status tests and source identity |
| `tdxRs/tdx-runtime` | workspace compatibility tests |
| `netzip-supplement` | manifest tests and Clippy |
| `stock-source-netzip` | manifest tests and Clippy |

Rust compilation and deployment use the webClx queue. A shared change is not
complete when only one consumer compiles.

## Promotion levels

- `experimental`: offline diagnostics only.
- `shadow`: isolated, noncanonical, default off, bounded nonblocking queue,
  authenticated diagnostics, explicit rollback.
- `canonical`: requires business parity, reconnect/readiness, coverage,
  freshness, consumer canaries, and rollback verification.
