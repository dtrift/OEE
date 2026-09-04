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
fn pre_subscription_publishes_are_not_delivered() {
    // QoS 0 + clean session: a message published before the subscription
    // existed is NOT delivered. The capture channel is the processing
    // barrier (the broker captures after the dispatch pass): once the early
    // publish is captured, it is gone for good, and any subscribe that
    // happens after cannot retroactively deliver it. Without this sync the
    // test would race two independent TCP connections against the broker's
    // scheduler — exactly the flake the CI caught.
    let broker = LoopbackBroker::spawn();
    let mut client = mqtt_min::Client::connect(&broker.addr, "late-sub", 60).expect("connect");
    let mut publisher = mqtt_min::Client::connect(&broker.addr, "node-a", 60).expect("connect");
    publisher.publish("oee/line1/a/status", "{}").unwrap();
    let early = broker
        .publishes
        .recv_timeout(Duration::from_secs(5))
        .expect("the early publish is captured (dispatch already done)");
    assert_eq!(early.topic, "oee/line1/a/status");
    client.subscribe("oee/line1/#").expect("subscribe");
    assert!(
        client
            .next_message(Duration::from_millis(300))
            .expect("idle read")
            .is_none(),
        "a message published before the subscription must not be delivered"
    );
    // After the SUBACK everything flows.
    publisher.publish("oee/line1/a/status", "{}").unwrap();
    let message = next_soon(&mut client);
    assert_eq!(message.topic, "oee/line1/a/status");
}

#[test]
fn subscribe_buffers_publishes_that_beat_the_suback() {
    // The client-side half of the race: a broker may dispatch a matching
    // PUBLISH while the SUBACK is still in flight (our broker reaches this
    // wire order when a publisher races a fresh subscriber — registration
    // precedes the SUBACK). The client must buffer such a message while
    // waiting for the ack and serve it from the next read, not lose it and
    // not mistake it for a protocol error.
    use std::io::Write as _;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let peer = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("peer accept");
        // CONNACK, then a PUBLISH that beats the SUBACK, then the SUBACK
        // itself (packet id 1, one filter granted QoS 0).
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).unwrap();
        stream
            .write_all(&mqtt_min::encode_publish("oee/line1/oee", r#"{"oee":0.5}"#))
            .unwrap();
        stream.write_all(&[0x90, 0x03, 0x00, 0x01, 0x00]).unwrap();
        stream.flush().unwrap();
        // Hold the connection open until the client is done reading.
        std::thread::sleep(Duration::from_millis(300));
    });
    let mut client = mqtt_min::Client::connect(&addr, "race-sub", 60).expect("connect");
    client.subscribe("oee/line1/#").expect("subscribe");
    let message = next_soon(&mut client);
    assert_eq!(message.topic, "oee/line1/oee");
    assert_eq!(message.payload, r#"{"oee":0.5}"#);
    peer.join().expect("peer done");
}
