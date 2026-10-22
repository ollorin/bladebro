# Phase 8: Tab Following - Research

**Researched:** 2026-10-22
**Domain:** recorder rebinding across tabs (Rust, tokio, CDP over the page WebSocket, ffmpeg stdin pipe)
**Confidence:** MEDIUM. The code facts are HIGH (read). The Chrome behaviors (`/json/list` order, `attached`, cross-origin target id, popup foregrounding) are MEDIUM/LOW and must be settled by live checks. No browser was run.

<user_constraints>
## User Constraints (no CONTEXT.md for Phase 8; taken from the coordinator brief, verbatim intent)

### Locked
- CAPT-03 default mode: ONE file, same encoder/ffmpeg process; each switch is a segment boundary `[<offset> s tab <id>]` in record-stop; OUT-05 `segments` extended.
- CAPT-04 opt-in per-tab-files mode: one file + encoder per tab, cap ~8 tabs with an honest note, record-stop lists all files compactly.
- Closing the recorded tab / cross-origin target-id swap / a backgrounded tab must not silently end the recording (rebind or honest note, CAPT-05 style; never a clean success when frames were lost).
- All logic in NEW files under src/record/ (hub/follow.rs etc.). Upstream files get at most tiny hooks (<= ~10 lines), PREFER ZERO: follow by polling from inside the hub, not by hooking `Page::switch_tab`. Five tools only, no new `state` op.
- New option `record.tabs` = "follow"|"per-tab" + env `BLADE_RECORD_TABS`, through src/record/layered.rs (per-call > env > project > user > default), docs/recording-and-launch-config.md with its drift test, project-file trust rules.
### Discretion
- Poll interval, rebind mechanics, report format, per-call layer exposure, test seams, plan split.
### Deferred
- Phase 9 (exit-path finalization), anything in `npm/`, schema changes beyond what is listed below.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CAPT-03 | default mode follows the active tab, segments reported | sections 1, 2, 3, 4, 6 |
| CAPT-04 | opt-in per-tab files | section 6 (per-tab = same follower, different sink policy) |
| CAPT-05 (ext.) | no clean success when frames lost | section 4 merge rules |
| OUT-05 (ext.) | segments list multi-entry | section 6 |
</phase_requirements>

## Summary

The hub has no way to ask the agent's `Page` anything from a background task: `Hub` is a `Mutex<Hub>` global (src/record/hub.rs:41,45) with no task of its own, and `HubEnv` is borrowed per call (hub/env.rs:33). The `Page` is only visible inside a `state` call (`try_handle(args, page)` hub.rs:153). So following must be an OWNED background task (the "follower") spawned at record-start, observing Chrome from outside via the HTTP `/json/list` (no CDP traffic, no page-visible effect). The only "current tab" signal usable from outside is Chrome's list: order (most recently activated first, MEDIUM) and the `attached` flag. `Page::switch_tab` ends with `new_page.activate()` (`Page.bringToFront`, src/page/tabs.rs:8-14, 245-247) and drops the old `Page`, so the old tab's WebSocket closes.

Rebinding is cheap because a `Source` is one WebSocket + one loop, and the encoder is already shared as `Arc<Encoder>` behind `Arc<dyn FrameSink>` (hub/machine.rs:152). Start a NEW `Source` per tab, all feeding the SAME encoder through a per-source wrapper sink. No change to `Source` internals is required except a 3-line `is_finished()`. The wrapper sink neutralizes `mark_end` (which would otherwise end the video at the first switch, capture.rs:357-360, process.rs:210-212), drops frames from a retired source (timestamp monotonicity), and records each segment's first frame timestamp (true video offsets).

Per-tab mode is the same follower with a different sink policy (map tab -> encoder). Build it second.

**Primary recommendation:** one `Follower` tokio task in `src/record/hub/follow.rs` polling `/json/list` every ~200 ms with the rule "agent tab = a page that is `attached` but is not a tab the recorder itself is connected to, else the recorded tab", 2-sample confirm, start-new-source-before-stopping-old, wrapper sink, merged report. Zero upstream hooks.

