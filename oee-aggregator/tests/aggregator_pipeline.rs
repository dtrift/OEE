//! The aggregator runtime end-to-end over the loopback broker (D2): fake
//! node publishers (hand-made payloads, the exact node shapes) feed the
//! real `aggregator::run` over the real wire path; the dashboard contract
//! (`oee/line1/oee`) is consumed by a real subscribed client.
//!
//! The full simulator->nodes->aggregator loop lives in tests/experiment.rs
//! (it needs the `nodes` dev-dependency); this file pins the runtime's MQTT
//! behavior in isolation.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use mqtt_min::testing::LoopbackBroker;
use mqtt_min::Client;
use oee_aggregator::aggregator::{self, Config};
use oee_aggregator::payload::{str_field, u32_field};

/// Reads one `oee/line1/oee` payload within 5 s (other topics skipped).
fn next_oee(subscriber: &mut Client) -> String {
    loop {
        let message = subscriber
            .next_message(Duration::from_secs(5))
            .expect("read")
            .expect("a message within the timeout");
        if message.topic == "oee/line1/oee" {
            return message.payload;
        }
    }
}

#[test]
fn aggregator_subscribes_folds_publishes_and_flushes_on_end_markers() {
    let broker = LoopbackBroker::spawn();

    // The dashboard stand-in subscribes to everything first.
    let mut dashboard = Client::connect(&broker.addr, "dashboard-test", 60).unwrap();
    dashboard.subscribe("oee/line1/#").unwrap();

    // The aggregator signals readiness after its SUBACKs — the nodes start
    // only then (QoS 0 does not replay messages published before the
    // subscription was in place).
    let (ready_tx, ready_rx) = mpsc::channel();
    let config = Config {
        broker_addr: broker.addr.clone(),
        ideal_cycle_ms: 400,
        minute_ms: 60_000,
        expect_nodes: vec!["a".into(), "p".into(), "q".into()],
        csv_path: None,
        ready: Some(ready_tx),
        ..Config::default()
    };
    let aggregator_thread = thread::spawn(move || aggregator::run(&config));
    ready_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("aggregator subscribed");

    // The three nodes: statuses, counts, verdicts, end markers — the exact
    // payload shapes pinned by mqtt_sink.
    let addr = broker.addr.clone();
    thread::spawn(move || {
        let mut node = Client::connect(&addr, "node-a", 60).unwrap();
        node.publish(
            "oee/line1/a/status",
            r#"{"state":"run","t_ms":1000,"run_id":"e2e"}"#,
        )
        .unwrap();
        node.publish(
            "oee/line1/a/status",
            r#"{"state":"idle","t_ms":61000,"run_id":"e2e"}"#,
        )
        .unwrap();
        node.publish("oee/line1/a/end", r#"{"t_ms":61999,"run_id":"e2e"}"#)
            .unwrap();
    });
    let addr = broker.addr.clone();
    thread::spawn(move || {
        let mut node = Client::connect(&addr, "node-p", 60).unwrap();
        for (count, t_ms) in [(1, 5_000u32), (2, 15_000), (3, 25_000), (4, 50_000)] {
            node.publish(
                "oee/line1/p/count",
                &format!(r#"{{"count":{count},"t_ms":{t_ms},"run_id":"e2e"}}"#),
            )
            .unwrap();
        }
        node.publish("oee/line1/p/end", r#"{"t_ms":58000,"run_id":"e2e"}"#)
            .unwrap();
    });
    let addr = broker.addr.clone();
    thread::spawn(move || {
        let mut node = Client::connect(&addr, "node-q", 60).unwrap();
        node.publish(
            "oee/line1/q/verdict",
            r#"{"verdict":"good","t_ms":5000,"run_id":"e2e"}"#,
        )
        .unwrap();
        node.publish(
            "oee/line1/q/verdict",
            r#"{"verdict":"cracked","t_ms":15000,"run_id":"e2e"}"#,
        )
        .unwrap();
        node.publish(
            "oee/line1/q/verdict",
            r#"{"verdict":"good","t_ms":25000,"run_id":"e2e"}"#,
        )
        .unwrap();
        node.publish("oee/line1/q/end", r#"{"t_ms":59000,"run_id":"e2e"}"#)
            .unwrap();
    });

    // The dashboard sees live shift snapshots as the run streams, and the
    // final cumulative row once all end markers flush the fold.
    let mut saw_snapshot = false;
    let mut final_payload = String::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let payload = next_oee(&mut dashboard);
        if str_field(&payload, "scope") == Some("shift")
            && u32_field(&payload, "t_to_ms") == Some(61_999)
        {
            final_payload = payload;
            break;
        }
        saw_snapshot = true;
    }
    assert!(
        !final_payload.is_empty(),
        "the final shift payload must arrive (saw earlier snapshots: {saw_snapshot})"
    );

    // The final row over [0, 61999): run 60 s (1000..61000), 4 parts,
    // 2 good of 3 verdicts.
    assert_eq!(u32_field(&final_payload, "run_ms"), Some(60_000));
    assert_eq!(u32_field(&final_payload, "parts"), Some(4));
    assert_eq!(u32_field(&final_payload, "good"), Some(2));
    assert_eq!(u32_field(&final_payload, "total"), Some(3));

    let summary = aggregator_thread
        .join()
        .expect("aggregator thread")
        .expect("aggregator run");
    assert_eq!(summary.parse_errors, 0);
    assert_eq!(summary.messages, 2 + 4 + 3 + 3); // statuses + counts + verdicts + ends
    assert_eq!(summary.windows, 1, "one (partial) minute window");
    let shift = summary.final_shift.expect("final shift row");
    assert_eq!(shift.parts, 4);
    assert_eq!(shift.t_to_ms, 61_999);
}
