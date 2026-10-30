//! The recordings-dir listing helpers count videos only. Sidecars (poster
//! `.png`, timeline `.json`) sit next to the video after record-stop, and a
//! count or a "the file" lookup must never pick them up.

use super::machine_tests::files_in;
use crate::record::encoder::test_util::Scratch;

#[test]
fn files_in_lists_videos_only() {
    let s = Scratch::new();
    let dir = s.0.join("recordings");
    std::fs::create_dir_all(&dir).expect("mkdir");
    for n in ["a.webm", "a.poster.png", "a.timeline.json", "b.mp4"] {
        std::fs::write(dir.join(n), b"x").expect("write");
    }
    // read_dir order is unspecified: compare sorted.
    let mut names: Vec<String> = files_in(&dir)
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    assert_eq!(names, ["a.webm", "b.mp4"], "sidecars must not be listed");
}