## 1. How the hub learns the agent's tab (Q1)

Facts:
- Phase 7 `current_tab` = `Target.getTargetInfo` on the AGENT's session (hub/live_env.rs:46-57). `own_target` does the same for the ListTabs hook (hub.rs:79-86, state.rs:259). Both need `&Page`/`&CdpSession`, so they cannot be called from a background task. They are the ORACLE for live checks (compare follower segments to `state tabs` `*`), not the runtime signal.
- Why the Chrome `attached` flag alone fails: the recorder's own page-WS marks the recorded tab attached (07-04 gap, hub.rs:76-78). While recording, attached = {recorded tab} + {agent tab}.
- WS mode only matters (pipe/real/ephemeral lanes are refused, machine.rs:114-119). `list_page_targets(base)` (src/cdp/discovery.rs:81) returns `TargetInfo{id,url,attached,web_socket_debugger_url}` in Chrome order. The sidecar `poll` used "first real page in /json/list" and tracked open-tab AND switch-tab over 3 segments, 7-10 ms rebind (RESULTS.md:33-35, 110). Upstream itself treats `tabs.first()` as the next tab after close-tab (src/mcp/server/state.rs:134-136).
- Caveat: the sidecar attached with a browser-level flat session, so `attached` never confounded it. Our recorder uses the page WS, so the attached-based rule below is NEW and unproven.

Recommended rule (pure fn `agent_tab(list, bound: &[id]) -> Option<id>`, unit-testable):
1. `pages` = type page, url not `devtools://`.
2. `foreign` = pages with `attached` and id not in `bound` (ids the follower holds a connection to). If non-empty, agent = first of `foreign` in list order.
3. Else agent = the current bound tab (agent is on it, or nothing else is attached).
4. If the bound tab is absent from `pages`: agent = first of `foreign`, else first of `pages` (mirrors upstream `tabs.first()`), else none (see section 4).
5. Fallback if live checks show `attached` unreliable (e.g. auxiliary attaches by Bladebro): first page in list order (the sidecar rule), with popup debounce below.
- Popup (section 5): an unattached new page never wins rule 2, so a popup does not flip the video.
- Confirm: switch only after the same candidate is seen on 2 samples, the second taken ~50 ms after the first (fast confirm). Needed because on a return to the bound tab the old agent tab is still attached for a few ms (switch_tab attaches new before dropping old, tabs.rs:238-262).
- Interval: 200 ms default (const, not an option, like `NO_FRAMES_AFTER`). One loopback HTTP GET per tick, no CDP method, no stealth surface. Detection latency ~200-250 ms worst, plus ~10 ms rebind. Frame loss: the new tab's first frame arrives immediately after `startScreencast` (a static page still sends one), so only up to ~250 ms of animation of the new tab is not in the file; the old tab stops delivering (hidden), so the video holds the old frame in that window. Segment offset is therefore accurate to ~one poll. Say so in docs.
- HubEnv gets two additions (tests inject): an owned `TabProbe` (`async fn list() -> Result<Vec<TabView{id,url,attached,ws_url}>>`; use an enum `Http{base}` | `Scripted(...)` or a boxed-future closure, NOT the async_trait crate: no new dependency) and the follower always starts sources with `Source::start_ws(tab.ws_url, ...)`, so Source's own /json retry loop (capture.rs:94-111) is bypassed.
- Zero hooks: `PageEnv::attach()` already returns the base (live_env.rs:59); the probe is built from `Attach::Port(base)`.

## 2. Rebind mechanics (Q2)

