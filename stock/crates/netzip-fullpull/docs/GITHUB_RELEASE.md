# GitHub update procedure

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

This is a release blocker, not permission to copy shared source into a consumer.
Before claiming a GitHub update includes all crates, configure or identify that
worktree's authoritative remote, or extract the shared crate into its own
versioned repository and update every consumer dependency atomically.

## Push gate

1. Confirm clean ownership of intended files; preserve unrelated dirty work.
2. Verify the shared crate and every direct consumer.
3. Commit protocol docs and code together in the shared repository.
4. Update consumer lockfiles/revisions and project integration docs.
5. Push each affected repository and verify remote commit IDs.
6. Never state “GitHub updated” until remote inspection proves every affected
   crate/project commit is reachable.
