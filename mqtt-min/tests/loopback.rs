//! End-to-end tests of the client against the in-process loopback broker.

use mqtt_min::testing::LoopbackBroker;

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

    let first = broker
        .publishes
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("first publish captured");
    assert_eq!(first.topic, "oee/line1/a/status");
    assert_eq!(first.payload, r#"{"state":"run","t_ms":1250}"#);
    let second = broker
        .publishes
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("second publish captured");
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
