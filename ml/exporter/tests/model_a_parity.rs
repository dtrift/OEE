//! Track D6: the parity safety net for the rust-born `model_a.tflite`, by the
//! §5.3 pattern (`fork/microflow/tests/conv1d_parity.rs`), with the interp
//! reference as the source of expectations (regenerate with
//! `cargo run -p exporter --release --bin parity_gen`, then `touch` this file
//! — `#[model]` bakes the model at compile time).

use std::fs;

use microflow::model;
use nalgebra::SMatrix;

#[model("ml/models/model_a.tflite")]
struct ModelA;

const FIXTURE: &str = "../../ml/models/model_a_parity.txt";

struct Case {
    input: [i8; 128],
    expected_output: [i8; 4],
}

/// Parses the fixture (the same format the fork's conv1d_parity.rs reads).
fn parse_fixture(text: &str) -> Option<(f32, i8, f32, i8, Vec<Case>)> {
    let mut lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let header: Vec<f32> = lines
        .next()?
        .split_whitespace()
        .filter_map(|token| token.parse::<f32>().ok())
        .collect();
    if header.len() != 4 {
        return None;
    }
    let (input_scale, input_zero_point, output_scale, output_zero_point) =
        (header[0], header[1] as i8, header[2], header[3] as i8);

    let mut cases = Vec::new();
    let mut case = Case {
        input: [0; 128],
        expected_output: [0; 4],
    };
    let mut filled = 0usize;
    let mut in_input = false;
    let mut in_output = false;
    for line in lines {
        match line {
            "input" => {
                if in_output {
                    if filled != case.expected_output.len() {
                        return None;
                    }
                    cases.push(case);
                    case = Case {
                        input: [0; 128],
                        expected_output: [0; 4],
                    };
                }
                in_input = true;
                in_output = false;
                filled = 0;
            }
            "output" => {
                if !in_input || filled != case.input.len() {
                    return None;
                }
                in_input = false;
                in_output = true;
                filled = 0;
            }
            value => {
                let value: i8 = value.parse().ok()?;
                if in_input && filled < case.input.len() {
                    case.input[filled] = value;
                    filled += 1;
                } else if in_output && filled < case.expected_output.len() {
                    case.expected_output[filled] = value;
                    filled += 1;
                } else {
                    return None;
                }
            }
        }
    }
    if in_output && filled == case.expected_output.len() {
        cases.push(case);
    }
    Some((
        input_scale,
        input_zero_point,
        output_scale,
        output_zero_point,
        cases,
    ))
}

/// D6.1: microflow vs the interp reference on the val windows, ±2 quanta per
/// output element (bit-for-bit is NOT required: the operation orders and the
/// rounding modes differ — pool/FC round half-away, conv/interp round
/// ties-even).
#[test]
fn microflow_matches_interp_within_two_quanta() {
    let text = fs::read_to_string(FIXTURE)
        .unwrap_or_else(|_| panic!("run `cargo run -p exporter --release --bin parity_gen` first"));
    let Some((_, _, output_scale, output_zero_point, cases)) = parse_fixture(&text) else {
        panic!("could not parse {FIXTURE}");
    };
    assert!(!cases.is_empty(), "{FIXTURE} holds no cases");

    let mut argmax_disagreements = 0usize;
    for (n, case) in cases.iter().enumerate() {
        let input: SMatrix<i8, 128, 1> = SMatrix::from_fn(|t, _| case.input[t]);
        let output = ModelA::predict_quantized(input);
        let quantized: Vec<i8> = output
            .iter()
            .map(|v| (v / output_scale).round() as i32 + output_zero_point as i32)
            .map(|v| v.clamp(-128, 127) as i8)
            .collect();
        let argmax = |values: &[i8]| {
            values
                .iter()
                .enumerate()
                .max_by_key(|(i, v)| (*v, usize::MAX - *i))
                .map(|(i, _)| i)
                .unwrap_or(usize::MAX)
        };
        if argmax(&quantized) != argmax(&case.expected_output) {
            argmax_disagreements += 1;
        }
        for (k, (got, expected)) in quantized.iter().zip(case.expected_output).enumerate() {
            let diff = (*got as i32 - expected as i32).abs();
            assert!(
                diff <= 2,
                "case {n}, output {k}: microflow {got} vs interp {expected} (diff {diff} > 2 quanta)"
            );
        }
    }
    assert_eq!(
        argmax_disagreements, 0,
        "the argmax must agree on every fixture case"
    );
}

/// D6.2: float-parity — the float model (burn's forward, pinned by the
/// trainer's layout test) against the int8 microflow on val windows:
/// argmax agreement ≥ 99% and `max |Δp| ≤ 0.05` (the week-1 sanity threshold).
#[test]
fn int8_microflow_matches_the_float_model() {
    let float =
        exporter::weights::read_float_model(std::path::Path::new("../../ml/models/model_a.float"))
            .expect("run the trainer pipeline first (ml/README.md)");
    let text = fs::read_to_string(FIXTURE)
        .unwrap_or_else(|_| panic!("run `cargo run -p exporter --release --bin parity_gen` first"));
    let Some((input_scale, input_zero_point, _, _, cases)) = parse_fixture(&text) else {
        panic!("could not parse {FIXTURE}");
    };

    let mut checked = 0usize;
    let mut argmax_disagreements = 0usize;
    let mut max_delta = 0.0f32;
    for case in &cases {
        // The dequantized fixture window — the same data both sides see.
        let window: Vec<f32> = case
            .input
            .iter()
            .map(|&q| (q as i32 - input_zero_point as i32) as f32 * input_scale)
            .collect();
        let input: SMatrix<f32, 128, 1> = SMatrix::from_fn(|t, _| window[t]);
        let output = ModelA::predict(input);
        let int8_probs: Vec<f32> = output.iter().copied().collect();
        let float_probs = exporter::quant::float_probs(&float, &window);

        let argmax = |values: &[f32]| {
            values
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .map(|(i, _)| i)
                .unwrap_or(usize::MAX)
        };
        if argmax(&int8_probs) != argmax(&float_probs) {
            argmax_disagreements += 1;
        }
        for (a, b) in int8_probs.iter().zip(&float_probs) {
            max_delta = max_delta.max((a - b).abs());
        }
        checked += 1;
    }
    assert!(checked > 0);
    assert!(
        argmax_disagreements * 100 <= checked,
        "argmax agreement below 99%: {argmax_disagreements}/{checked}"
    );
    assert!(
        max_delta <= 0.05,
        "max |Δp| = {max_delta} exceeds the 0.05 threshold"
    );
    println!("float-parity: {checked} windows, {argmax_disagreements} argmax diffs, max|Δp| = {max_delta:.4}");
}
