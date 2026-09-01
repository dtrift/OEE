//! The naive float reference over a `.tflite` (track D3): our replacement for
//! the TF interpreter in the repro loop. Reads the file through the same
//! vendored reader the `#[model]` macro uses, decodes the buffers and runs
//! honest float math per operator — dequantized operands, an f32 accumulator,
//! requantized layer outputs (mirroring the kernels' per-layer int8 hops, so
//! the divergence against microflow stays within the agreed ±1–2 quanta).
//!
//! Shape handling mirrors the macro's folding (§2.1): shape operators
//! (EXPAND_DIMS/RESHAPE/SHAPE/STRIDED_SLICE/PACK) don't touch the data, rank-3
//! inputs are normalized to `(1, 1, T, C)`, rank > 2 FC inputs flatten to
//! `(1, N)` — so both the TF-converted `conv1d.tflite` and the rust-born
//! minimal file run through the same code.

use crate::vendor::tflite::{
    root_as_model, ActivationFunctionType, Buffer, BuiltinOperator, Model, Operator, OperatorCode,
    Tensor, TensorType,
};

use flatbuffers::{ForwardsUOffset, Vector};

/// One operator's intermediate result (for the layer-wise parity debugging,
/// track D6 / the escalation path "int8 vs float > 2%").
#[derive(Clone, Debug)]
pub struct LayerTrace {
    pub op_index: usize,
    pub kind: String,
    pub shape: Vec<usize>,
    /// The requantized int8 output as the microflow chain would hold it.
    pub quantized: Vec<i8>,
    /// The dequantized view of the same values.
    pub dequantized: Vec<f32>,
}

/// The result of one inference.
#[derive(Clone, Debug)]
pub struct InterpOutput {
    /// The final probabilities (dequantized softmax output).
    pub probabilities: Vec<f32>,
    /// The quantized model output (what `predict_quantized` dequantizes).
    pub quantized_output: Vec<i8>,
    /// Per-operator intermediates, execution order.
    pub layers: Vec<LayerTrace>,
}

/// A parsed model: owns the bytes, re-parses the root table per run (cheap —
/// flatbuffers is a zero-copy view).
#[derive(Clone, Debug)]
pub struct InterpModel {
    bytes: Vec<u8>,
}

impl InterpModel {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        root_as_model(&bytes).map_err(|e| format!("invalid flatbuffers model: {e:?}"))?;
        Ok(Self { bytes })
    }

    pub fn from_file(path: &std::path::Path) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Self::from_bytes(bytes)
    }

    fn with_model<R>(&self, f: impl FnOnce(&Model) -> Result<R, String>) -> Result<R, String> {
        let model = root_as_model(&self.bytes).map_err(|e| format!("invalid model: {e:?}"))?;
        f(&model)
    }

    /// The declared input shape `[1, T, C]`.
    pub fn input_shape(&self) -> Result<[usize; 3], String> {
        self.with_model(|model| {
            let subgraph = model
                .subgraphs()
                .and_then(|s| if !s.is_empty() { Some(s.get(0)) } else { None })
                .ok_or("no subgraphs")?;
            let tensors = subgraph.tensors().ok_or("no tensors")?;
            let input = tensors.get(subgraph.inputs().unwrap().get(0) as usize);
            let shape: Vec<usize> = input.shape().unwrap().iter().map(|d| d as usize).collect();
            let rank3: [usize; 3] = shape
                .clone()
                .try_into()
                .map_err(|_| format!("expected a rank-3 input, got {shape:?}"))?;
            Ok(rank3)
        })
    }

    /// Runs the model on a float window (`T * C` values, `(t, c)` row-major):
    /// quantizes with the input tensor's parameters, then [`Self::run_quantized`].
    pub fn run(&self, input: &[f32]) -> Result<InterpOutput, String> {
        self.with_model(|model| {
            let subgraph = model
                .subgraphs()
                .and_then(|s| if !s.is_empty() { Some(s.get(0)) } else { None })
                .ok_or("no subgraphs")?;
            let tensors = subgraph.tensors().ok_or("no tensors")?;
            let input_tensor = tensors.get(subgraph.inputs().unwrap().get(0) as usize);
            let (scale, zp) = tensor_quant(&input_tensor);
            if scale.len() != 1 {
                return Err("the input tensor must be per-tensor quantized".into());
            }
            let zp = zp[0] as i32;
            let quantized: Vec<i8> = input
                .iter()
                .map(|&v| {
                    let q = (v / scale[0]).round_ties_even() + zp as f32;
                    q.clamp(-128.0, 127.0) as i8
                })
                .collect();
            run_quantized(model, &quantized)
        })
    }

    /// Runs the model on an already-quantized window (int8, the layout
    /// `predict_quantized` takes).
    pub fn run_quantized(&self, input: &[i8]) -> Result<InterpOutput, String> {
        self.with_model(|model| run_quantized(model, input))
    }
}

