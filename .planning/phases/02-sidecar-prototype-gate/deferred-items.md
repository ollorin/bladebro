# Deferred items: phase 02 (sidecar prototype gate)

Found during 02-02 execution, not fixed there (scope: measure, do not change the recorder under test).
Each item needs a decision in 02-03 or a later plan.

- [DEFERRED] wallclock output carries non-monotonic dts warnings at ~100 fps capture, so the strict
  "full decode with no errors" check fails on every live wallclock file (SC1: 65 lines in the 32 s
  tab-switch run; core fps_default 47, acks_60s 54; live 30 s runs 36 vp9 / 45 vp8 / 9 x264, final run).
  Decode exit is 0 and every frame is read. The warning shape ("217 >= 217") points at equal
  millisecond stamps from the 1 ms input timebase. Fix candidates: a finer input timebase, or
  stamping in the writer. Not tried in 02-02 (would change the recorder under measurement).
  Found during 02-02 Task 2 / Task 3 / Task 4.
- [DEFERRED] screencast silence: in 1 of 7 recordings of 60 s or more on anim.html the screencast
  delivered frames for 23 s and then none for 37 s, with 0 ack errors and the sidecar still attached
  (2246 frames in 60 s). That run's metrics were overwritten when core.sh was rerun, so the
  recorded evidence is this entry and the 02-02 SUMMARY, not a result file. Two later 60 s probe
  runs with a page-counter sample every 5 s did not reproduce it (results/stall_probe.json). The
  cause is not established. Found during 02-02 Task 3.
- [DEFERRED] background and minimized targets stop delivering frames: a recorded page that is
  switched away from delivers 0 frames for the rest of the run (16 s observed, headed and headless);
  a minimized window delivers ~0.3 fps and resumes within about 1 s of restore. The tail hold keeps
  the duration right, but the content is frozen. Affects the 02-03 product-behavior verdict. Found
  during 02-02 Task 4.
