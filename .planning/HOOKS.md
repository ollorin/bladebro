# Upstream hook ledger (SYNC-01)

Every edit this project makes to a file that exists in `upstream/main` is listed here, so a merge of upstream can be
reviewed quickly and conflicts localized. New files (e.g. `src/record/**`, `tools/record_sidecar/**`) need no entry.

| Phase | File | Lines / symbol | Reason | Added by commit |
|-------|------|----------------|--------|-----------------|
| 3 | src/lib.rs | `pub mod record;` (between realbrowser and reddit) | expose the fork-only recording module | 5782a59 |
| 3 | README.md | new "## Recording (in development)" block before the Maintenance heading (additions only) | document recording config and the sensitive-content warning (SAFE-03) | 1f9e11d |
| 4 | src/updater/doctor.rs | `checks.push(check_ffmpeg());` after check_disk_space + `fn check_ffmpeg` before check_version | doctor reports ffmpeg path/version/encoders (OUT-02); logic in src/record/ffmpeg/report.rs | f7e51f1 |