fn tensor_quant(tensor: &Tensor) -> (Vec<f32>, Vec<i64>) {
    match tensor.quantization() {
        Some(q) => (
            q.scale().unwrap_or_default().iter().collect(),
            q.zero_point().unwrap_or_default().iter().collect(),
        ),
        None => (Vec::new(), Vec::new()),
    }
}

/// Kind resolution mirroring the macro's `builtin_kind`.
fn builtin_kind(
    codes: Vector<ForwardsUOffset<OperatorCode>>,
    operator: &Operator,
) -> BuiltinOperator {
    let code = codes.get(operator.opcode_index() as usize);
    let builtin = code.builtin_code();
    if builtin == BuiltinOperator::ADD && code.deprecated_builtin_code() != 0 {
        BuiltinOperator(code.deprecated_builtin_code() as i32)
    } else {
        builtin
    }
}

/// Effective (rank-2/4) shape of a tensor, mirroring `normalize_real_input`:
/// conv-like rank-3 `(1, T, C)` → `(1, 1, T, C)`; FC/softmax rank > 2 → the
/// flattened `(1, N)`.
fn effective_input_shape(kind: BuiltinOperator, tensor: &Tensor) -> Result<Vec<usize>, String> {
    let conv_like = matches!(
        kind,
        BuiltinOperator::CONV_2D
            | BuiltinOperator::DEPTHWISE_CONV_2D
            | BuiltinOperator::AVERAGE_POOL_2D
    );
    let shape: Vec<usize> = tensor.shape().unwrap().iter().map(|d| d as usize).collect();
    if conv_like {
        match shape.len() {
            4 => Ok(shape),
            3 if shape[0] == 1 => Ok(vec![1, 1, shape[1], shape[2]]),
            rank => Err(format!(
                "the input must be rank-4 (batch, 1, timesteps, channels), got rank {rank} {shape:?}"
            )),
        }
    } else {
        match shape.len() {
            1 => Ok(vec![1, shape[0]]),
            2 => Ok(shape),
            rank if rank > 2 => Ok(vec![1, shape.iter().product()]),
            rank => Err(format!("unsupported input rank {rank} {shape:?}")),
        }
    }
}

fn read_i8_buffer(
    _tensors: Vector<ForwardsUOffset<Tensor>>,
    buffers: Vector<ForwardsUOffset<Buffer>>,
    tensor: &Tensor,
    expected: usize,
    what: &str,
) -> Result<Vec<i8>, String> {
    if tensor.type_() != TensorType::INT8 {
        return Err(format!("{what} must be INT8, got {:?}", tensor.type_()));
    }
    let data = buffers
        .get(tensor.buffer() as usize)
        .data()
        .ok_or_else(|| format!("{what} has no buffer"))?;
    let bytes = data.bytes();
    if bytes.len() != expected {
        return Err(format!(
            "{what} holds {} values, the shape implies {expected}",
            bytes.len()
        ));
    }
    Ok(bytes.iter().map(|&b| b as i8).collect())
}

