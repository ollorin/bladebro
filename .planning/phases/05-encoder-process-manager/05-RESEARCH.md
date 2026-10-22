# Phase 5 Research: Windows process lifecycle (short pass, knowledge-based, NOT executed on Windows)

Scope: only the gap flagged in STATE.md ("Windows process lifecycle: no SIGINT, stdin-EOF finalize, no console"). Everything below is from
documentation/knowledge of tokio 1.x and Win32, not observed here. Confidence is stated per item.

| Question | Answer | Confidence |
|---|---|---|
| Graceful stop without SIGINT | ffmpeg finishes cleanly on stdin EOF for `-i pipe:0` inputs: the demuxer sees EOF, flushes encoders, writes the trailer. Same on all OSes, so stdin-EOF is the single graceful path. Phase 2 verified it on macOS only. | high (mechanism), Windows unobserved |
| How to deliver EOF | Drop/shutdown the `tokio::process::ChildStdin`; dropping closes the pipe handle, the child reads EOF. Parent must not hold a second handle (no `try_clone`, no inherited duplicate). Rust std marks pipe handles non-inheritable and passes only the redirected std handles. | medium-high |
| No console window | `tokio::process::Command::creation_flags(0x0800_0000)` (CREATE_NO_WINDOW) under `#[cfg(windows)]`; tokio exposes this as an inherent method on Windows (no `CommandExt` import needed for the tokio type; std needs `std::os::windows::process::CommandExt`, as Phase 4 probe.rs already does). | medium: verify method availability from the tokio 1.53 source in ~/.cargo/registry |
| Kill last resort | `Child::kill()` = TerminateProcess (unconditional, no trailer, file is not trusted). Always follow with `wait()`. | high |
| kill_on_drop on Windows | Supported by tokio (it kills via TerminateProcess on drop of the Child if not reaped). Backstop only. No job object: if Bladebro is hard-killed, ffmpeg gets stdin EOF anyway because the pipe closes, so it self-terminates after finalizing; no job object needed here (orphan policy is Phase 9). | medium |
| Fake encoder for tests | A `.cmd`/`.bat` file can be spawned directly via `CreateProcess` when the path ends in `.bat`/`.cmd` (it runs via cmd.exe implicitly; Rust applies extra arg escaping rules for batch files since 1.77). A batch fake cannot reliably `read stdin until EOF`; `more >nul` or `findstr "^"` drain stdin and exit on EOF. Honest uncertainty: these tests are cfg(windows), written but never compiled or run here. Prefer using PowerShell via `powershell -NoProfile -File` only if a .cmd shim is insufficient; the hang/ignore-EOF fake is the least certain. | low-medium |
| Path edge cases | Output path with spaces is fine (argv array, Rust quotes). A path starting with `-` is rejected by our check before spawn; on Windows also reject paths that begin with `-` after any `\\?\` strip is irrelevant. Drive-letter paths like `C:\x` do not start with `-`. | high |

Open uncertainty carried into the plan: no Windows run is possible in this environment, so criterion 4 ("verified in windows-latest CI") is
reportable only as "written, not executed".