- One `Source` is bound to one WS for life (capture.rs:124-191). It cannot be re-pointed; a new `Source` per tab is the design. `Source::start_ws` is already `pub` and takes `Arc<dyn FrameSink>`.
- Sink ownership: `Source` moves its `Arc<dyn FrameSink>` into `Loop` and drops it when the loop ends (capture.rs:416). `take_encoder` retries `Arc::try_unwrap` 20x50 ms (machine.rs:70-85). So every wrapper must be dropped before final `take_encoder`; the follower drops its wrappers after joining pending stops.
- `mark_end` trap: `Loop::run` calls `sink.mark_end()` on stop (capture.rs:357-360); `Encoder::mark_end` is first-call-wins (process.rs:210-212). A rebind that stops the old Source would end the whole video. Fix with NO source change: `SegmentSink { enc: Arc<Encoder>, gen: u64, shared: Arc<FollowShared> }` implementing `FrameSink` with `mark_end` a no-op. The follower calls `enc.mark_end()` itself as the first thing in the final stop (keeps "video ends at the stop request", docs line 63).
- Monotonic timestamps: `Pacer::on_frame` takes `saturating_sub` gaps but then sets `last_ts = ts_ms` (pace.rs:66-75), so an out-of-order frame from a retiring Source would shift the clock back and inflate the next gap. `SegmentSink::send_frame` returns `true` (not counted as a queue drop) and discards when `gen != shared.current_gen`. The old Source's trailing held frame (handler take_held, capture.rs:406-408) is thereby dropped; loss <= one fps-cap interval.
- Frame timestamps are Chrome `metadata.timestamp` (epoch ms, frame.rs); same browser clock for all tabs. A tab absent for T seconds produces a gap of T in `Pacer`, written as repeats of the last frame (continuous wall-clock timeline). A single gap > 10 min is clamped (`MAX_GAP_MS`, pace.rs:32) and counted in `gaps_clamped`.
- Order on switch: (1) `Source::start_ws(new)` first (~10 ms), (2) bump `current_gen` (new wrapper), (3) retire the old Source in the background: `tokio::spawn(old.stop())` and keep the `JoinHandle<SourceReport>` in `pending`. Do NOT await the old `stop()` on the critical path: it takes at least the 500 ms grace (capture.rs:223, 392). Each old `stop()` also sends `Page.stopScreencast` on its own socket only (SAFE-01 unchanged: the four `RecMethod`s).
- Segment offset: `SegmentSink` stores `first_ts_ms` of the first frame accepted for its generation; `at_s = (first_ts - video_t0_ts)/1000`, where `video_t0_ts` is the first accepted frame overall (matches "clock starts at the first frame", docs line 63 and the Pacer timeline). A segment with no frame yet uses `Instant` since `t0` or 0.0 before any frame.
- Stop-grace: final stop = `enc.mark_end()`, then stop the CURRENT source (one grace), then join `pending` (bounded 2*grace+3 s each, already inside Source::stop, capture.rs:223), drop wrappers, then existing `take_encoder` + `enc.finish()`. Total stop time stays ~0.5 s plus joins in parallel.
- Gate/no-frames: `Source::stop` converts `Captured` + 0 frames into `NoFrames` per Source (capture.rs:238-244). With many sources a quick tab visit would look like a failure; so the follower MERGES (section 4) instead of passing a raw per-source report to `compose`.
- Minimal source-API change: add `pub fn is_finished(&self) -> bool { self.task.as_ref().map_or(true, |t| t.is_finished()) }` to `Source` (3 lines, capture.rs). Optional: `stats()` and `health()` already exist and are pub. Nothing else.
- Hub change: `Active.src: FrameSource` (machine.rs:51) becomes `Active.rec: Tracker` (a handle in follow.rs exposing `health()`, `current_tab()`, `stop() -> FollowResult`). `msg::active` takes `&a.src.health()` (machine.rs:124); the handle reads a shared snapshot the task refreshes each tick. `finish_active`/`finish_orphan` call `tracker.stop()`.

## 3. Frame size across tabs (Q3)

