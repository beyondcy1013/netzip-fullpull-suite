# Promoted protocol rules

Only independently supported rules belong here. Exploratory interpretations
remain in `HYPOTHESES.md`.

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

## Projection

- Public projection uses same-session `0104` metadata.
- Publish timestamp is floored to local 15:00 after close.
- STAR lot conversion uses positive half-up `/100`; a priced zero-volume level
  displays one lot. Levels 6-10 are compatibility zeros and are not a decoder
  gate.
- Runtime candidate validation remains transactional and fail-closed.

