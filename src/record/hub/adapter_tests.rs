//! Adapter tests: CLI parse for the two ops, lane mapping, error mapping and
//! the try_handle pre-lock validation. No Chrome, no global hub state.

use serde_json::json;

use super::env::Lane;
use super::live_env::lane_for;
use super::{map_err, HubError};
use crate::error::BladeError;

#[test]
fn errors_map_to_usage_and_other_never_closed_or_transport() {
    assert!(matches!(
        map_err(HubError::Usage("u".into())),
        BladeError::Usage(_)
    ));
    assert!(matches!(
        map_err(HubError::Failed("f".into())),
        BladeError::Other(_)
    ));
}

#[test]
fn lane_mapping() {
    assert_eq!(lane_for(true, true, true, "pipe"), Lane::Real);
    assert_eq!(lane_for(false, true, true, "pipe"), Lane::Ephemeral);
    assert_eq!(lane_for(false, false, true, "127.0.0.1:9"), Lane::Pipe);
    assert_eq!(lane_for(false, false, false, "pipe"), Lane::Pipe);
    assert_eq!(lane_for(false, false, false, "127.0.0.1:9"), Lane::Ws);
}

#[test]
fn bad_name_is_refused_before_the_global_is_touched() {
    // try_handle needs a Page for the success path; the name check comes
    // first, so these never reach the env or the lock.
    assert!(super::check_name(&json!({"op":"record-start"})).is_err());
    assert!(super::check_name(&json!({"op":"record-start","name":5})).is_err());
    assert!(super::check_name(&json!({"op":"record-start","name":"x"})).is_ok());
    assert!(!super::initialized());
}