`build_args` (encoder/args.rs:126-129): `-vf scale=W:H:force_original_aspect_ratio=decrease:force_divisible_by=2,pad=W:H:(ow-iw)/2:(oh-ih)/2,format=yuv420p`, no output `-r`, `-fps_mode passthrough`. Output is a fixed W x H canvas, so different tab frame sizes are letterboxed, not an encoder error. ffmpeg's default filtergraph reinit (`-reinit_filter` default on) handles a mid-stream size change in image2pipe+mjpeg; the Phase 2 3-segment run decoded fully (RESULTS.md:33-34) but in headless with equal viewport sizes, so a REAL size change is unproven (MEDIUM). Popups with `window.open(url,'', 'width=500,height=400')` give a different size; `maxWidth/maxHeight` only caps (capture.rs:151-157). Add a real-ffmpeg test (skips when no ffmpeg, like encoder/real_tests.rs:1-4) writing frames of two JPEG sizes into one `Encoder`, ffprobe expects constant WxH and the full frame count. No code change expected; contingency: none needed unless that test fails.

## 4. Close, cross-origin swap, backgrounded tab (Q4)

What the source sees:
- Tab closed or browser gone: Chrome closes the page WS. The client's event bus does NOT report Closed while a handle lives; detection is the 250 ms `LIVENESS_TICK` `session.is_closed()` (capture.rs:28, 395-399) -> `lost=true` -> `SourceOutcome::ConnectionLost` (capture.rs:417-419). The follower sees this via `Source::is_finished()` on its tick (also covers the `/json/list` absence of the id). Disambiguate: probe OK and id absent = tab closed -> REBIND (rule 4 above) with a note; probe error (connection refused) = browser gone -> stop polling, keep the encoder, mark `Lost`; `record-stop` then reports "recording not clean" as Phase 7 `h_` does (machine_tests.rs:287-306).
- Never end silently: if no page remains, the follower records `ended: no tab left at X s` and stays Active until record-stop, which returns Err (not clean) with the path (file kept).
- Cross-origin swap: UNPROVEN. Phase 6 only saw same-process navigation keep the WS (06-03-SUMMARY.md:36); the PITFALLS claim (stops silently, PITFALLS.md:187) is also unverified. Two cases the follower handles generically with the same code as close: (a) target id changes / WS closes -> id absent or `is_finished` -> rebind to the rule-4 candidate with note `tab X replaced by tab Y at T s`; (b) WS and id survive but frames stop: indistinguishable from a static page (watch.rs:3-11), so nothing is done; contingency only if the live check shows it: re-send `Page.startScreencast` (already in `RecMethod`) after a `Page.frameNavigated` seen on the recorder session (Page.enable is on, handler.rs:104 currently ignores it). Decide after the live check.
- Backgrounded tab: the follower binds the agent's tab, which Bladebro foregrounds (`activate()`; tabs.rs:8). Hidden tab = 0 frames (RESULTS.md:40-41). Per segment merge rule (a new fn `merge(reports, segments)` in follow.rs, pure and table-tested): total frames 0 -> existing `NoFrames`; any segment with 0 frames lasting >= `no_frames_after` (5 s) -> not clean, text names the segment (`tab X: no frames for N s ...`); shorter zero-frame segment -> kept in `segments` as `(no frames)` plus a note, still clean. `quiet_tail_ms` stays from the last segment only. `ConnectionLost` that was rebound is not a failure; an UNREBOUND one is. Stats are summed (frames_received, forwarded, lost, queue_dropped, capped, bad_frames); `compose` consumes one synthetic `SourceReport`.

## 5. Popups and new windows (Q5)

- Bladebro's `Page` does NOT become the popup. After a click that opened a tab, `Page::action` lists targets before/after and, if new tabs appeared, calls `self.activate()` on the OPENER (page/mod.rs:590-619), verdict `outcome: new tab opened`. Tool text says so: "Popup clicks keep the opener active; switch-tab explicitly to use the popup" (mcp/tools.rs:43). `eval` with `window.open` only reports popups (mcp/server/eval.rs:202-232) and does not re-activate. Input actions call `activate()` first (page/mod.rs:524-526).
- Visible from outside as: new unattached page, momentarily first in `/json/list` (Chrome foregrounds it) until the opener's `bringToFront` re-promotes the opener. Hence the `attached` rule + 2-sample confirm; a popup that the agent never switches to is NOT recorded. This differs from the ROADMAP wording ("a popup rebinds capture") and from PITFALLS 9: the correct reading of "what the agent is looking at" is the agent's `Page`. The popup becomes a segment only after `switch-tab`/`open-tab` (open-tab = `open_tab_target` then `switch_tab`, mcp/server/state.rs:46-47).
- Gap to document: after an `eval` popup (headed) Chrome keeps the popup in front while the opener is hidden, so the opener's source can go silent until the next input action; reported by the normal quiet-tail note, not a failure.

