//! `Source::targets` over the fake CDP endpoint.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use super::super::fake_cdp::FakeOpts;
use super::super::report::SourceError;
use super::*;

#[tokio::test]
async fn targets_reads_rows_on_the_sources_own_connection() {
    guard(async {
        let fake = fake_with(FakeOpts {
            replies: vec![(
                "Target.getTargets",
                json!({"targetInfos": [
                    {"targetId": "P", "type": "page", "url": "http://x/", "openerId": "A"},
                    {"targetId": "S", "type": "service_worker", "url": "http://x/sw.js"}
                ]}),
            )],
            ..FakeOpts::default()
        })
        .await;
        let src = start(&fake, 10, Arc::new(CountingSink::default())).await;
        let rows = src.targets(Duration::from_secs(1)).await.expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].opener.as_deref(), Some("A"));
        let sent = src.stats().methods_sent;
        assert!(sent.contains(&("Target.getTargets", 1)), "{sent:?}");
        assert_eq!(
            fake.methods()
                .iter()
                .filter(|m| *m == "Target.getTargets")
                .count(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn targets_error_is_an_err_and_the_source_keeps_streaming() {
    guard(async {
        let fake = fake_with(FakeOpts {
            fail: Some(("Target.getTargets", "denied")),
            frames: 20,
            interval: Duration::from_millis(5),
            ..FakeOpts::default()
        })
        .await;
        let sink = Arc::new(CountingSink::default());
        let src = start(&fake, 100, sink.clone()).await;
        let err = src.targets(Duration::from_secs(1)).await.unwrap_err();
        assert!(matches!(err, SourceError::Cdp { .. }), "{err}");
        fake.wait_emitter_done().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(sink.n.load(std::sync::atomic::Ordering::SeqCst) > 0);
        assert!(!src.is_finished());
    })
    .await;
}
