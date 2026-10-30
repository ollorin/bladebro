//! Disk guard pieces: `df` parsing table and the real probe.

use super::disk::{free_bytes, is_low, mib, parse_df, DISK_LOW_MIB};

const MAC: &str = "Filesystem   1024-blocks      Used Available Capacity iused      ifree %iused  Mounted on\n/dev/disk3s5  971350180 500000000 123456789    81% 1234567 1234567890    0%   /System/Volumes/Data\n";
const LINUX: &str = "Filesystem     1K-blocks     Used Available Use% Mounted on\n/dev/sda1       98298500 40000000  53333333  43% /\n";
const WRAPPED: &str = "Filesystem     1K-blocks     Used Available Use% Mounted on\n/dev/mapper/very-long-volume-group-name-root\n                98298500 40000000  53333333  43% /\n";
const SPACES: &str = "Filesystem     1K-blocks     Used Available Use% Mounted on\nmy share name   100 50  50  50% /mnt/x\n";

#[test]
fn parse_df_table() {
    assert_eq!(parse_df(MAC), Some(123_456_789 * 1024));
    assert_eq!(parse_df(LINUX), Some(53_333_333 * 1024));
    assert_eq!(parse_df(WRAPPED), Some(53_333_333 * 1024));
    assert_eq!(parse_df(SPACES), Some(50 * 1024));
    assert_eq!(parse_df(""), None);
    assert_eq!(parse_df("garbage line without percent\n"), None);
    assert_eq!(parse_df("50% only\n"), None);
    assert_eq!(parse_df("fs 1 2 notanumber 50% /\n"), None);
}

#[cfg(unix)]
#[test]
fn real_probe_answers_for_existing_and_missing_dirs() {
    let t = std::env::temp_dir();
    assert!(free_bytes(&t).is_some_and(|b| b > 0));
    // A recordings dir not created yet is measured on its parent.
    assert!(free_bytes(&t.join("not-yet-created-09-03/sub")).is_some());
}

#[test]
fn threshold_is_strict_and_unknown_is_never_low() {
    assert!(is_low(Some((DISK_LOW_MIB - 1) << 20)));
    assert!(!is_low(Some(DISK_LOW_MIB << 20)));
    assert!(!is_low(None));
    assert_eq!(mib(200 << 20), 200);
}

#[test]
fn no_updater_dependency() {
    let src = include_str!("disk.rs");
    assert!(!src.contains("use crate::updater"));
    assert!(!src.contains("updater::"));
}