## 6. Per-tab mode, report, names (Q6)

Current report (hub/report.rs): `Segment{at_s, tab}` (line 15), `fields_line` (38-62) = `<path> (bytes N, duration D s, frames N, dropped N, segments [..][, captured N])`, `compose` (66-98) = `recording saved: ...` (ok) or `recording not clean: <verdict>; <fields>` plus `. Note: <tab note>`; `finish_active` builds the one-element segments (machine.rs:237). The Phase 7 stop note `msg::tab_moved` (msg.rs:87-92, "tab following is not implemented yet") and test `k_` (machine_tests.rs:362-378) are obsolete and must be replaced. Default-mode stop: segments become `[0.0 s tab A, 4.2 s tab B, 9.8 s tab A]`; notes from the follower (`tab X closed at T s; following tab Y`, `tab Y: no frames`) are joined into the existing `. Note:` slot. No new fields.

Per-tab mode (`record.tabs=per-tab`):
- Same follower, sink policy `PerTab`: map tab id -> `{enc: Arc<Encoder>, path, n}`. On the agent entering a tab: if it has an encoder, start a new Source into it (rebinding to the SAME file on return); else if count < 8, create file + `Encoder::start` + Source; else cap note. Only the agent's current tab has a recorder connection at a time, so the section-1 rule works unchanged (a dormant tab holds no connection).
- Dormant tab file timeline: its encoder holds the last frame (pacer) until the tab is revisited or stop, so each file spans first visit -> stop with frozen time in between. Document it; `MAX_GAP_MS` 10 min clamp applies. Alternative (finish a tab's encoder when the agent leaves) is rejected: it breaks "one file per tab" on return.
- Cap: `MAX_TAB_FILES = 8` const. The 9th distinct tab is NOT recorded; the follower keeps the previous tab's source retired (no frames) and notes `tab Z not recorded: per-tab cap of 8 files reached`. Recommend this is a note with a clean result when everything else is clean (documented limit, not loss); flag to coordinator as a decision (a stricter reading = not clean).
- Names: add `names::tab_stem(stem, n) -> String` (sanitized stem cut to MAX_STEM-6 then `-tab<N>`, N = first-visit order), fed to `create_recording_file` via a cloned `cfg.name` (dir.rs:230-241). Result `<stem>-tab2-<UTC>-<id>.<ext>`; the unique id keeps it collision-free. The first file keeps tab 1.
- Report: `recording saved (per-tab, 3 files): <fields_line file1> ; <fields_line file2> ; ...` where each fields line reuses `fields_line` with that file's own segments (visits, offsets relative to that file's start) and the line's `dropped` from that encoder. `ok` = all encoders ok and merged source ok. Put the composer in a NEW `hub/follow_report.rs`; leave `compose` untouched for the one-file path.
- Config key `tabs` (values `follow` default, `per-tab`): add `Tabs` enum + `parse_tabs` + `ENV_RECORD_TABS = "BLADE_RECORD_TABS"` in values.rs (beside ENV_RECORD_SIZE, values.rs:21); `tabs: Option<String>` in `FileConfig` (config.rs:27, `deny_unknown_fields` makes it valid in record.json and the project `record` section at once, since both use `FileConfig`, config.rs:154-166); `tabs: Option<String>` in `RecordOverrides` and `Parsed` + `parse_layer` + precedence `call.or(env).or(proj).or(usr).unwrap_or_default()` in layered.rs:33-36, 40-76, 94-102, 158-172; `pub tabs: Tabs` on `RecordConfig` (config.rs:60). Per-call: add the field for resolver parity (the library keeps per-call for format/fps/size, docs line 53) but do NOT expose it in the `state` schema (every schema word is billed, AGENTS.md ask-first); docs row says "none via the `state` tool". Error prefixes follow the existing pattern (`BLADE_RECORD_TABS:`, `record.json tabs:`, `<path> record.tabs:`, `per-call tabs:`). Project-file trust needs nothing new: it is an enum, not a path (unlike `dir`, which has vetting).
- Drift test: src/launch_mode/registry.rs lists `RECORD_FILE_KEYS` (line 25) and the env list (lines 15-21); the docs test compares serde keys <-> registry <-> docs table both ways (registry.rs:113-132, 232-280, plus `source_literals_are_registered` which scans src/record for `BLADE_*` literals, line 189). Add `"tabs"` and `ENV_RECORD_TABS` there, one row in the env table (docs line ~19), two rows in "Config keys" (record.json, `.bladebro.json record`), one in the precedence table (docs line ~176), and rewrite docs lines 86, 116-117, 135 (they say one segment and "does not move"). Also update the examples/notes list at line 117.
- Upstream text that goes stale: mcp/tools.rs:159 "video of the active tab at start" and cli/help.rs:667 "video of the active tab". Both are upstream files with ledger rows already. Recommended: leave tools.rs unchanged (still true at start; schema budget, CONTEXT D-01) and decide at plan time whether the 5-token help.rs tweak is worth another ledger row. No new hook is needed for the feature.