fn read_i32_buffer(
    buffers: Vector<ForwardsUOffset<Buffer>>,
    tensor: &Tensor,
    expected: usize,
    what: &str,
) -> Result<Vec<i32>, String> {
    if tensor.type_() != TensorType::INT32 {
        return Err(format!("{what} must be INT32, got {:?}", tensor.type_()));
    }
    let data = buffers
        .get(tensor.buffer() as usize)
        .data()
        .ok_or_else(|| format!("{what} has no buffer"))?;
    let bytes = data.bytes();
    if bytes.len() != expected * 4 {
        return Err(format!(
            "{what} holds {} values, the shape implies {expected}",
            bytes.len() / 4
        ));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

fn requantize(value: f32, scale: f32, zp: i64) -> i8 {
    ((value / scale).round_ties_even() + zp as f32).clamp(-128.0, 127.0) as i8
}

fn dequantize(q: i8, scale: f32, zp: i64) -> f32 {
    (q as i32 - zp as i32) as f32 * scale
}

fn run_quantized(model: &Model, input: &[i8]) -> Result<InterpOutput, String> {
    let subgraph = model
        .subgraphs()
        .and_then(|s| if !s.is_empty() { Some(s.get(0)) } else { None })
        .ok_or("no subgraphs")?;
    let tensors = subgraph.tensors().ok_or("no tensors")?;
    let buffers = model.buffers().ok_or("no buffers")?;
    let codes = model.operator_codes().ok_or("no operator codes")?;
    let operators = subgraph.operators().unwrap_or_default();

    let input_tensor_index = subgraph.inputs().unwrap().get(0) as usize;
    let output_tensor_index = subgraph.outputs().unwrap().get(0) as usize;
    let input_shape = {
        let t = tensors.get(input_tensor_index);
        let shape: Vec<usize> = t.shape().unwrap().iter().map(|d| d as usize).collect();
        if shape.len() != 3 || shape[0] != 1 {
            return Err(format!(
                "expected a rank-3 (1, T, C) input, got {shape:?} (the reference targets the Conv1D family)"
            ));
        }
        shape
    };
    if input.len() != input_shape[1] * input_shape[2] {
        return Err(format!(
            "the input holds {} values, (1, {}, {}) needs {}",
            input.len(),
            input_shape[1],
            input_shape[2],
            input_shape[1] * input_shape[2]
        ));
    }

    let mut data: Vec<i8> = input.to_vec();
    // Data-origin tracking, mirroring the fold's `Value` table: shape
    // operators (SHAPE/STRIDED_SLICE/PACK) produce no data, EXPAND_DIMS/
    // RESHAPE pass the origin through — the Flatten chain re-attaches to the
    // tensor produced several operators earlier.
    let mut origin: Vec<Option<i32>> = vec![None; tensors.len()];
    origin[input_tensor_index] = Some(input_tensor_index as i32);
    let mut current = input_tensor_index as i32;
    let mut layers = Vec::new();

    for (index, operator) in operators.iter().enumerate() {
        let kind = builtin_kind(codes, &operator);
        let inputs = operator.inputs().unwrap_or_default();
        let outputs = operator.outputs().unwrap_or_default();
        let output_index = outputs.get(0).max(-1);
        if output_index < 0 {
            return Err(format!("op {index} ({kind:?}) has no output"));
        }
        let input_origin = origin.get(inputs.get(0).max(0) as usize).copied().flatten();
        let current_origin = origin
            .get(current as usize)
            .copied()
            .flatten()
            .unwrap_or(current);
        // The linearity check applies to data consumers only; the SHAPE chain
        // (F4) feeds on shape tensors and constants, not on the data flow.
        let shape_op = matches!(
            kind,
            BuiltinOperator::SHAPE | BuiltinOperator::STRIDED_SLICE | BuiltinOperator::PACK
        );
        if !shape_op && input_origin != Some(current_origin) {
            return Err(format!(
                "op {index} ({kind:?}): the input tensor #{} does not follow the previous \
                 operator's output #{current} — branching graphs are not supported",
                inputs.get(0)
            ));
        }
        match kind {
            BuiltinOperator::EXPAND_DIMS | BuiltinOperator::RESHAPE => {
                // A virtual reshape: the data row-major layout is unchanged,
                // the origin passes through (§2.1).
                origin[output_index as usize] = Some(current_origin);
                current = output_index;
            }
            BuiltinOperator::SHAPE | BuiltinOperator::STRIDED_SLICE | BuiltinOperator::PACK => {
                // Shape tensors: no data flows here.
                origin[output_index as usize] = None;
            }
            BuiltinOperator::CONV_2D => {
                let input_tensor = tensors.get(inputs.get(0) as usize);
                let shape = effective_input_shape(kind, &input_tensor)?;
                let [_, _, t, c] = shape[..] else {
                    return Err(format!("op {index}: conv input {shape:?}"));
                };
                let (sx, zpx) = tensor_quant(&input_tensor);
                let (sx, zpx) = (sx[0], zpx[0] as i32);

                let weights_tensor = tensors.get(inputs.get(1) as usize);
                let wshape: Vec<usize> = weights_tensor
                    .shape()
                    .unwrap()
                    .iter()
                    .map(|d| d as usize)
                    .collect();
                if wshape.len() != 4 || wshape[1] != 1 {
                    return Err(format!(
                        "op {index}: CONV_2D expects OHWI filters with height 1, got {wshape:?}"
                    ));
                }
                let (f, k, wc) = (wshape[0], wshape[2], wshape[3]);
                if wc != c {
                    return Err(format!(
                        "op {index}: filters expect {wc} channels, the input has {c}"
                    ));
                }
                let (sw, zpw) = tensor_quant(&weights_tensor);
                let weights =
                    read_i8_buffer(tensors, buffers, &weights_tensor, f * k * c, "conv weights")?;

                let bias = if inputs.len() >= 3 && inputs.get(2) >= 0 {
                    let bias_tensor = tensors.get(inputs.get(2) as usize);
                    let (sb, _) = tensor_quant(&bias_tensor);
                    let raw = read_i32_buffer(buffers, &bias_tensor, f, "conv bias")?;
                    raw.iter()
                        .zip(sb.iter().chain(std::iter::repeat(&sb[0])))
                        .map(|(&v, &s)| v as f32 * s)
                        .collect::<Vec<f32>>()
                } else {
                    vec![0.0; f]
                };

                let out_tensor = tensors.get(output_index as usize);
                let oshape: Vec<usize> = out_tensor
                    .shape()
                    .unwrap()
                    .iter()
                    .map(|d| d as usize)
                    .collect();
                let out_t = if oshape.len() == 4 {
                    oshape[2]
                } else if oshape.len() == 3 && oshape[0] == 1 {
                    oshape[1]
                } else {
                    return Err(format!("op {index}: conv output shape {oshape:?}"));
                };
                if out_t != t.saturating_sub(k) + 1 {
                    return Err(format!(
                        "op {index}: conv output length {out_t} does not match the geometry (T {t}, k {k})"
                    ));
                }
                let (so, zpo) = tensor_quant(&out_tensor);
                let (so, zpo) = (so[0], zpo[0]);

                let options = operator
                    .builtin_options_as_conv_2_doptions()
                    .ok_or_else(|| format!("op {index}: missing Conv2D options"))?;
                let fused_relu =
                    options.fused_activation_function() == ActivationFunctionType::RELU;
                if options.stride_w() != 1 || options.stride_h() != 1 {
                    return Err(format!(
                        "op {index}: the reference implements stride-1 convolutions only (got ({}, {}))",
                        options.stride_h(),
                        options.stride_w()
                    ));
                }

                // Float math over dequantized operands (OHWI weights, per-channel).
                let mut out = vec![0i8; out_t * f];
                for ti in 0..out_t {
                    for fi in 0..f {
                        let scale_w = sw.get(fi).copied().unwrap_or(sw[0]);
                        let zp_w = zpw.get(fi).copied().unwrap_or(zpw[0]) as i32;
                        let mut acc = bias[fi];
                        for ki in 0..k {
                            for ci in 0..c {
                                let x = dequantize(data[(ti + ki) * c + ci], sx, zpx as i64);
                                let w = dequantize(
                                    weights[fi * k * c + ki * c + ci],
                                    scale_w,
                                    zp_w as i64,
                                );
                                acc += x * w;
                            }
                        }
                        if fused_relu {
                            acc = acc.max(0.0);
                        }
                        out[ti * f + fi] = requantize(acc, so, zpo);
                    }
                }
                push_trace(
                    &mut layers,
                    index,
                    "CONV_2D",
                    vec![1, 1, out_t, f],
                    &out,
                    so,
                    zpo,
                );
                data = out;
                origin[output_index as usize] = Some(output_index);
                current = output_index;
            }
            BuiltinOperator::AVERAGE_POOL_2D => {
                let input_tensor = tensors.get(inputs.get(0) as usize);
                let shape = effective_input_shape(kind, &input_tensor)?;
                let [_, _, t, c] = shape[..] else {
                    return Err(format!("op {index}: pool input {shape:?}"));
                };
                let (sx, zpx) = tensor_quant(&input_tensor);
                let (sx, zpx) = (sx[0], zpx[0]);
                let out_tensor = tensors.get(output_index as usize);
                let oshape: Vec<usize> = out_tensor
                    .shape()
                    .unwrap()
                    .iter()
                    .map(|d| d as usize)
                    .collect();
                let out_t = if oshape.len() == 4 {
                    oshape[2]
                } else {
                    return Err(format!("op {index}: pool output shape {oshape:?}"));
                };
                let (so, zpo) = tensor_quant(&out_tensor);
                let (so, zpo) = (so[0], zpo[0]);

                let options = operator
                    .builtin_options_as_pool_2_doptions()
                    .ok_or_else(|| format!("op {index}: missing Pool2D options"))?;
                let p = options.filter_width() as usize;
                if options.filter_height() != 1
                    || options.stride_w() as usize != p
                    || options.stride_h() != 1
                {
                    return Err(format!(
                        "op {index}: the reference implements (1, p)/(1, p) pools only (got filter ({}, {}), strides ({}, {}))",
                        options.filter_height(),
                        options.filter_width(),
                        options.stride_h(),
                        options.stride_w()
                    ));
                }
                if out_t != (t.saturating_sub(p)) / p + 1 || t < p {
                    return Err(format!(
                        "op {index}: pool output length {out_t} does not match the geometry (T {t}, p {p})"
                    ));
                }
                let mut out = vec![0i8; out_t * c];
                for ti in 0..out_t {
                    for ci in 0..c {
                        let mut sum = 0.0f32;
                        for j in 0..p {
                            sum += dequantize(data[(ti * p + j) * c + ci], sx, zpx);
                        }
                        out[ti * c + ci] = requantize(sum / p as f32, so, zpo);
                    }
                }
                push_trace(
                    &mut layers,
                    index,
                    "AVERAGE_POOL_2D",
                    vec![1, 1, out_t, c],
                    &out,
                    so,
                    zpo,
                );
                data = out;
                origin[output_index as usize] = Some(output_index);
                current = output_index;
            }
            BuiltinOperator::FULLY_CONNECTED => {
                let input_tensor = tensors.get(inputs.get(0) as usize);
                let shape = effective_input_shape(kind, &input_tensor)?;
                let n = shape[1];
                let (sx, zpx) = tensor_quant(&input_tensor);
                let (sx, zpx) = (sx[0], zpx[0] as i32);

                let weights_tensor = tensors.get(inputs.get(1) as usize);
                let wshape: Vec<usize> = weights_tensor
                    .shape()
                    .unwrap()
                    .iter()
                    .map(|d| d as usize)
                    .collect();
                if wshape.len() != 2 {
                    return Err(format!(
                        "op {index}: FC weights must be rank-2, got {wshape:?}"
                    ));
                }
                let (out_units, in_units) = (wshape[0], wshape[1]);
                if in_units != n {
                    return Err(format!(
                        "op {index}: FC weights expect {in_units} inputs, the tensor has {n}"
                    ));
                }
                let (sw, zpw) = tensor_quant(&weights_tensor);
                let weights = read_i8_buffer(
                    tensors,
                    buffers,
                    &weights_tensor,
                    out_units * in_units,
                    "fc weights",
                )?;

                let bias = if inputs.len() >= 3 && inputs.get(2) >= 0 {
                    let bias_tensor = tensors.get(inputs.get(2) as usize);
                    let (sb, _) = tensor_quant(&bias_tensor);
                    let raw = read_i32_buffer(buffers, &bias_tensor, out_units, "fc bias")?;
                    raw.iter()
                        .zip(sb.iter().chain(std::iter::repeat(&sb[0])))
                        .map(|(&v, &s)| v as f32 * s)
                        .collect::<Vec<f32>>()
                } else {
                    vec![0.0; out_units]
                };

                let out_tensor = tensors.get(output_index as usize);
                let (so, zpo) = tensor_quant(&out_tensor);
                let (so, zpo) = (so[0], zpo[0]);

                let mut out = vec![0i8; out_units];
                for j in 0..out_units {
                    let scale_w = sw.get(j).copied().unwrap_or(sw[0]);
                    let zp_w = zpw.get(j).copied().unwrap_or(zpw[0]) as i32;
                    let mut acc = bias[j];
                    for i in 0..n {
                        let x = dequantize(data[i], sx, zpx as i64);
                        let w = dequantize(weights[j * in_units + i], scale_w, zp_w as i64);
                        acc += x * w;
                    }
                    out[j] = requantize(acc, so, zpo);
                }
                push_trace(
                    &mut layers,
                    index,
                    "FULLY_CONNECTED",
                    vec![1, out_units],
                    &out,
                    so,
                    zpo,
                );
                data = out;
                origin[output_index as usize] = Some(output_index);
                current = output_index;
            }
            BuiltinOperator::SOFTMAX => {
                let input_tensor = tensors.get(inputs.get(0) as usize);
                let shape = effective_input_shape(kind, &input_tensor)?;
                let k = shape[1];
                let (sx, zpx) = tensor_quant(&input_tensor);
                let (sx, zpx) = (sx[0], zpx[0]);
                let out_tensor = tensors.get(output_index as usize);
                let (so, zpo) = tensor_quant(&out_tensor);
                let (so, zpo) = (so[0], zpo[0]);

                let mut logits = vec![0.0f32; k];
                for (i, l) in logits.iter_mut().enumerate() {
                    *l = dequantize(data[i], sx, zpx);
                }
                let max = logits.iter().fold(f32::NEG_INFINITY, |a, v| a.max(*v));
                let sum: f32 = logits.iter().map(|l| (l - max).exp()).sum();
                let mut out = vec![0i8; k];
                for (i, o) in out.iter_mut().enumerate() {
                    let p = (logits[i] - max).exp() / sum;
                    *o = requantize(p, so, zpo);
                }
                push_trace(&mut layers, index, "SOFTMAX", vec![1, k], &out, so, zpo);
                data = out;
                origin[output_index as usize] = Some(output_index);
                current = output_index;
            }
            unsupported => {
                return Err(format!(
                    "op {index}: the reference does not implement {unsupported:?}"
                ))
            }
        }
    }

    if origin.get(output_tensor_index).copied().flatten()
        != Some(current_origin_of(&origin, current))
    {
        return Err(format!(
            "the subgraph output #{output_tensor_index} is not produced by the chain (ends at #{current})"
        ));
    }
    let out_tensor = tensors.get(output_tensor_index);
    let (so, zpo) = tensor_quant(&out_tensor);
    let probabilities = data.iter().map(|&q| dequantize(q, so[0], zpo[0])).collect();
    Ok(InterpOutput {
        probabilities,
        quantized_output: data,
        layers,
    })
}

fn current_origin_of(origin: &[Option<i32>], current: i32) -> i32 {
    origin
        .get(current as usize)
        .copied()
        .flatten()
        .unwrap_or(current)
}

fn push_trace(
    layers: &mut Vec<LayerTrace>,
    op_index: usize,
    kind: &str,
    shape: Vec<usize>,
    quantized: &[i8],
    scale: f32,
    zp: i64,
) {
    layers.push(LayerTrace {
        op_index,
        kind: kind.to_string(),
        shape,
        quantized: quantized.to_vec(),
        dequantized: quantized
            .iter()
            .map(|&q| dequantize(q, scale, zp))
            .collect(),
    });
}
