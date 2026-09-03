//! Post-training quantization (track D2): float weights + calibration windows
//! → the quantized `ModelGraph` for the writer.
//!
//! Conventions (the week-1 serialization facts, `fork/docs/conv1d-spec.md`):
//! - weights per-channel symmetric: `scale_w = max|w| / 127`, zp = 0 (F5/F6);
//! - activations per-tensor asymmetric (F7); post-ReLU tensors get the
//!   `[0, max]` range with zp = −128 (the full lower half of int8 is the
//!   clamped-to-zero region, exactly how the TF-converted file behaves);
//! - pools keep the input's quantization (the TF file's pools share the
//!   convolution's scale — the requant ratio is 1);
//! - the softmax **output** (the model output tensor) gets `scale = 1/256`,
//!   zp = −128 — the TF convention the microflow kernel quantizes
//!   probabilities with;
//! - bias int32 = `round_ties_even(b / (sx * sw))`, `scale_b = sx * sw` (so
//!   the macro's accumulator-unit conversion is an identity).
//!
//! Calibration runs the float model over the windows and records per-layer
//! min/max (like `tf.lite`'s representative dataset does).

use crate::graph::{GraphBuilder, ModelGraph, TensorQuant};

/// A float Conv1D (burn layout `[F, C, k]`, matching [`crate::weights`]).
#[derive(Clone, Debug)]
pub struct FloatConv {
    pub filters: usize,
    pub kernel: usize,
    /// `[F][C][k]` row-major.
    pub weights: Vec<f32>,
    pub bias: Vec<f32>,
}

/// A float fully-connected layer (`[Out, In]` row-major).
#[derive(Clone, Debug)]
pub struct FloatFc {
    pub out_units: usize,
    pub in_units: usize,
    pub weights: Vec<f32>,
    pub bias: Vec<f32>,
}

/// The float model A architecture with trained weights.
#[derive(Clone, Debug)]
pub struct FloatModel {
    pub timesteps: usize,
    pub channels: usize,
    pub conv1: FloatConv,
    pub pool1: usize,
    pub conv2: FloatConv,
    pub pool2: usize,
    pub fc: FloatFc,
}

/// Per-channel symmetric scale of one filter: `max|w| / 127` (1.0 for an
/// all-zero filter — keeps the downstream ratios finite).
pub fn symmetric_channel_scale(values: &[f32]) -> f32 {
    let max = values.iter().fold(0.0f32, |acc, v| acc.max(v.abs()));
    if max == 0.0 {
        1.0
    } else {
        max / 127.0
    }
}

/// Symmetric quantization of one value (round-to-nearest-even, ±127).
pub fn quantize_symmetric(value: f32, scale: f32) -> i8 {
    let q = (value / scale).round_ties_even();
    q.clamp(-127.0, 127.0) as i8
}

/// Asymmetric per-tensor parameters for a `[min, max]` range, mapping it
/// onto the full int8 range `[-128, 127]` (the TFLite convention:
/// `zp = -128 - round(min / scale)`, so `q(min) = -128`, `q(max) ≈ 127`).
pub fn asymmetric_params(min: f32, max: f32) -> (f32, i64) {
    let range = max - min;
    if range <= 0.0 {
        return (1.0, 0);
    }
    let scale = range / 255.0;
    let zp = (-128.0 - (min / scale).round_ties_even()).clamp(-128.0, 127.0) as i64;
    (scale, zp)
}

/// Post-ReLU parameters: `[0, max]` with zp = −128.
pub fn relu_params(max: f32) -> TensorQuant {
    let max = max.max(1e-6);
    TensorQuant::per_tensor(max / 255.0, -128)
}

/// Bias quantization to accumulator units: `round_ties_even(b / (sx * sw))`.
pub fn quantize_bias(bias: f32, sx: f32, sw: f32) -> i32 {
    (bias / (sx * sw)).round_ties_even() as i32
}