## 7. Test seams (Q7)

Existing: `FakeCdp` = one-connection WS server with `frames`, `close_now()`, `push_raw()` (source/fake_cdp.rs:61-221); `machine_tests.rs` `TestEnv` with fake ffmpeg script `cat > "$out"` and `Attach::Ws(fake.url)` (lines 33-110); `take_encoder` knob tests (`j_`). `FakeCdp` accepts ONE connection (accept called once, line 71) so each fake = one tab.
Add:
1. `TabProbe::Scripted` (test enum variant) returning a `Mutex<Vec<TabView>>` the test mutates; each `TabView.ws_url` = a distinct `FakeCdp.url`. No HTTP server needed.
2. Follow scenarios (hub/follow_tests.rs, unix-gated like machine_tests): switch A->B->A (3 FakeCdps; the A->B->A return needs a SECOND `FakeCdp` for A since one fake accepts one socket); segments text with offsets from frame timestamps; open-tab (new id attached); popup (unattached new id) does NOT switch; confirm debounce (single transient flip ignored); recorded tab closed (`close_now` + id removed from list) rebinds with a note; all tabs gone -> not clean, file kept; browser gone (probe Err) -> not clean; zero-frame >=5 s segment (use `no_frames_after` knob) -> not clean; short zero-frame segment -> clean + `(no frames)`; `mark_end` not called on switch (fake ffmpeg frame/size count: no early end) and exactly once at stop; stale-frame drop; double `record-start` still refused; `take_encoder` succeeds after N switches (all wrappers dropped); per-tab: files count, cap of 8 with the 9th noted, names `-tab<N>-`, return-to-tab reuses the file.
3. Pure unit tests: `agent_tab` table, `merge`, `tab_stem`, `parse_tabs`, layered precedence rows for `tabs` (copy `format_fps_size_precedence_with_project`, layered.rs:232), config-key drift tests (registry.rs).
4. Real-ffmpeg two-size test (section 3). Update `k_` and msg tests; `adapter_tests`/`schema_tests` budget unaffected.
5. Live (`#[ignore]` + scenario script, like hub_live.sh): fixtures `tools/record_sidecar/fixtures/` already has a.html b.html anim.html; add `popup.html` (target=_blank link, `window.open` with size features) and `redirect.html` (cross-origin nav on 127.0.0.1 vs localhost ports). Assert via ffprobe frame count = reported and the `state tabs` oracle.

## 8. Recommended decomposition

