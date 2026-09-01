//! Track D1.4: the writer roundtrip — build → serialize → parse through the
//! vendored reader (the same code the `#[model]` macro compiles) → dump = the
//! expected six operators with the expected shapes and quantization.

use exporter::dumper;
use exporter::graph::{GraphBuilder, TensorQuant};
use exporter::writer;

fn toy_graph() -> exporter::graph::ModelGraph {
    let mut b = GraphBuilder::new();
    b.add_input([1, 128, 1], TensorQuant::per_tensor(0.0128, 1))
        .unwrap()
        .add_conv_1d(
            8,
            3,
            vec![7; 8 * 3],
            vec![0.02; 8],
            vec![10; 8],
            vec![0.000256; 8],
            TensorQuant::per_tensor(0.004, -128),
            true,
        )
        .unwrap()
        .add_avg_pool(2, TensorQuant::per_tensor(0.004, -128))
        .unwrap()
        .add_conv_1d(
            16,
            3,
            vec![-7; 16 * 3 * 8],
            vec![0.01; 16],
            vec![-20; 16],
            vec![0.00004; 16],
            TensorQuant::per_tensor(0.003, -128),
            true,
        )
        .unwrap()
        .add_avg_pool(2, TensorQuant::per_tensor(0.003, -128))
        .unwrap()
        .add_fc(
            4,
            vec![5; 4 * 480],
            vec![0.05; 4],
            vec![100; 4],
            vec![0.00015; 4],
            TensorQuant::per_tensor(0.0024, -40),
        )
        .unwrap()
        .add_softmax(TensorQuant::per_tensor(1.0 / 256.0, -128))
        .unwrap();
    b.build().unwrap()
}

#[test]
fn roundtrip_dumps_the_six_real_operators() {
    let bytes = writer::write(&toy_graph());
    let dump = dumper::dump_bytes(&bytes).unwrap();

    // The six real operators, in order, with the TF-converted file's shapes.
    for expected in [
        "op[0] CONV_2D",
        "op[1] AVERAGE_POOL_2D",
        "op[2] CONV_2D",
        "op[3] AVERAGE_POOL_2D",
        "op[4] FULLY_CONNECTED",
        "op[5] SOFTMAX",
    ] {
        assert!(dump.contains(expected), "missing {expected} in:\n{dump}");
    }
    // No wrapper operators (the plan's minimal-graph contract).
    assert!(!dump.contains("EXPAND_DIMS"));
    assert!(!dump.contains("RESHAPE"));
    assert!(!dump.contains("SHAPE"));
    // Shapes and quantization survive the roundtrip.
    assert!(dump.contains("shape=[1, 128, 1] dtype=INT8, scale=0.0128, zp=1"));
    assert!(dump.contains("shape=[1, 1, 126, 8]"));
    assert!(dump.contains("shape=[1, 1, 63, 8]"));
    assert!(dump.contains("shape=[1, 1, 61, 16]"));
    assert!(dump.contains("shape=[1, 1, 30, 16]"));
    assert!(dump.contains("shape=[4, 480]"));
    assert!(dump.contains("scale=0.00390625, zp=-128"));
    // Per-channel weight scales: F entries on the conv/FC weights.
    assert!(dump.contains("scale=[8 values"));
    assert!(dump.contains("scale=[16 values"));
    assert!(dump.contains("scale=[4 values"));
}

#[test]
fn writer_output_is_deterministic() {
    let a = writer::write(&toy_graph());
    let b = writer::write(&toy_graph());
    assert_eq!(a, b, "identical graphs must serialize to identical bytes");
}

#[test]
fn committed_dummy_file_roundtrips() {
    // The committed fixture must stay parseable and keep the structure
    // (regenerable via `cargo run -p exporter --bin gen_dummy`). Test binaries
    // run with the package dir as cwd — hence the ../../ prefix.
    let bytes = std::fs::read("../../ml/models/model_dummy_rust.tflite")
        .expect("run `cargo run -p exporter --bin gen_dummy` from the repo root");
    let dump = dumper::dump_bytes(&bytes).unwrap();
    assert!(dump.contains("op[5] SOFTMAX"), "{dump}");
    let fresh = std::fs::read("../../ml/models/model_dummy_rust.tflite").unwrap();
    assert_eq!(bytes, fresh);
}
