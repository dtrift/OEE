//! A directed probe (D1 debugging): the unambiguous toy graphs through the
//! `#[model]` macro. `toy` — single channel; `toy2` — two channels/filters
//! (the multichannel weight-order probe): input [1,2,3,4,5,6,7,8] as (t,c)
//! must produce logits [2,3] → softmax [0.269, 0.731] → quantized [-59, 59].
//! If these fail while the interp self-test passes, the writer's byte order
//! and the macro's decoder disagree.

use microflow::model;
use nalgebra::SMatrix;

#[model("ml/models/model_toy_rust.tflite")]
struct Toy;

#[model("ml/models/model_toy2_rust.tflite")]
struct Toy2;

#[test]
fn toy_chain_matches_the_hand_computation() {
    // predict_quantized dequantizes the softmax output with (q - (-128))/256.
    let input: SMatrix<i8, 4, 1> = SMatrix::from_fn(|t, _| [1i8, 2, 3, 4][t]);
    let output = Toy::predict_quantized(input);
    let got: Vec<i32> = output
        .iter()
        .map(|v| (v * 256.0).round() as i32 - 128)
        .collect();
    // Expected quantized output [127, -128] → recovered [-? ]: the dequantized
    // probabilities are 255/256 and 0/256.
    assert_eq!(got[0], 127, "output {got:?}");
    assert_eq!(got[1], -128, "output {got:?}");
}

#[test]
fn toy2_multichannel_order_matches() {
    let input: SMatrix<i8, 4, 2> = SMatrix::from_fn(|t, c| [1i8, 2, 3, 4, 5, 6, 7, 8][t * 2 + c]);
    let output = Toy2::predict_quantized(input);
    let got: Vec<i32> = output
        .iter()
        .map(|v| (v * 256.0).round() as i32 - 128)
        .collect();
    assert_eq!(got[0], -59, "output {got:?}");
    assert_eq!(got[1], 59, "output {got:?}");
}

#[test]
fn toy_predict_sums_to_one() {
    let window: SMatrix<f32, 4, 1> = SMatrix::from_fn(|t, _| [1.0f32, 2.0, 3.0, 4.0][t]);
    let output = Toy::predict(window);
    let sum: f32 = output.iter().sum();
    assert!((sum - 1.0).abs() < 0.02, "softmax must sum to 1, got {sum}");
}
