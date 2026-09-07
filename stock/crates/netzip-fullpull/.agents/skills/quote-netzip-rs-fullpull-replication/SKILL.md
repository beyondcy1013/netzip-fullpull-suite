---
name: quote-netzip-fullpull-replication
description: Govern the authenticated official 5188 full-push protocol shared by netzip-fullpull and all Rust consumers.
---

# Shared official full-push governance

Published source suite:
`https://github.com/beyondcy1013/netzip-fullpull-suite`.
Published Skill:
`https://github.com/beyondcy1013/quote-netzip-fullpull-replication`.

## Authority

- Resolve the shared implementation from the current consumer's
  `netzip-fullpull` Cargo path dependency. Do not assume a host-absolute path;
  the workspace may be mounted through Samba or SSHFS.
- The authoritative project-level Skill is stored with that crate at
  `.agents/skills/quote-netzip-rs-fullpull-replication/SKILL.md`. Install a
  complete mirrored copy at the same `.agents/skills/...` path in every direct
  product repository so project-scoped agents and Samba-mounted workspaces can
  load it. An empty directory or symlink is not an installation.
- After editing the authoritative Skill, copy the complete file to
  `quoteNetzipRs` and `netzip_win`, run the user-level multi-agent sync, and
  require identical SHA256 values for every mirror before finishing.
- From that resolved crate, read `docs/STATUS.md`, `PROTOCOL.md`, `EVIDENCE_INDEX.md`,
  `HYPOTHESES.md`, `ACCEPTANCE.md`, and `GITHUB_RELEASE.md` first.
- Protocol facts, decoder gaps, fixtures, and acceptance state belong there.
- Product runtime/API/deployment facts remain in the consuming project.
- Project `progress.MD` files are journals, not protocol authority.

## Documentation routing

Write each result to exactly one authoritative destination:

| Content | Authoritative file |
|---|---|
| Current shared protocol/decoder gap, owner, status, next gate | `netzip-fullpull/docs/STATUS.md` |
| Independently confirmed wire, decoder, state, or projection rule | `netzip-fullpull/docs/PROTOCOL.md` |
| Reusable capture, replay, tooling, account, timing, or diagnosis lesson | `netzip-fullpull/docs/EXPERIENCE.md` |
| Fixture, pcap, disassembly, report path, scope, and SHA256 | `netzip-fullpull/docs/EVIDENCE_INDEX.md` |
| Open explanation, falsifying test, or rejected hypothesis | `netzip-fullpull/docs/HYPOTHESES.md` |
| Shared promotion criteria and direct-consumer verification | `netzip-fullpull/docs/ACCEPTANCE.md` |
| Git repository inventory, commit/push procedure, and remote audit | `netzip-fullpull/docs/GITHUB_RELEASE.md` |
| Linux runtime/API/shadow/publication/rollback detail | `quoteNetzipRs/docs/` |
| Windows driver/GUI/service/build/deployment detail | `netzip_win/docs/` |
| Chronological terminal activity before promotion | the owning project's `progress.MD` |

Do not copy a shared gap or experience section into both product repositories.
Product documents link the shared authority and contain only product-specific
impact or acceptance. Promote a journal observation only after evidence review;
until then keep it in diagnostics/progress and, when useful, as an explicit
hypothesis rather than a protocol rule.

## Consumers

Current direct consumers include `quoteNetzipRs`, `netzip_win` packages
`netzip-driver-hub` and `netzip-service`, `tdxRs/tdx-runtime`,
`netzip-supplement`, and `stock-source-netzip`. Re-enumerate Cargo path
dependencies before every major release.

## Evidence discipline

- Record fact, hypothesis, and verdict separately.
- Scope evidence by account, session, TCP flow, `0104` version, time window,
  producer version, and SHA256.
- Keep frame, error-record, omitted, completed, EOF/clamp, missing-seed,
  accepted, and rejected counts separate.
- Dynamic parity requires market/code and exact business second/state group.
- A connection, all-clean replay, internal commit, or successful build does not
  independently prove business correctness.
- Never change token/mask semantics from OEM hit rate alone.

## Current boundary

- NativeWineClamp is `mechanism-pass / business-fail` and explicit opt-in.
- Strict remains the default decoder; `publication=disabled` remains.
- A default-off, bounded, nonblocking, authenticated, noncanonical shadow may
  be deployed through webClx, but must not feed canonical quotes.

## Workflow

1. Claim one-writer ownership for shared source and authority documents.
2. Reproduce narrowly, then replay all required fixtures.
3. Update shared status, protocol, evidence, or hypothesis documents.
4. Run every direct consumer in the shared acceptance matrix through webClx.
5. For a major algorithm breakthrough follow `docs/GITHUB_RELEASE.md`: inspect
   repository boundaries, commit shared code/docs, update consumers, push each
   authoritative repository, and verify remote commit IDs.
6. Never claim all crates reached GitHub when only `quoteNetzipRs` was pushed;
   the enclosing shared-crate worktree currently has no configured remote.

## Ownership

- `netzip-fullpull`: auth protocol, 5188 transport/init/tables/2704/state.
- `quoteNetzipRs`: Linux runtime, shadow/API, publication and rollback.
- `netzip_win`: Windows driver/GUI/service, target build and deployment.
- 7709/K-line/F10 belongs to supplementation, not official full-push.

Use `webclx-compile-and-deploy` for Rust checks and deployment. Preserve
unrelated dirty work. Credentials remain runtime-only and never enter evidence,
messages, docs, commits, or fixtures.
