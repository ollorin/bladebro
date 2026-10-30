//! Recording file names: caller name -> safe stem, plus UTC stamp and short id.
//!
//! A caller-supplied name is untrusted text that ends up in a path and in an
//! ffmpeg argv, so it is sanitized, never rejected (D3-7): the agent always
//! gets a file and the result path shows the real name.

use super::values::Format;

/// Longest stem in bytes (ASCII by then); keeps the full path inside the
/// Windows MAX_PATH budget together with the stamp, id and extension.
pub const MAX_STEM: usize = 80;
/// Stem used when nothing usable is left of the caller's name.
pub const FALLBACK_STEM: &str = "recording";

fn is_trim(c: char) -> bool {
    matches!(c, '-' | '.' | '_')
}

/// Windows device names, compared case-insensitively on the part before the
/// first '.'. Applied on every OS so a name means the same file everywhere
/// (D3-8).
fn is_reserved(base: &str) -> bool {
    let b = base.split('.').next().unwrap_or("").to_ascii_lowercase();
    if matches!(b.as_str(), "con" | "prn" | "aux" | "nul") {
        return true;
    }
    for p in ["com", "lpt"] {
        if let Some(d) = b.strip_prefix(p) {
            if d.len() == 1 && matches!(d.as_bytes()[0], b'1'..=b'9') {
                return true;
            }
        }
    }
    false
}

/// Sanitize a caller name into a stem matching
/// `^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$` that is not a Windows device name.
///
/// Steps, in order, and why:
/// 1. every char outside `[A-Za-z0-9._-]` becomes `_` (separators, drive
///    colons, NUL, shell metacharacters, non-ASCII: no traversal, no UNC);
/// 2. runs of `_` collapse (readability);
/// 3. leading `-._` trimmed (ffmpeg option injection via leading '-', hidden
///    files via '.'), trailing `-._` trimmed (Windows strips trailing dots);
/// 4. cap at [`MAX_STEM`] and trim again (the cut can expose a new tail);
/// 5. empty -> [`FALLBACK_STEM`];
/// 6. reserved device name -> prefix `rec_`, then cap and trim once more.
///
/// "a..b" is kept: no separator survives step 1, so ".." inside a basename
/// cannot traverse.
pub fn sanitize_stem(raw: &str) -> String {
    let mut mapped = String::with_capacity(raw.len());
    for c in raw.chars() {
        let c = if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
            c
        } else {
            '_'
        };
        if c == '_' && mapped.ends_with('_') {
            continue;
        }
        mapped.push(c);
    }
    let mut s = mapped.trim_matches(is_trim).to_string();
    s.truncate(MAX_STEM);
    s = s.trim_matches(is_trim).to_string();
    if s.is_empty() {
        s = FALLBACK_STEM.to_string();
    }
    if is_reserved(&s) {
        s = format!("rec_{s}");
        s.truncate(MAX_STEM);
        s = s.trim_matches(is_trim).to_string();
    }
    s
}

/// Stem of the per-tab file for the `n`-th tab seen (first-visit order from 1):
/// the sanitized stem cut so `-tab<n>` still fits inside [`MAX_STEM`]. The
/// sanitized stem is ASCII, so the cut is always on a char boundary.
pub fn tab_stem(stem: &str, n: u32) -> String {
    let suffix = format!("-tab{n}");
    let mut s = sanitize_stem(stem);
    s.truncate(MAX_STEM.saturating_sub(suffix.len()));
    let s = s.trim_end_matches(is_trim);
    format!("{s}{suffix}")
}

/// `YYYYMMDDTHHMMSSZ` for a Unix time. Integer math only; valid for any
/// value reachable from `SystemTime::now()` (years up to 9999).
/// Civil-from-days after Howard Hinnant (public domain).
pub fn utc_stamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Six lowercase hex chars mixing clock, pid and a per-process sequence.
/// For fixed nanos/pid distinct `seq` give distinct ids: multiplying by an
/// odd constant is a bijection on the low 24 bits, and xor with a constant
/// keeps it one.
pub fn short_id(nanos: u32, pid: u32, seq: u32) -> String {
    format!(
        "{:06x}",
        (nanos ^ pid.rotate_left(13) ^ seq.wrapping_mul(0x9E37_79B9)) & 0xff_ffff
    )
}

/// `<stem>-<stamp>-<id>.<ext>`. Callers pass a stem from [`sanitize_stem`].
pub fn file_name(stem: &str, format: Format, unix_secs: u64, id: &str) -> String {
    format!("{stem}-{}-{id}.{}", utc_stamp(unix_secs), format.ext())
}

