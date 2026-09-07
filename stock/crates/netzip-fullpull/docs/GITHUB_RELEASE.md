# GitHub update procedure

Published repositories:

- Buildable shared-code suite: <https://github.com/beyondcy1013/netzip-fullpull-suite>
- Shared agent Skill: <https://github.com/beyondcy1013/quote-netzip-fullpull-replication>

The suite preserves the relative source layout for `netzip-fullpull` and its
direct `dllhqarrow-rs` dependency. Publishing a suite snapshot does not add a
remote to the enclosing local worktree; future updates must repeat the consumer
inventory, verification, snapshot, push, and remote-commit audit.

The managing agent owns cross-repository publication after a major protocol or
algorithm breakthrough.

## Mandatory inventory

Before commit or push, enumerate every `Cargo.toml` path dependency and verify
the consumer matrix in `ACCEPTANCE.md`. Record the exact shared-crate commit,
consumer commits, fixture hashes, webClx request IDs, and unresolved gates.

Run from this documentation directory:

```bash
../scripts/list-direct-consumers.sh
```

## Repository boundary

`quoteNetzipRs` is an independent sibling Git repository with GitHub remote
`beyondcy1013/quoteNetzipRs`. The shared crate and `netzip_win` currently belong
to the enclosing workspace Git worktree. As of 2026-09-07 that worktree has no Git
remote configured. Therefore pushing only `quoteNetzipRs` does not publish
`netzip-fullpull` or `netzip_win`.

This prevents a direct push from the enclosing worktree. The published suite is
the current portable snapshot path; before claiming a later GitHub update
includes all crates, either repeat its audited snapshot workflow, configure an
authoritative remote for the worktree, or extract the crates into independently
versioned repositories and update every consumer dependency atomically.

## Push gate

1. Confirm clean ownership of intended files; preserve unrelated dirty work.
2. Verify the shared crate and every direct consumer.
3. Commit protocol docs and code together in the shared repository.
4. Update consumer lockfiles/revisions and project integration docs.
5. Push each affected repository and verify remote commit IDs.
6. Never state “GitHub updated” until remote inspection proves every affected
   crate/project commit is reachable.
