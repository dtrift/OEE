//! Track D1/D2: the first rust-born `.tflite`. Two artifacts:
//!
//! 1. `model_dummy_rust.tflite` — the full PTQ path on a dummy float model
//!    (fixed seed): realistic scales, so the downstream kernels stay in their
//!    numeric domain (the microflow softmax computes `expf(logit * scale)`
//!    without max-subtraction — an activation scale of 1.0 would overflow).
//! 2. `model_toy_rust.tflite` / `model_toy2_rust.tflite` — the unambiguous
//!    hand-computed probes of the byte order (`tests/toy_probe.rs`), where
//!    unit scales are safe (logits ≤ 8).
//!
//! Run from the repo root:
//!     cargo run -p exporter --bin gen_dummy

use exporter::dumper;
use exporter::graph::{GraphBuilder, TensorQuant};
use exporter::quant::{self, FloatConv, FloatFc, FloatModel};
use exporter::writer;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// A deterministic float dummy: small weights so the calibrated ranges look
/// like a trained model's.
fn dummy_float_model(rng: &mut StdRng) -> FloatModel {
    let mut conv = |filters: usize, chans: usize| FloatConv {
        filters,
        kernel: 3,
        weights: (0..filters * chans * 3)
            .map(|_| rng.random_range(-0.5..0.5))
            .collect(),
        bias: (0..filters)
            .map(|_| rng.random_range(-0.05..0.05))
            .collect(),
    };
    FloatModel {
        timesteps: 128,
        channels: 1,
        conv1: conv(8, 1),
        pool1: 2,
        conv2: conv(16, 8),
        pool2: 2,
        fc: FloatFc {
            out_units: 4,
            in_units: 480,
            weights: (0..4 * 480).map(|_| rng.random_range(-0.1..0.1)).collect(),
            bias: vec![0.1, -0.1, 0.05, -0.05],
        },
    }
}

/// Calibration windows: the simulator's carrier shape at several amplitudes.
fn calibration_windows() -> Vec<Vec<f32>> {
    [0.15f32, 0.4, 0.8, 1.2]
        .iter()
        .map(|&amplitude| {
            (0..128)
                .map(|t| {
                    let ts = t as f32 / 1600.0;
                    amplitude
                        * ((2.0 * core::f32::consts::PI * 50.0 * ts).sin()
                            + 0.15 * (2.0 * core::f32::consts::PI * 150.0 * ts).sin()
                            + 0.07 * (2.0 * core::f32::consts::PI * 250.0 * ts).sin())
                })
                .collect()
        })
        .collect()
}

fn main() {
    let mut rng = StdRng::seed_from_u64(42);

    // 1. The PTQ dummy: float weights → calibration → quantized graph.
    let float_model = dummy_float_model(&mut rng);
    let graph =
        quant::quantize(&float_model, &calibration_windows()).expect("the dummy model quantizes");
    let bytes = writer::write(&graph);
    let tflite = "ml/models/model_dummy_rust.tflite";
    std::fs::write(tflite, &bytes).expect("write the dummy model");
    let dump = dumper::dump_bytes(&bytes).unwrap();
    std::fs::write("ml/models/model_dummy_rust_ops.txt", dump).expect("write the dump");
    println!("wrote {tflite} ({} bytes) + dump", bytes.len());
    assert_eq!(
        writer::write(&graph),
        bytes,
        "the writer must be deterministic"
    );

    // 2. The toy probes (unit scales, hand-computed — see tests/toy_probe.rs
    //    and tests/interp_self.rs for the expected chains).
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
    let toy = b.build().unwrap();
    std::fs::write("ml/models/model_toy_rust.tflite", writer::write(&toy))
        .expect("write the toy model");

    // Toy2: two channels, two filters — the multichannel order probe.
    let mut b = GraphBuilder::new();
    b.add_input([1, 4, 2], unit())
        .unwrap()
        .add_conv_1d(
            2,
            2,
            vec![1, 0, 0, 0, 0, 1, 0, 0],
            vec![1.0; 2],
            vec![0; 2],
            vec![1.0; 2],
            unit(),
            true,
        )
        .unwrap()
        .add_avg_pool(2, unit())
        .unwrap()
        .add_fc(
            2,
            vec![1, 0, 0, 1],
            vec![1.0; 2],
            vec![0; 2],
            vec![1.0; 2],
            unit(),
        )
        .unwrap()
        .add_softmax(TensorQuant::per_tensor(1.0 / 256.0, -128))
        .unwrap();
    let toy2 = b.build().unwrap();
    std::fs::write("ml/models/model_toy2_rust.tflite", writer::write(&toy2))
        .expect("write the toy2 model");
    println!("wrote ml/models/model_toy_rust.tflite + ml/models/model_toy2_rust.tflite");
}