/// Quantizes the float model with calibration windows (each `T * C` values,
/// `(t, c)` row-major — the CSV column order).
pub fn quantize(model: &FloatModel, calib: &[Vec<f32>]) -> Result<ModelGraph, String> {
    if calib.is_empty() {
        return Err("quantization needs at least one calibration window".into());
    }
    let expected = model.timesteps * model.channels;
    for (n, w) in calib.iter().enumerate() {
        if w.len() != expected {
            return Err(format!(
                "calibration window {n} holds {} values, the input needs {expected}",
                w.len()
            ));
        }
    }

    // Input range from the raw windows.
    let mut in_min = f32::INFINITY;
    let mut in_max = f32::NEG_INFINITY;
    for w in calib {
        for &v in w {
            in_min = in_min.min(v);
            in_max = in_max.max(v);
        }
    }
    let (in_scale, in_zp) = asymmetric_params(in_min, in_max);

    // The flattened FC input must line up with the declared architecture.
    let l1 = model.timesteps - model.conv1.kernel + 1;
    let l1p = (l1 - model.pool1) / model.pool1 + 1;
    let l2 = l1p - model.conv2.kernel + 1;
    let l2p = (l2 - model.pool2) / model.pool2 + 1;
    let flat_len = l2p * model.conv2.filters;
    if flat_len != model.fc.in_units {
        return Err(format!(
            "the architecture flattens to {flat_len} units, but the FC declares {}",
            model.fc.in_units
        ));
    }

    // Activation ranges from the float forward pass.
    let mut ranges = Ranges::new();
    for w in calib {
        float_forward(model, w, &mut ranges);
    }

    // Quantized weights + biases.
    let q_conv1 = quantize_conv(&model.conv1, in_scale)?;
    let conv1_out_quant = relu_params(ranges.conv1_max);
    let q_conv2 = quantize_conv(&model.conv2, conv1_out_quant.scale[0])?;
    let conv2_out_quant = relu_params(ranges.conv2_max);
    let fc_in_scale = conv2_out_quant.scale[0];
    let q_fc = quantize_fc(&model.fc, fc_in_scale)?;
    let (fc_scale, fc_zp) = asymmetric_params(ranges.fc_min, ranges.fc_max);

    let mut b = GraphBuilder::new();
    b.add_input(
        [1, model.timesteps, model.channels],
        TensorQuant::per_tensor(in_scale, in_zp),
    )?
    .add_conv_1d(
        model.conv1.filters,
        model.conv1.kernel,
        q_conv1.weights_ohwi,
        q_conv1.weight_scales,
        q_conv1.bias,
        q_conv1.bias_scales,
        conv1_out_quant.clone(),
        true,
    )?
    .add_avg_pool(model.pool1, conv1_out_quant.clone())?
    .add_conv_1d(
        model.conv2.filters,
        model.conv2.kernel,
        q_conv2.weights_ohwi,
        q_conv2.weight_scales,
        q_conv2.bias,
        q_conv2.bias_scales,
        conv2_out_quant.clone(),
        true,
    )?
    .add_avg_pool(model.pool2, conv2_out_quant.clone())?
    .add_fc(
        model.fc.out_units,
        q_fc.weights,
        q_fc.weight_scales,
        q_fc.bias,
        q_fc.bias_scales,
        TensorQuant::per_tensor(fc_scale, fc_zp),
    )?
    .add_softmax(TensorQuant::per_tensor(1.0 / 256.0, -128))?;
    b.build()
}

struct QuantConv {
    weights_ohwi: Vec<i8>,
    weight_scales: Vec<f32>,
    bias: Vec<i32>,
    bias_scales: Vec<f32>,
}

/// Quantizes one convolution: burn `[F, C, k]` → file OHWI `[F, 1, k, C]`
/// (the permutation of the last two axes, track D4.2).
fn quantize_conv(conv: &FloatConv, sx: f32) -> Result<QuantConv, String> {
    let (f, c, k) = (
        conv.filters,
        conv.weights.len() / conv.filters / conv.kernel,
        conv.kernel,
    );
    if conv.weights.len() != f * c * k {
        return Err(format!(
            "conv weights hold {} values, ({f}, {c}, {k}) needs {}",
            conv.weights.len(),
            f * c * k
        ));
    }
    if conv.bias.len() != f {
        return Err(format!(
            "conv bias needs {f} values, got {}",
            conv.bias.len()
        ));
    }
    let mut weight_scales = Vec::with_capacity(f);
    for fi in 0..f {
        let channel: Vec<f32> = (0..c * k).map(|i| conv.weights[fi * c * k + i]).collect();
        weight_scales.push(symmetric_channel_scale(&channel));
    }
    let mut weights_ohwi = Vec::with_capacity(f * k * c);
    let mut bias = Vec::with_capacity(f);
    let mut bias_scales = Vec::with_capacity(f);
    for (fi, &scale_f) in weight_scales.iter().enumerate().take(f) {
        // OHWI: filter, then the kernel axis, then channels.
        for ki in 0..k {
            for ci in 0..c {
                let w = conv.weights[fi * c * k + ci * k + ki];
                weights_ohwi.push(quantize_symmetric(w, scale_f));
            }
        }
        bias.push(quantize_bias(conv.bias[fi], sx, scale_f));
        bias_scales.push(sx * scale_f);
    }
    Ok(QuantConv {
        weights_ohwi,
        weight_scales,
        bias,
        bias_scales,
    })
}

