//! Track D1.5: the rust-born `.tflite` compiles through the fork's `#[model]`
//! macro and predicts — the writer's acceptance test. The fixture is
//! committed (regenerate with `cargo run -p exporter --bin gen_dummy`).
//!
//! This is also a live test of the parser's generality (§2.1): the file is
//! the minimal six-operator graph — rank-3 input, rank-4 FC input — with no
//! EXPAND_DIMS/RESHAPE/Flatten wrappers.

use microflow::model;
use nalgebra::SMatrix;

#[model("ml/models/model_dummy_rust.tflite")]
struct DummyRust;

#[test]
fn model_macro_accepts_the_rust_born_file() {
    let window: SMatrix<f32, 128, 1> = SMatrix::from_fn(|t, _| {
        let ts = t as f32 / 1600.0;
        (2.0 * core::f32::consts::PI * 50.0 * ts).sin()
    });
    let output = DummyRust::predict(window);
    let sum: f32 = output.iter().sum();
    assert!((sum - 1.0).abs() < 0.02, "softmax must sum to 1, got {sum}");
    for p in output.iter() {
        assert!((0.0..=1.0).contains(p), "probability out of range: {p}");
    }
}

#[test]
fn model_macro_accepts_the_rust_born_file_quantized() {
    let input: SMatrix<i8, 128, 1> = SMatrix::from_element(0);
    let output = DummyRust::predict_quantized(input);
    let sum: f32 = output.iter().sum();
    assert!((sum - 1.0).abs() < 0.02, "softmax must sum to 1, got {sum}");
}

/// The microflow kernel vs the interp reference on the same file: the
/// quantized pipelines must agree within ±2 quanta per output element
/// (the D3.3 agreement — different rounding modes and operation orders).
#[test]
fn microflow_and_interp_agree_within_two_quanta() {
    use exporter::interp::InterpModel;

    let bytes = std::fs::read("../../ml/models/model_dummy_rust.tflite")
        .expect("the committed dummy fixture");
    let interp = InterpModel::from_bytes(bytes).unwrap();
    let window: Vec<i8> = (0..128).map(|t| (t % 51 - 25) as i8).collect();
    let input: SMatrix<i8, 128, 1> = SMatrix::from_fn(|t, _| window[t]);
    let output = DummyRust::predict_quantized(input);

    let out_scale = 1.0f32 / 256.0;
    let out_zp = -128i8;
    let quantized: Vec<i8> = output
        .iter()
        .map(|v| (v / out_scale).round() as i32 + out_zp as i32)
        .map(|v| v.clamp(-128, 127) as i8)
        .collect();
    let reference = interp.run_quantized(&window).unwrap();
    for (k, (got, expected)) in quantized.iter().zip(reference.quantized_output).enumerate() {
        let diff = (*got as i32 - expected as i32).abs();
        assert!(
            diff <= 2,
            "output {k}: microflow {got} vs interp {expected} (diff {diff} > 2 quanta)"
        );
    }
}
