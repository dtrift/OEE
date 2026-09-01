//! Track D3: the interp reference against hand-written expectations, and a
//! smoke run over the TF-converted `conv1d.tflite` (the wrapper-heavy file —
//! the folding-mirroring shape handling gets exercised on real data).

use exporter::graph::{GraphBuilder, TensorQuant};
use exporter::interp::InterpModel;
use exporter::writer;

/// A tiny hand-computable graph: input (1,4,1) scale 1 zp 0;
/// Conv1d(F=1,k=2, w=[1,1], b=0) → [3,5,7];
/// AvgPool(p=2) → [4];
/// FC(Out=2, In=1, w=[[2],[-1]], b=0) → [8,-4];
/// Softmax(out scale 1/256, zp -128) → [127, -128]
/// (p0 = e⁸/(e⁸+e⁻⁴) ≈ 0.999994 → round(255.998)−128 → clamped 127;
///  p1 ≈ 6.1e-6 → round(0.00157) = 0 → −128).
#[test]
fn interp_matches_handwritten_values() {
    let unit = || TensorQuant::per_tensor(1.0, 0);
    let mut b = GraphBuilder::new();
    b.add_input([1, 4, 1], unit())
        .unwrap()
        .add_conv_1d(
            1,
            2,
            vec![1, 1],
            vec![1.0],
            vec![0],
            vec![1.0],
            unit(),
            true,
        )
        .unwrap()
        .add_avg_pool(2, unit())
        .unwrap()
        .add_fc(
            2,
            vec![2, -1],
            vec![1.0; 2],
            vec![0; 2],
            vec![1.0; 2],
            unit(),
        )
        .unwrap()
        .add_softmax(TensorQuant::per_tensor(1.0 / 256.0, -128))
        .unwrap();
    let graph = b.build().unwrap();
    let bytes = writer::write(&graph);
    let interp = InterpModel::from_bytes(bytes).unwrap();

    let out = interp.run_quantized(&[1, 2, 3, 4]).unwrap();
    let kinds: Vec<&str> = out.layers.iter().map(|l| l.kind.as_str()).collect();
    assert_eq!(
        kinds,
        ["CONV_2D", "AVERAGE_POOL_2D", "FULLY_CONNECTED", "SOFTMAX"]
    );
    assert_eq!(out.layers[0].quantized, vec![3, 5, 7]);
    assert_eq!(out.layers[1].quantized, vec![4]);
    assert_eq!(out.layers[2].quantized, vec![8, -4]);
    assert_eq!(out.quantized_output, vec![127, -128]);
    // The dequantized probabilities: (q - (-128)) / 256.
    assert!((out.probabilities[0] - 255.0 / 256.0).abs() < 1e-6);
    assert!(out.probabilities[0] + out.probabilities[1] > 0.99);
}

/// D3 smoke: the TF-converted spike file (18 operators with wrappers) runs
/// through the reference and produces sane softmax probabilities.
#[test]
fn interp_smokes_the_tf_converted_file() {
    let path = std::path::Path::new("../../fork/microflow/models/conv1d.tflite");
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {path:?}: {e}"));
    let interp = InterpModel::from_bytes(bytes).unwrap();
    assert_eq!(interp.input_shape().unwrap(), [1, 128, 1]);
    // A mid-amplitude synthetic window (the a.rs test's carrier shape).
    let window: Vec<f32> = (0..128)
        .map(|t| {
            let ts = t as f32 / 1600.0;
            (2.0 * core::f32::consts::PI * 50.0 * ts).sin()
                + 0.15 * (2.0 * core::f32::consts::PI * 150.0 * ts).sin()
        })
        .collect();
    let out = interp.run(&window).unwrap();
    assert_eq!(out.probabilities.len(), 4);
    let sum: f32 = out.probabilities.iter().sum();
    assert!((sum - 1.0).abs() < 0.02, "softmax must sum to 1, got {sum}");
    for &p in &out.probabilities {
        assert!((0.0..=1.0).contains(&p), "probability out of range: {p}");
    }
}