struct QuantFc {
    weights: Vec<i8>,
    weight_scales: Vec<f32>,
    bias: Vec<i32>,
    bias_scales: Vec<f32>,
}

fn quantize_fc(fc: &FloatFc, sx: f32) -> Result<QuantFc, String> {
    if fc.weights.len() != fc.out_units * fc.in_units {
        return Err(format!(
            "fc weights hold {} values, ({}, {}) needs {}",
            fc.weights.len(),
            fc.out_units,
            fc.in_units,
            fc.out_units * fc.in_units
        ));
    }
    let mut weight_scales = Vec::with_capacity(fc.out_units);
    for j in 0..fc.out_units {
        let row = &fc.weights[j * fc.in_units..(j + 1) * fc.in_units];
        weight_scales.push(symmetric_channel_scale(row));
    }
    let mut bias = Vec::with_capacity(fc.out_units);
    let mut bias_scales = Vec::with_capacity(fc.out_units);
    for (j, &scale_j) in weight_scales.iter().enumerate().take(fc.out_units) {
        bias.push(quantize_bias(fc.bias[j], sx, scale_j));
        bias_scales.push(sx * scale_j);
    }
    Ok(QuantFc {
        weights: fc
            .weights
            .iter()
            .zip((0..fc.out_units * fc.in_units).map(|i| weight_scales[i / fc.in_units]))
            .map(|(&w, s)| quantize_symmetric(w, s))
            .collect(),
        weight_scales,
        bias,
        bias_scales,
    })
}

/// The float forward pass over one window, probabilities only (the D6.2
/// float-parity reference: burn's own forward is pinned equal to this math
/// by the trainer's `burn_forward_matches_the_export_layout` test).
pub fn float_probs(model: &FloatModel, x: &[f32]) -> Vec<f32> {
    let mut ranges = Ranges::new();
    float_forward(model, x, &mut ranges);
    let (t, c) = (model.timesteps, model.channels);
    let f1 = model.conv1.filters;
    let l1 = t - model.conv1.kernel + 1;
    let conv1 = conv_forward(&model.conv1, x, t, c);
    let pool1 = pool_forward(&conv1, l1, f1, model.pool1);
    let l1p = (l1 - model.pool1) / model.pool1 + 1;
    let l2 = l1p - model.conv2.kernel + 1;
    let conv2 = conv_forward(&model.conv2, &pool1, l1p, f1);
    let flat = pool_forward(&conv2, l2, model.conv2.filters, model.pool2);
    let logits: Vec<f32> = (0..model.fc.out_units)
        .map(|j| {
            let mut acc = model.fc.bias[j];
            for (i, &v) in flat.iter().enumerate() {
                acc += v * model.fc.weights[j * model.fc.in_units + i];
            }
            acc
        })
        .collect();
    let max = logits.iter().fold(f32::NEG_INFINITY, |a, v| a.max(*v));
    let sum: f32 = logits.iter().map(|l| (l - max).exp()).sum();
    logits.iter().map(|l| (l - max).exp() / sum).collect()
}

#[derive(Default)]
struct Ranges {
    conv1_max: f32,
    conv2_max: f32,
    fc_min: f32,
    fc_max: f32,
}

impl Ranges {
    fn new() -> Self {
        Self {
            conv1_max: 0.0,
            conv2_max: 0.0,
            fc_min: f32::INFINITY,
            fc_max: f32::NEG_INFINITY,
        }
    }
}

