//! Exit-path matrix (LIFE-01): which way a session can end, what happens to an
//! active recording, and where that is proven. Pure data plus the reasoning
//! behind it; docs/recording-and-launch-config.md renders the same table.
//!
//! Module map:
//! - `Row` / `MATRIX`: exit path x lane -> outcome -> test (or marker).
//! - why-comments below: signal flow per lane (file:line), Windows reasoning.
//!
//! Every surface finalizes through ONE function, `hub::finish_best_effort`:
//! it drops the producer, closes ffmpeg's stdin, waits a bounded time
//! (`FINISH_TIMEOUT` 10 s + `KILL_REAP_TIMEOUT` 5 s + stderr drain 1 s, so
//! ~16 s worst case for one file; per-tab bounds in follow/files.rs
//! `stop_cap`), kills as a last resort with an honest `unfinalized` outcome,
//! and keeps the report for the next record op, prefixed
//! `previous recording ended with the browser: `.
//!
//! Where it is called (line numbers at the Phase 9 commit):
//! - MCP: every Chrome shutdown goes through `boot.rs::shutdown_browser`, whose
//!   first line is the hook (idle shutdown, drift relaunch, `Closed` relaunch,
//!   stdin EOF / signal post-loop in serve.rs all call it).
//! - Daemon (`src/cli/daemon.rs`): idle timeout, drift/lost relaunch and the
//!   `Closed` relaunch each call it right before `b.shutdown()` (Phase 9
//!   hooks); the post-loop exit (SIGTERM, SIGINT, `bladebro stop`) calls it
//!   BEFORE the login snapshot so a dead Chrome cannot delay the finalize.
//!   The launch-failure cleanup (end of daemon.rs) is NOT hooked: no page ever
//!   existed there, so no recording can be active.
//! - One-shot CLI: `Lane::Ephemeral` refuses record-start, so nothing can be
//!   active at exit.
//!
//! Signals: no handler was added. MCP `wait_for_shutdown_signal` (boot.rs:
//! SIGTERM, SIGINT, SIGHUP; Windows ctrl_c) breaks the serve loop into the
//! post-loop `shutdown_browser`. The daemon (daemon.rs top: SIGTERM, SIGINT;
//! SIGHUP ignored on purpose, it fires when the first CLI parent exits)
//! breaks into the post-loop finish. A second signal during finish is
//! swallowed by the tokio handler, so the process stays until the bounded
//! finish ends; a SIGKILL from a supervisor still ends in a finalized file
//! because ffmpeg exits on stdin EOF (it is in its own process group on unix,
//! so a terminal Ctrl-C reaches bladebro only and cannot race the trailer).
//!
//! Windows (reasoned, NOT executed): only `ctrl_c` is handled. Console close,
//! logoff and shutdown events are not (that needs SetConsoleCtrlHandler, an
//! upstream architecture change; not done). When the parent dies, stdin EOF
//! reaches ffmpeg, which finalizes; the KILL_ON_JOB_CLOSE job object
//! (platform/windows_job.rs) already contains ffmpeg because the guarded
//! process spawned it. A TerminateProcess of bladebro leaves whatever ffmpeg
//! managed to write (possibly unfinalized).

/// One exit path in one lane.
pub struct Row {
    pub path: &'static str,
    pub lane: &'static str,
    pub outcome: &'static str,
    /// Test fn name in exit_tests.rs, or a `live:` / `n/a:` / `planned:` marker.
    pub test: &'static str,
}

pub const FINALIZED: &str = "finalized";
pub const REPORTED_LATER: &str = "finalized, reported at next record op";
pub const INTERRUPTED: &str = "interrupted, partial kept";
pub const REFUSED: &str = "refused (no recording possible)";
pub const SELF_FINALIZED: &str = "self-finalized by ffmpeg on stdin EOF";

const fn row(
    path: &'static str,
    lane: &'static str,
    outcome: &'static str,
    test: &'static str,
) -> Row {
    Row {
        path,
        lane,
        outcome,
        test,
    }
}

pub const MATRIX: &[Row] = &[
    row("record-stop", "mcp", FINALIZED, "row_record_stop_mcp"),
    row("record-stop", "daemon", FINALIZED, "row_record_stop_daemon"),
    row(
        "recorded tab closed (follow)",
        "mcp+daemon",
        "continues on the next tab, finalized at stop",
        "row_tab_close_follow",
    ),
    row(
        "recorded tab closed (per-tab)",
        "mcp+daemon",
        "that tab's file finalized, the others keep recording",
        "row_per_tab_close",
    ),
    row("stdin EOF", "mcp", REPORTED_LATER, "row_stdin_eof_mcp"),
    row("SIGTERM / SIGINT", "mcp", REPORTED_LATER, "row_signal_mcp"),
    row(
        "SIGTERM / SIGINT",
        "daemon",
        REPORTED_LATER,
        "row_signal_daemon",
    ),
    row(
        "bladebro stop",
        "daemon",
        REPORTED_LATER,
        "row_bladebro_stop_daemon",
    ),
    row("idle timeout", "mcp", REPORTED_LATER, "row_idle_mcp"),
    row("idle timeout", "daemon", REPORTED_LATER, "row_idle_daemon"),
    row(
        "Chrome relaunch (drift / crash / Closed)",
        "mcp",
        REPORTED_LATER,
        "row_relaunch_mcp",
    ),
    row(
        "Chrome relaunch (drift / crash / Closed)",
        "daemon",
        REPORTED_LATER,
        "row_relaunch_daemon",
    ),
    row(
        "Chrome relaunch, recorder connection already lost",
        "daemon",
        INTERRUPTED,
        "row_relaunch_daemon_dead_source",
    ),
    row(
        "one-shot CLI exit",
        "oneshot",
        REFUSED,
        "row_oneshot_refused",
    ),
    row(
        "kill -9 of bladebro",
        "mcp+daemon",
        SELF_FINALIZED,
        "live: exit_live.sh kill9-parent",
    ),
    row(
        "ffmpeg killed mid-recording",
        "mcp+daemon",
        INTERRUPTED,
        "row_kill_ffmpeg",
    ),
    row(
        "Chrome killed mid-recording",
        "mcp+daemon",
        INTERRUPTED,
        "planned: 09-02",
    ),
    row(
        "disk low / max duration / max size",
        "mcp+daemon",
        "stopped, finalized",
        "planned: 09-03",
    ),
    row(
        "Windows console close / logoff",
        "mcp+daemon",
        "not handled",
        "n/a: not executed; reasoning in exit.rs",
    ),
];