**08-01 default follow + segments** (CAPT-03, SC1, SC2, SC4 part): `follow.rs` (Follower task, `agent_tab`, `SegmentSink`, `merge`), `TabProbe` + `HubEnv` additions + `PageEnv` probe, `Source::is_finished`, `Active.rec` swap in machine.rs, `msg` updates (replace `tab_moved`), report text, fixtures, follow_tests, real two-size ffmpeg test, live scenario `follow` (switch, open-tab, popup, close recorded tab, cross-origin), docs lines 86/116-117/135 rewritten. Done = existing audit 111/111 count unchanged vs Phase 7 numbers, CDP trace shows only the 4 RecMethods plus HTTP polls, `HOOKS.md` unchanged (zero hooks).
**08-02 per-tab files** (CAPT-04, SC3, SC4): sink policy `PerTab`, `names::tab_stem`, `follow_report.rs`, config key `tabs` end to end (values/config/layered/registry/docs/drift/precedence tests), cap handling, live check with 3 tabs, docs per-tab section.

## Risks / unknowns for the live checks

1. `/json/list` MRU order after `switch_tab`/`open-tab`/popup+reactivate, headless AND headed (macOS), settles rule step 5 fallback.
2. `attached` semantics: does a transient Bladebro auxiliary attach (`attach_tab_target`, tabs.rs:205-232) or DevTools ever flag a non-agent tab? Does the old agent tab drop `attached` promptly (<50 ms) after `*self = new_page`?
3. Cross-origin navigation: does the page target id change, does the page WS close, do frames continue (needs two origins; localhost vs 127.0.0.1)?
4. Real frame-size change through ffmpeg (popup with window features).
5. Hidden-popup case after `eval` window.open (headed).
6. Cost of a 200 ms poll on a loaded browser (expect negligible; measure with lane_matrix).
7. Race on rapid switch bursts (A->B->C within one poll): the follower should jump to the latest confirmed candidate, skipping unvisited tabs; intermediate tabs get no segment. Confirm acceptable.
8. Daemon lane: the follower task must live on the daemon runtime and survive between calls; one-shot is already refused (hub.rs:49-57). MCP lane likewise. `finish_best_effort` (hub.rs:62) must stop the follower (Phase 9 owns the full path; here only drop-safety: follower aborted + encoder `kill_on_drop` backstop).

## Open questions / uncertainty

- `/json/list` ordering by last activation is from the sidecar result plus my recollection of Chrome's devtools_http_handler; not verified in Chrome source this session (no network). MEDIUM.
- The `attached`-based agent detection is a new design that I derived from the 07-04 finding; unproven live. The sidecar fallback (first page) is proven only for single-agent switches.
- Popup semantics diverge from the ROADMAP/REQUIREMENTS wording ("popups rebind"). Coordinator must confirm: popup is followed only once the agent switches to it.
- Cap policy (note vs not-clean) and zero-frame-segment policy (5 s threshold, not clean) are my recommendations, not locked decisions.
- Whether `Page.startScreencast` survives a cross-process navigation without re-issue is unknown.
- Per-tab dormant files hold a frozen last frame for the whole absence; confirm this is wanted vs shortening via finishing the encoder.
- I did not read `capture_tests/` bodies, `battery`/`audit` tooling, or `launch_mode::project_in`; the key-flow claim for the project file rests on `FileConfig` being shared (config.rs:154-166, registry.rs:259-261).

## Sources
HIGH (code read): src/record/hub.rs, hub/{machine,env,live_env,msg,report}.rs, source/{capture,handler,sink,report,wire,fake_cdp,watch}.rs, encoder/{process,args,pace,writer}.rs, names.rs, dir.rs, layered.rs, config.rs, values.rs, src/page/{tabs,mod}.rs, src/mcp/server/{state,eval}.rs, src/state.rs, src/launch_mode/registry.rs, docs/recording-and-launch-config.md.
MEDIUM (project evidence): tools/record_sidecar/RESULTS.md, lib/follow.mjs, sidecar.mjs poll section, 06-03-SUMMARY.md, 07-VERIFICATION.md.
LOW: Chrome internals recalled from memory (list ordering, target-id stability); no web access used.

**Valid until:** until Chrome major changes or live checks contradict section 1 (re-verify per Chrome version, AGENTS.md gotcha on version sensitivity).