/// The float forward pass (the calibration reference): Conv1D → ReLU → pool →
/// Conv1D → ReLU → pool → flatten → FC, recording the activation ranges.
fn float_forward(model: &FloatModel, x: &[f32], ranges: &mut Ranges) {
    let (t, c) = (model.timesteps, model.channels);
    let f1 = model.conv1.filters;
    let l1 = t - model.conv1.kernel + 1;
    let conv1 = conv_forward(&model.conv1, x, t, c);
    ranges.conv1_max = ranges
        .conv1_max
        .max(conv1.iter().fold(0.0f32, |a, v| a.max(*v)));
    let l1p = (l1 - model.pool1) / model.pool1 + 1;
    let pool1 = pool_forward(&conv1, l1, f1, model.pool1);
    let f2 = model.conv2.filters;
    let l2 = l1p - model.conv2.kernel + 1;
    let conv2 = conv_forward(&model.conv2, &pool1, l1p, f1);
    ranges.conv2_max = ranges
        .conv2_max
        .max(conv2.iter().fold(0.0f32, |a, v| a.max(*v)));
    // The pool output is already (t, f) row-major — the TFLite FC input order.
    let flat = pool_forward(&conv2, l2, f2, model.pool2);
    let logits: Vec<f32> = (0..model.fc.out_units)
        .map(|j| {
            let mut acc = model.fc.bias[j];
            for (i, &v) in flat.iter().enumerate() {
                acc += v * model.fc.weights[j * model.fc.in_units + i];
            }
            acc
        })
        .collect();
    ranges.fc_min = ranges
        .fc_min
        .min(logits.iter().fold(f32::INFINITY, |a, v| a.min(*v)));
    ranges.fc_max = ranges
        .fc_max
        .max(logits.iter().fold(f32::NEG_INFINITY, |a, v| a.max(*v)));
}

/// Conv1D forward over a `(t, c)` row-major input; output `(t', f)` row-major.
fn conv_forward(conv: &FloatConv, x: &[f32], t: usize, c: usize) -> Vec<f32> {
    let out_t = t - conv.kernel + 1;
    let mut out = vec![0.0f32; out_t * conv.filters];
    for ti in 0..out_t {
        for f in 0..conv.filters {
            let mut acc = conv.bias[f];
            for ki in 0..conv.kernel {
                for ci in 0..c {
                    acc += x[(ti + ki) * c + ci]
                        * conv.weights[f * c * conv.kernel + ci * conv.kernel + ki];
                }
            }
            out[ti * conv.filters + f] = acc.max(0.0); // fused ReLU
        }
    }
    out
}

