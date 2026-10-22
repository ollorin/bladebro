//! Start/stop/drop, start failures and a lost connection.

use std::sync::Arc;
use std::time::Duration;

use super::super::capture::{Endpoint, Source};
use super::super::fake_cdp::FakeOpts;
use super::super::report::{SourceError, SourceOutcome};
use super::*;

#[tokio::test]
async fn stop_closes_the_socket_and_releases_the_sink() {
    guard(async {
        let fake = fake_with(FakeOpts::default()).await;
        let sink = Arc::new(CountingSink::default());
        let src = start(&fake, 10, sink.clone()).await;
        let rep = src.stop().await;
        // Zero frames is never a success (CAPT-05), but stop still completes.
        assert!(matches!(rep.outcome, SourceOutcome::NoFrames { .. }));
        tokio::time::timeout(Duration::from_secs(1), fake.closed())
            .await
            .expect("ws closed after stop");
        assert_eq!(Arc::strong_count(&sink), 1);
    })
    .await;
}

#[tokio::test]
async fn dropping_without_stop_closes_the_socket() {
    guard(async {
        let fake = fake_with(FakeOpts::default()).await;
        let src = start(&fake, 10, Arc::new(CountingSink::default())).await;
        drop(src);
        tokio::time::timeout(Duration::from_secs(1), fake.closed())
            .await
            .expect("ws closed after drop");
    })
    .await;
}

#[tokio::test]
async fn page_enable_failure_is_a_cdp_error_and_start_is_never_sent() {
    guard(async {
        let fake = fake_with(FakeOpts {
            fail: Some(("Page.enable", "Not attached to an active page")),
            ..Default::default()
        })
        .await;
        let err = Source::start_ws(&fake.url, "T1", cfg(10), Arc::new(CountingSink::default()))
            .await
            .err()
            .expect("must fail");
        match &err {
            SourceError::Cdp { method, error } => {
                assert_eq!(*method, "Page.enable");
                assert!(error.contains("Not attached"));
            }
            other => panic!("{other:?}"),
        }
        assert!(err.to_string().ends_with("Browsing is unaffected."));
        assert!(!fake.methods().contains(&"Page.startScreencast".to_string()));
    })
    .await;
}

#[tokio::test]
async fn pipe_transport_is_refused_without_io() {
    let err = Source::start(
        &Endpoint::Pipe,
        "T1",
        cfg(10),
        Arc::new(CountingSink::default()),
    )
    .await
    .err()
    .expect("must fail");
    assert_eq!(err, SourceError::PipeTransport);
}

#[tokio::test]
async fn unreachable_port_is_an_error_not_a_panic() {
    guard(async {
        let ep = Endpoint::Port {
            base: "127.0.0.1:1".into(),
        };
        let err = Source::start(&ep, "T1", cfg(10), Arc::new(CountingSink::default()))
            .await
            .err()
            .expect("must fail");
        assert!(
            matches!(
                err,
                SourceError::Connect { .. } | SourceError::TargetNotFound { .. }
            ),
            "{err:?}"
        );
    })
    .await;
}

#[tokio::test]
async fn a_closed_connection_reports_connection_lost() {
    guard(async {
        let fake = fake_with(FakeOpts::default()).await;
        let src = start(&fake, 10, Arc::new(CountingSink::default())).await;
        fake.close_now();
        tokio::time::sleep(Duration::from_millis(700)).await;
        let rep = src.stop().await;
        assert!(
            matches!(rep.outcome, SourceOutcome::ConnectionLost { .. }),
            "{}",
            rep.verdict()
        );
    })
    .await;
}
