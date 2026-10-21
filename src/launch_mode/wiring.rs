//! Hook checks: every upstream edit of this module must stay in place.

#[test]
fn lib_declares_module() {
    assert!(include_str!("../lib.rs").contains("pub mod launch_mode;"));
}