/// Average pool over `(t, f)` row-major; output `(t', f)` row-major.
fn pool_forward(x: &[f32], t: usize, f: usize, p: usize) -> Vec<f32> {
    let out_t = (t - p) / p + 1;
    let mut out = vec![0.0f32; out_t * f];
    for ti in 0..out_t {
        for fi in 0..f {
            let mut sum = 0.0;
            for j in 0..p {
                sum += x[(ti * p + j) * f + fi];
            }
            out[ti * f + fi] = sum / p as f32;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-computed per the formulas (the week-2/3 lesson: expectations from
    /// the formulas, cross-checked by a second method — the dequantize
    /// round-trip below).
    #[test]
    fn symmetric_scale_and_quantization() {
        // One filter, w = [1.0, -0.5, 0.25]: scale = 1.0/127.
        let scale = symmetric_channel_scale(&[1.0, -0.5, 0.25]);
        assert!((scale - 1.0 / 127.0).abs() < 1e-9, "{scale}");
        assert_eq!(quantize_symmetric(1.0, scale), 127);
        // -0.5 / (1/127) = -63.5 → ties-to-even → -64.
        assert_eq!(quantize_symmetric(-0.5, scale), -64);
        // 0.25 / (1/127) = 31.75 → 32.
        assert_eq!(quantize_symmetric(0.25, scale), 32);
        // Second method: |deq(q) - w| <= scale/2 (up to the tie rounding).
        for (w, q) in [(1.0f32, 127i8), (-0.5, -64), (0.25, 32)] {
            let deq = q as f32 * scale;
            assert!(
                (deq - w).abs() <= scale / 2.0 + 1e-9,
                "deq {deq} vs {w} (scale {scale})"
            );
        }
    }

    #[test]
    fn all_zero_filter_gets_unit_scale() {
        assert_eq!(symmetric_channel_scale(&[0.0; 8]), 1.0);
        assert_eq!(quantize_symmetric(0.0, 1.0), 0);
    }

    #[test]
    fn bias_accumulator_units() {
        // b = 0.3, sx = 0.01, sw = 1/127: b / (sx*sw) = 0.3 * 12700 = 3810.
        assert_eq!(quantize_bias(0.3, 0.01, 1.0 / 127.0), 3810);
        // ties-even: 0.5 → 0.
        assert_eq!(quantize_bias(0.5, 1.0, 1.0), 0);
        assert_eq!(quantize_bias(1.5, 1.0, 1.0), 2);
    }

    #[test]
    fn asymmetric_range_parameters() {
        let (scale, zp) = asymmetric_params(-1.65, 1.61);
        // The TF week-1 file fact: input scale 0.0128, zp 1 — the full int8
        // range must be used (q(min) = -128, q(max) ≈ 127).
        assert!((scale - (1.61 + 1.65) / 255.0).abs() < 1e-6, "{scale}");
        assert_eq!(zp, 1);
        let (scale, zp) = asymmetric_params(-35.3, 25.3);
        let q_max = (25.3f32 / scale).round_ties_even() as i32 + zp as i32;
        let q_min = (-35.3f32 / scale).round_ties_even() as i32 + zp as i32;
        assert_eq!(q_min, -128);
        assert!((q_max - 127).abs() <= 1, "q_max {q_max}");
        // Degenerate range: a safe identity quantization.
        assert_eq!(asymmetric_params(3.0, 3.0), (1.0, 0));
    }

    /// Near-zero calibration ranges (a near-silent window): tiny but valid
    /// ranges must yield finite parameters inside the int8 bounds, and
    /// out-of-range values under a tiny scale must saturate, not overflow.
    /// Expectations from the formulas; the fp division noise decides which
    /// side of a .5 tie the zero point lands on, so the tiny symmetric case
    /// pins the full-range mapping invariant, not an exact zp.
    #[test]
    fn near_zero_ranges_quantize_in_bounds() {
        // Tiny symmetric range [-0.001, 0.001]: scale ≈ 2e-3/255, min/scale
        // ≈ -127.5 — a tie, so zp is 0 or -1 depending on the rounding noise.
        let (scale, zp) = asymmetric_params(-0.001, 0.001);
        assert!(scale > 0.0 && scale < 1e-5, "scale {scale}");
        assert!((-128..=127).contains(&zp), "zp {zp}");
        let q_min = (-0.001f32 / scale).round_ties_even() as i32 + zp as i32;
        let q_max = (0.001f32 / scale).round_ties_even() as i32 + zp as i32;
        assert_eq!(q_min, -128);
        assert!((q_max - 127).abs() <= 1, "q_max {q_max}");
        // A loud value against a near-silent calibration saturates — no
        // inf/NaN through the tiny scale.
        assert_eq!(quantize_symmetric(1000.0, scale), 127);
        assert_eq!(quantize_symmetric(-1000.0, scale), -127);

        // One-sided near-zero [0, 1e-9]: round(0/scale) = 0 → zp = -128
        // exactly; the maximum maps onto 127 (255 - 128).
        let (scale, zp) = asymmetric_params(0.0, 1e-9);
        let expected_scale = 1e-9f32 / 255.0;
        assert!(
            (scale - expected_scale).abs() <= expected_scale * 1e-3,
            "scale {scale}"
        );
        assert_eq!(zp, -128);
        assert_eq!((1e-9f32 / scale).round_ties_even() as i32 + zp as i32, 127);

        // Mirrored [-1e-9, 0]: round(min/scale) = -255 → zp sits on the
        // upper clamp edge, 127.
        let (scale, zp) = asymmetric_params(-1e-9, 0.0);
        assert_eq!(zp, 127);
        assert_eq!(
            (-1e-9f32 / scale).round_ties_even() as i32 + zp as i32,
            -128
        );
    }

    #[test]
    fn post_relu_parameters_use_the_lower_half() {
        let q = relu_params(1.02);
        assert_eq!(q.zero_point[0], -128);
        assert!((q.scale[0] - 1.02 / 255.0).abs() < 1e-9);
        // Zero maps to the zero point (the clamp region IS the ReLU region).
        assert_eq!(
            (0.0 / q.scale[0]).round_ties_even() as i32 + q.zero_point[0] as i32,
            -128
        );
    }

    #[test]
    fn ohwi_permutation_from_burn_layout() {
        // burn [F=1, C=2, k=3] = [a b c; d e f] (c-major per filter) →
        // OHWI [1,1,3,2] = a d b e c f (k outer, c inner).
        let conv = FloatConv {
            filters: 1,
            kernel: 3,
            weights: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], // [f0c0k0..2, f0c1k0..2]
            bias: vec![0.0],
        };
        let q = quantize_conv(&conv, 1.0).unwrap();
        // scale = 6/127; expected order [1, 4, 2, 5, 3, 6] * 127/6 rounded.
        let s = 6.0f32 / 127.0;
        let expected: Vec<i8> = [1.0, 4.0, 2.0, 5.0, 3.0, 6.0]
            .iter()
            .map(|&w| (w / s).round_ties_even() as i8)
            .collect();
        assert_eq!(q.weights_ohwi, expected);
    }
}