#[cfg(test)]
mod tests {
    #[test]
    fn tab_stem_table() {
        use super::{sanitize_stem, tab_stem, MAX_STEM};
        assert_eq!(
            tab_stem("login flow", 2),
            format!("{}-tab2", sanitize_stem("login flow"))
        );
        assert_eq!(tab_stem("a", 1), "a-tab1");
        let long = "x".repeat(80);
        let t = tab_stem(&long, 3);
        assert!(t.len() <= MAX_STEM && t.ends_with("-tab3"), "{t}");
        assert_eq!(t.len(), MAX_STEM);
        // A cut that would expose a trailing separator is trimmed.
        let sep = format!("{}-{}", "y".repeat(74), "z".repeat(20));
        let t = tab_stem(&sep, 1);
        assert!(!t.contains("--") && t.ends_with("y-tab1"), "{t}");
        // Multibyte input is sanitized to ASCII before the cut (never splits a char).
        let mb = "é".repeat(60) + &"a".repeat(60);
        let t = tab_stem(&mb, 12);
        assert!(
            t.is_ascii() && t.len() <= MAX_STEM && t.ends_with("-tab12"),
            "{t}"
        );
        assert_eq!(tab_stem("", 1), "recording-tab1");
    }

    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

    const TABLE: &[(&str, &str)] = &[
        ("", "recording"),
        ("my test", "my_test"),
        ("ok-name_1.2", "ok-name_1.2"),
        ("../../etc/passwd", "etc_passwd"),
        ("..\\..\\Windows\\System32", "Windows_System32"),
        ("C:\\Users\\x", "C_Users_x"),
        ("C:foo", "C_foo"),
        ("\\\\server\\share\\rec", "server_share_rec"),
        ("\\\\?\\C:\\x", "C_x"),
        ("/abs/path/name", "abs_path_name"),
        ("a/b", "a_b"),
        ("-rf", "rf"),
        ("--help", "help"),
        (".hidden", "hidden"),
        ("...", "recording"),
        ("a\0b", "a_b"),
        ("name. . .", "name"),
        ("trailing.   ", "trailing"),
        ("$(rm -rf)", "rm_-rf"),
        ("%2e%2e%2f", "2e_2e_2f"),
        ("café", "caf"),
        ("отчёт", "recording"),
        ("con", "rec_con"),
        ("CON.webm", "rec_CON.webm"),
        ("Aux.tar.gz", "rec_Aux.tar.gz"),
        ("lpt9", "rec_lpt9"),
        ("nul", "rec_nul"),
        ("com1", "rec_com1"),
        ("console", "console"),
        ("com10", "com10"),
        ("com0", "com0"),
    ];

    fn valid(s: &str) -> bool {
        let mut it = s.chars();
        let first_ok = it.next().is_some_and(|c| c.is_ascii_alphanumeric());
        first_ok
            && s.len() <= MAX_STEM
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            && !is_reserved(s)
    }

    #[test]
    fn sanitize_stem_table() {
        for (input, expected) in TABLE {
            assert_eq!(sanitize_stem(input), *expected, "input {input:?}");
        }
    }

    #[test]
    fn sanitize_stem_caps_and_retrims() {
        assert_eq!(sanitize_stem(&"x".repeat(200)).len(), 80);
        let s = sanitize_stem(&format!("{}..b", "a".repeat(79)));
        assert_eq!(s, "a".repeat(79));
        assert!(!s.ends_with('.'));
    }

    #[test]
    fn sanitize_stem_invariant() {
        let mut inputs: Vec<String> = TABLE.iter().map(|(i, _)| i.to_string()).collect();
        inputs.push("x".repeat(200));
        inputs.push(format!("{}..b", "a".repeat(79)));
        inputs.push(format!("con.{}", "z".repeat(80)));
        inputs.push(format!("con{}", ".".repeat(100)));
        for i in &inputs {
            let out = sanitize_stem(i);
            assert!(valid(&out), "input {i:?} gave invalid stem {out:?}");
        }
    }

    #[test]
    fn utc_stamp_known_epochs() {
        assert_eq!(utc_stamp(0), "19700101T000000Z");
        assert_eq!(utc_stamp(951_782_400), "20000229T000000Z");
        assert_eq!(utc_stamp(1_700_000_000), "20231114T221320Z");
        assert_eq!(utc_stamp(4_102_444_799), "20991231T235959Z");
    }

    #[test]
    fn short_id_shape_and_distinct() {
        let mut set = HashSet::new();
        for s in 0..1000 {
            let id = short_id(123_456_789, 4242, s);
            assert_eq!(id.len(), 6);
            assert!(id
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
            set.insert(id);
        }
        assert_eq!(set.len(), 1000);
    }

    #[test]
    fn file_name_shape() {
        assert_eq!(
            file_name("demo", Format::WebM, 1_700_000_000, "0a1b2c"),
            "demo-20231114T221320Z-0a1b2c.webm"
        );
        assert_eq!(
            file_name("demo", Format::Mp4, 1_700_000_000, "0a1b2c"),
            "demo-20231114T221320Z-0a1b2c.mp4"
        );
        let n = file_name(&sanitize_stem("../x:y"), Format::WebM, 5, "000000");
        assert!(!n.contains(['/', '\\', ':']));
        assert_eq!(
            Path::new(&n).file_name().and_then(|f| f.to_str()),
            Some(n.as_str())
        );
    }
}
