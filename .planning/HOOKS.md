# Upstream hook ledger (SYNC-01)

Every edit this project makes to a file that exists in `upstream/main` is listed here, so a merge of upstream can be
reviewed quickly and conflicts localized. New files (e.g. `src/record/**`, `tools/record_sidecar/**`) need no entry.

| Phase | File | Lines / symbol | Reason | Added by commit |
|-------|------|----------------|--------|-----------------|
| 3 | src/lib.rs | `pub mod record;` (between realbrowser and reddit) | expose the fork-only recording module | pending (filled after Task 1 commit) |
