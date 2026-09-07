# netzip-fullpull suite

Rust replication workspace for the authenticated official 5188 full-push
protocol.

## Layout

- `stock/crates/netzip-fullpull`: authentication, 5188 transport,
  initialization, subscription tables, decoder, evidence, and acceptance docs.
- `crates/rustHq`: `dllhqarrow-rs`, the direct local dependency required by
  `netzip-fullpull`.

The layout intentionally preserves the source path dependency so a clone is
buildable without rewriting manifests.

## Status

NativeWineClamp is `mechanism-pass / business-fail`. It reproduces the native
EOF control flow in an explicit opt-in decoder, but dynamic business-field
parity is not complete. Strict remains the default and publication remains
disabled. Read `stock/crates/netzip-fullpull/docs/STATUS.md` before use.

## Verify

```bash
cargo fmt --manifest-path stock/crates/netzip-fullpull/Cargo.toml -- --check
cargo test --manifest-path stock/crates/netzip-fullpull/Cargo.toml --all-targets
cargo clippy --manifest-path stock/crates/netzip-fullpull/Cargo.toml --all-targets -- -D warnings
cargo build --release --manifest-path stock/crates/netzip-fullpull/Cargo.toml
```

Do not place credentials or credential-bearing captures in this repository.

