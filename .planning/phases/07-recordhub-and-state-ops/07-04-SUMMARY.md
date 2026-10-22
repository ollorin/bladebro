# 07-04 Summary: verification gap fixes

- Gap 1: `state tabs` marked the recorded tab current too. New `hub::own_target` (target id of the agent's session, only while a recording is active; no CDP call otherwise) and pure `hub::is_current`; hooked in `src/state.rs` ListTabs. 3 unit tests (off mode follows attached; recorded tab not current when agent elsewhere; agent on recorded tab). Live (headless, daemon): record-start, open-tab, `tabs` showed exactly one `*` (agent's tab); one after record-stop; no processes left after `bladebro stop`.
- Gap 2: docs `captured` row rewritten: frames passed to the encoder after the fps cap (report.rs prints `src.stats.forwarded`); can exceed or be below `frames`.
- Hook ledger: new row in .planning/HOOKS.md (commit 761910a).
- Gates: fmt, clippy --all-targets, cargo test --release green (lib 628).
