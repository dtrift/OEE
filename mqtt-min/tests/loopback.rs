//! End-to-end tests of the client against the in-process loopback broker.

use std::time::Duration;

use mqtt_min::testing::LoopbackBroker;

fn recv_soon(broker: &LoopbackBroker) -> mqtt_min::testing::CapturedPublish {
    broker
        .publishes
        .recv_timeout(Duration::from_secs(5))
        .expect("publish captured")
}

#[test]
fn client_publishes_through_the_broker() {
    let broker = LoopbackBroker::spawn();
    let mut client = mqtt_min::Client::connect(&broker.addr, "node-a-test", 60).expect("connect");
    client
        .publish("oee/line1/a/status", r#"{"state":"run","t_ms":1250}"#)
        .expect("publish");
    client.ping().expect("ping round-trip");
    client
        .publish("oee/line1/q/verdict", r#"{"verdict":"cracked"}"#)
        .expect("publish");

    let first = recv_soon(&broker);
    assert_eq!(first.topic, "oee/line1/a/status");
    assert_eq!(first.payload, r#"{"state":"run","t_ms":1250}"#);
    let second = recv_soon(&broker);
    assert_eq!(second.topic, "oee/line1/q/verdict");
}

#[test]
fn connect_to_a_dead_port_fails_cleanly() {
    // Bind and drop a listener to claim then free a port.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    drop(listener);
    let result = mqtt_min::Client::connect(&addr, "x", 60);
    assert!(result.is_err(), "a dead port must not connect");
}

/// Reads one message from a subscribed client or panics after 5 s.
fn next_soon(client: &mut mqtt_min::Client) -> mqtt_min::Message {
    client
        .next_message(Duration::from_secs(5))
        .expect("read")
        .expect("a message within the timeout")
}

#[test]
fn subscriber_receives_publishes_after_suback() {
    let broker = LoopbackBroker::spawn();
    let mut sub =
        mqtt_min::Client::connect(&broker.addr, "aggregator-test", 60).expect("subscriber connect");
    sub.subscribe("oee/line1/#").expect("subscribe");
    let mut publisher =
        mqtt_min::Client::connect(&broker.addr, "node-a-test", 60).expect("publisher connect");
    publisher
        .publish("oee/line1/a/status", r#"{"state":"run"}"#)
        .expect("publish");

    let message = next_soon(&mut sub);
    assert_eq!(message.topic, "oee/line1/a/status");
    assert_eq!(message.payload, r#"{"state":"run"}"#);
}

#[test]
fn wildcard_filters_scope_the_dispatch() {
    let broker = LoopbackBroker::spawn();
    let mut ends = mqtt_min::Client::connect(&broker.addr, "aggregator-ends", 60).expect("connect");
    ends.subscribe("oee/line1/+/end").expect("subscribe");
    let mut publisher = mqtt_min::Client::connect(&broker.addr, "node-p", 60).expect("connect");

    // A non-matching topic must not be delivered…
    publisher
        .publish("oee/line1/a/status", r#"{"state":"run"}"#)
        .expect("publish");
    // …but the node-end marker on the same prefix must be.
    publisher
        .publish("oee/line1/a/end", r#"{"t_ms":59999}"#)
        .expect("publish");
    let message = next_soon(&mut ends);
    assert_eq!(message.topic, "oee/line1/a/end");
    assert_eq!(message.payload, r#"{"t_ms":59999}"#);
    // And nothing else arrived: the next read times out cleanly.
    assert!(ends
        .next_message(Duration::from_millis(300))
        .expect("idle read")
        .is_none());
}

#[test]
fn two_subscribers_both_receive_a_publish() {
    let broker = LoopbackBroker::spawn();
    let mut dashboard =
        mqtt_min::Client::connect(&broker.addr, "dashboard", 60).expect("dashboard connect");
    dashboard.subscribe("oee/line1/#").expect("subscribe");
    let mut aggregator =
        mqtt_min::Client::connect(&broker.addr, "aggregator", 60).expect("aggregator connect");
    aggregator.subscribe("oee/line1/oee").expect("subscribe");

    // The aggregator is itself the publisher (statuses flow in, OEE flows
    // out — the real week-5 topology).
    aggregator
        .publish("oee/line1/oee", r#"{"oee":0.5}"#)
        .expect("publish");
    let for_dashboard = next_soon(&mut dashboard);
    assert_eq!(for_dashboard.topic, "oee/line1/oee");
    let for_aggregator = next_soon(&mut aggregator);
    assert_eq!(for_aggregator.payload, r#"{"oee":0.5}"#);
}

#[test]
fn subscribe_early_publishes_are_buffered_not_lost() {
    // A broker may dispatch a PUBLISH while the client still waits for its
    // SUBACK; the client must buffer it (subscribe returns after the SUBACK,
    // the message is served by the next read). The loopback broker orders
    // SUBACK first, so this pins the client's buffering path directly: the
    // publisher's message arrives while `subscribe` is between send and ack.
    let broker = LoopbackBroker::spawn();
    let mut client = mqtt_min::Client::connect(&broker.addr, "late-sub", 60).expect("connect");
    let mut publisher = mqtt_min::Client::connect(&broker.addr, "node-a", 60).expect("connect");
    // Publishing before subscribe: with clean sessions and no retention the
    // message is NOT delivered — that is the documented QoS-0 contract.
    publisher.publish("oee/line1/a/status", "{}").unwrap();
    client.subscribe("oee/line1/#").expect("subscribe");
    assert!(
        client
            .next_message(Duration::from_millis(300))
            .expect("idle read")
            .is_none(),
        "a message published before SUBACK must not be delivered"
    );
    // After the SUBACK everything flows.
    publisher.publish("oee/line1/a/status", "{}").unwrap();
    let message = next_soon(&mut client);
    assert_eq!(message.topic, "oee/line1/a/status");
}
