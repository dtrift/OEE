//! The TFLite flatbuffers writer (track D1): assembles the minimal graph into
//! a `.tflite` byte stream through the fork's builder API (`*Args`/`*::create`
//! from the schema bindings included in `src/lib.rs`). No `flatc` involved.
//!
//! Conventions (pinned by the roundtrip test and `fork/NOTES.md`):
//! - `operator_codes`: `[CONV_2D, AVERAGE_POOL_2D, FULLY_CONNECTED, SOFTMAX]`,
//!   `deprecated_builtin_code` mirrors `builtin_code` (all fit in i8);
//! - buffer 0 is empty (the TF convention), activation tensors point at it;
//! - `QuantizationParameters.zero_point` is an **i64** vector (schema type);
//! - `ModelArgs { version: 3, .. }`.
//!
//! The output is deterministic: identical input graphs serialize to identical
//! bytes (the D6 sha256 gate relies on it).

use crate::vendor::tflite::{
    ActivationFunctionType, Buffer, BufferArgs, BuiltinOperator, BuiltinOptions, Conv2DOptions,
    Conv2DOptionsArgs, CustomOptionsFormat, FullyConnectedOptions, FullyConnectedOptionsArgs,
    FullyConnectedOptionsWeightsFormat, Model, ModelArgs, Operator, OperatorArgs, OperatorCode,
    OperatorCodeArgs, Padding, Pool2DOptions, Pool2DOptionsArgs, QuantizationDetails,
    QuantizationParameters, QuantizationParametersArgs, SoftmaxOptions, SoftmaxOptionsArgs,
    SubGraph, SubGraphArgs, Tensor, TensorArgs, TensorType,
};
use flatbuffers::{FlatBufferBuilder, UnionWIPOffset, WIPOffset};

use crate::graph::{Layer, ModelGraph, TensorQuant};

/// Operator code order of the written file.
const CODES: [BuiltinOperator; 4] = [
    BuiltinOperator::CONV_2D,
    BuiltinOperator::AVERAGE_POOL_2D,
    BuiltinOperator::FULLY_CONNECTED,
    BuiltinOperator::SOFTMAX,
];

/// Serializes the graph into `.tflite` bytes.
pub fn write(graph: &ModelGraph) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let mut buffers: Vec<WIPOffset<Buffer>> = Vec::new();
    let mut tensors: Vec<WIPOffset<Tensor>> = Vec::new();

    // Buffer 0 is the empty one (the TF convention: activations point at it).
    buffers.push(Buffer::create(&mut fbb, &BufferArgs { data: None }));

    // The input tensor, rank-3 (1, T, C) as Keras serializes it (F1).
    let input_quant = quant_table(
        &mut fbb,
        &graph.input_quant.scale,
        &graph.input_quant.zero_point,
    );
    let input_name = fbb.create_string("serving_default_input");
    let input_shape = fbb.create_vector(&[
        1i32,
        graph.input_shape[1] as i32,
        graph.input_shape[2] as i32,
    ]);
    tensors.push(Tensor::create(
        &mut fbb,
        &TensorArgs {
            shape: Some(input_shape),
            type_: TensorType::INT8,
            buffer: 0,
            name: Some(input_name),
            quantization: Some(input_quant),
            is_variable: false,
            sparsity: None,
            shape_signature: None,
            has_rank: false,
            variant_tensors: None,
        },
    ));

    // Per-layer tensors and operator wiring, in execution order. The shape
    // cursor mirrors the builder's (the parser normalizes the same way).
    struct Wiring {
        inputs: Vec<i32>,
        outputs: Vec<i32>,
        opcode_index: u32,
        options_type: BuiltinOptions,
        options: WIPOffset<UnionWIPOffset>,
    }
    let mut wirings: Vec<Wiring> = Vec::new();
    let mut last_output = 0i32;
    let (mut timesteps, mut chans) = (graph.input_shape[1], graph.input_shape[2]);

    for (index, layer) in graph.layers.iter().enumerate() {
        match layer {
            Layer::Conv1d(c) => {
                let out = c.out_len;
                let out_idx = tensors.len() as i32;
                tensors.push(activation_tensor(
                    &mut fbb,
                    &c.out_quant,
                    &[1, 1, out, c.filters],
                    &format!("conv{index}_out"),
                ));
                let (w_tensor, _) = i8_tensor(
                    &mut fbb,
                    &mut buffers,
                    TensorQuant::symmetric(c.weight_scales.clone()),
                    &[c.filters, 1, c.kernel, c.in_chans],
                    &format!("conv{index}_weights"),
                    &c.weights,
                );
                tensors.push(w_tensor);
                let w_idx = tensors.len() as i32 - 1;
                let (b_tensor, _) = i32_tensor(
                    &mut fbb,
                    &mut buffers,
                    &c.bias_scales,
                    &[c.filters],
                    &format!("conv{index}_bias"),
                    &c.bias,
                );
                tensors.push(b_tensor);
                let b_idx = tensors.len() as i32 - 1;

                let options = Conv2DOptions::create(
                    &mut fbb,
                    &Conv2DOptionsArgs {
                        padding: Padding::VALID,
                        stride_w: 1,
                        stride_h: 1,
                        fused_activation_function: if c.fused_relu {
                            ActivationFunctionType::RELU
                        } else {
                            ActivationFunctionType::NONE
                        },
                        dilation_w_factor: 1,
                        dilation_h_factor: 1,
                    },
                );
                wirings.push(Wiring {
                    inputs: vec![last_output, w_idx, b_idx],
                    outputs: vec![out_idx],
                    opcode_index: CODE_CODE_INDEX,
                    options_type: BuiltinOptions::Conv2DOptions,
                    options: WIPOffset::new(options.value()),
                });
                last_output = out_idx;
                timesteps = out;
                chans = c.filters;
            }
            Layer::AvgPool1d(p) => {
                let pool = p.pool;
                let out = (timesteps - pool) / pool + 1;
                let out_idx = tensors.len() as i32;
                tensors.push(activation_tensor(
                    &mut fbb,
                    &p.out_quant,
                    &[1, 1, out, chans],
                    &format!("pool{index}_out"),
                ));
                let options = Pool2DOptions::create(
                    &mut fbb,
                    &Pool2DOptionsArgs {
                        padding: Padding::VALID,
                        stride_w: pool as i32,
                        stride_h: 1,
                        filter_width: pool as i32,
                        filter_height: 1,
                        fused_activation_function: ActivationFunctionType::NONE,
                    },
                );
                wirings.push(Wiring {
                    inputs: vec![last_output],
                    outputs: vec![out_idx],
                    opcode_index: POOL_CODE_INDEX,
                    options_type: BuiltinOptions::Pool2DOptions,
                    options: WIPOffset::new(options.value()),
                });
                last_output = out_idx;
                timesteps = out;
            }
            Layer::Fc(f) => {
                let out_idx = tensors.len() as i32;
                tensors.push(activation_tensor(
                    &mut fbb,
                    &f.out_quant,
                    &[1, f.out_units],
                    &format!("fc{index}_out"),
                ));
                let (w_tensor, _) = i8_tensor(
                    &mut fbb,
                    &mut buffers,
                    TensorQuant::symmetric(f.weight_scales.clone()),
                    &[f.out_units, f.in_units],
                    &format!("fc{index}_weights"),
                    &f.weights,
                );
                tensors.push(w_tensor);
                let w_idx = tensors.len() as i32 - 1;
                let (b_tensor, _) = i32_tensor(
                    &mut fbb,
                    &mut buffers,
                    &f.bias_scales,
                    &[f.out_units],
                    &format!("fc{index}_bias"),
                    &f.bias,
                );
                tensors.push(b_tensor);
                let b_idx = tensors.len() as i32 - 1;

                let options = FullyConnectedOptions::create(
                    &mut fbb,
                    &FullyConnectedOptionsArgs {
                        fused_activation_function: ActivationFunctionType::NONE,
                        weights_format: FullyConnectedOptionsWeightsFormat::DEFAULT,
                        keep_num_dims: false,
                        asymmetric_quantize_inputs: false,
                    },
                );
                wirings.push(Wiring {
                    inputs: vec![last_output, w_idx, b_idx],
                    outputs: vec![out_idx],
                    opcode_index: FC_CODE_INDEX,
                    options_type: BuiltinOptions::FullyConnectedOptions,
                    options: WIPOffset::new(options.value()),
                });
                last_output = out_idx;
                // The flat cursor from here on: `chans` now holds the FC unit
                // count (the softmax output shape needs it).
                timesteps = 0;
                chans = f.out_units;
            }
            Layer::Softmax(s) => {
                let out_idx = tensors.len() as i32;
                tensors.push(activation_tensor(
                    &mut fbb,
                    &s.out_quant,
                    &[1, chans],
                    &format!("softmax{index}_out"),
                ));
                let options = SoftmaxOptions::create(&mut fbb, &SoftmaxOptionsArgs { beta: 1.0 });
                wirings.push(Wiring {
                    inputs: vec![last_output],
                    outputs: vec![out_idx],
                    opcode_index: SOFTMAX_CODE_INDEX,
                    options_type: BuiltinOptions::SoftmaxOptions,
                    options: WIPOffset::new(options.value()),
                });
                last_output = out_idx;
            }
        }
    }

    // Operators.
    let mut operators = Vec::with_capacity(wirings.len());
    for wiring in &wirings {
        let inputs = fbb.create_vector(&wiring.inputs);
        let outputs = fbb.create_vector(&wiring.outputs);
        operators.push(Operator::create(
            &mut fbb,
            &OperatorArgs {
                opcode_index: wiring.opcode_index,
                inputs: Some(inputs),
                outputs: Some(outputs),
                builtin_options_type: wiring.options_type,
                builtin_options: Some(wiring.options),
                custom_options: None,
                custom_options_format: CustomOptionsFormat::FLEXBUFFERS,
                mutating_variable_inputs: None,
                intermediates: None,
            },
        ));
    }

    // Subgraph, operator codes, buffers vector, model.
    let tensors_vec = fbb.create_vector(&tensors);
    let operators_vec = fbb.create_vector(&operators);
    let subgraph_inputs = fbb.create_vector(&[0i32]);
    let subgraph_outputs = fbb.create_vector(&[last_output]);
    let subgraph_name = fbb.create_string("main");
    let subgraph = SubGraph::create(
        &mut fbb,
        &SubGraphArgs {
            tensors: Some(tensors_vec),
            inputs: Some(subgraph_inputs),
            outputs: Some(subgraph_outputs),
            operators: Some(operators_vec),
            name: Some(subgraph_name),
        },
    );

    let code_tables: Vec<_> = CODES
        .iter()
        .map(|&code| {
            OperatorCode::create(
                &mut fbb,
                &OperatorCodeArgs {
                    deprecated_builtin_code: code.0 as i8,
                    custom_code: None,
                    version: 1,
                    builtin_code: code,
                },
            )
        })
        .collect();
    let codes_vec = fbb.create_vector(&code_tables);
    let buffers_vec = fbb.create_vector(&buffers);
    let subgraphs_vec = fbb.create_vector(&[subgraph]);
    let description = fbb.create_string("rust-ml track writer");

    let model = Model::create(
        &mut fbb,
        &ModelArgs {
            version: 3,
            operator_codes: Some(codes_vec),
            subgraphs: Some(subgraphs_vec),
            description: Some(description),
            buffers: Some(buffers_vec),
            metadata_buffer: None,
            metadata: None,
            signature_defs: None,
        },
    );
    fbb.finish(model, None);
    fbb.finished_data().to_vec()
}

const CODE_CODE_INDEX: u32 = 0;
const POOL_CODE_INDEX: u32 = 1;
const FC_CODE_INDEX: u32 = 2;
const SOFTMAX_CODE_INDEX: u32 = 3;

/// A `QuantizationParameters` table (scale + zero point vectors, the i64
/// schema type for zero points).
fn quant_table<'b>(
    fbb: &mut FlatBufferBuilder<'b>,
    scale: &[f32],
    zero_point: &[i64],
) -> WIPOffset<QuantizationParameters<'b>> {
    let scale = fbb.create_vector(scale);
    let zp = fbb.create_vector(zero_point);
    QuantizationParameters::create(
        fbb,
        &QuantizationParametersArgs {
            min: None,
            max: None,
            scale: Some(scale),
            zero_point: Some(zp),
            details_type: QuantizationDetails::NONE,
            details: None,
            quantized_dimension: 0,
        },
    )
}

/// An activation tensor: empty buffer 0, per-tensor quantization.
fn activation_tensor<'b>(
    fbb: &mut FlatBufferBuilder<'b>,
    quant: &TensorQuant,
    shape: &[usize],
    name: &str,
) -> WIPOffset<Tensor<'b>> {
    let quant_offset = quant_table(fbb, &quant.scale, &quant.zero_point);
    let name = fbb.create_string(name);
    let shape: Vec<i32> = shape.iter().map(|&d| d as i32).collect();
    let shape = fbb.create_vector(&shape);
    Tensor::create(
        fbb,
        &TensorArgs {
            shape: Some(shape),
            type_: TensorType::INT8,
            buffer: 0,
            name: Some(name),
            quantization: Some(quant_offset),
            is_variable: false,
            sparsity: None,
            shape_signature: None,
            has_rank: false,
            variant_tensors: None,
        },
    )
}

/// Appends a data buffer; returns its index in the buffers vector.
fn push_buffer<'b>(
    fbb: &mut FlatBufferBuilder<'b>,
    buffers: &mut Vec<WIPOffset<Buffer<'b>>>,
    bytes: &[u8],
) -> u32 {
    let data_vec = fbb.create_vector(bytes);
    let buffer = Buffer::create(
        fbb,
        &BufferArgs {
            data: Some(data_vec),
        },
    );
    buffers.push(buffer);
    (buffers.len() - 1) as u32
}

/// An int8 constant tensor (weights): per-channel quantization, data in a
/// fresh buffer. Returns the tensor and its buffer index.
fn i8_tensor<'b>(
    fbb: &mut FlatBufferBuilder<'b>,
    buffers: &mut Vec<WIPOffset<Buffer<'b>>>,
    quant: TensorQuant,
    shape: &[usize],
    name: &str,
    data: &[i8],
) -> (WIPOffset<Tensor<'b>>, u32) {
    let bytes: Vec<u8> = data.iter().map(|&v| v as u8).collect();
    let buffer_index = push_buffer(fbb, buffers, &bytes);
    let quant_offset = quant_table(fbb, &quant.scale, &quant.zero_point);
    let name = fbb.create_string(name);
    let shape: Vec<i32> = shape.iter().map(|&d| d as i32).collect();
    let shape = fbb.create_vector(&shape);
    let tensor = Tensor::create(
        fbb,
        &TensorArgs {
            shape: Some(shape),
            type_: TensorType::INT8,
            buffer: buffer_index,
            name: Some(name),
            quantization: Some(quant_offset),
            is_variable: false,
            sparsity: None,
            shape_signature: None,
            has_rank: false,
            variant_tensors: None,
        },
    );
    (tensor, buffer_index)
}

/// An int32 constant tensor (bias): per-channel bias scales, zero points 0.
/// Returns the tensor and its buffer index.
fn i32_tensor<'b>(
    fbb: &mut FlatBufferBuilder<'b>,
    buffers: &mut Vec<WIPOffset<Buffer<'b>>>,
    bias_scales: &[f32],
    shape: &[usize],
    name: &str,
    data: &[i32],
) -> (WIPOffset<Tensor<'b>>, u32) {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for v in data {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let buffer_index = push_buffer(fbb, buffers, &bytes);
    let zero_points = vec![0i64; bias_scales.len()];
    let quant_offset = quant_table(fbb, bias_scales, &zero_points);
    let name = fbb.create_string(name);
    let shape: Vec<i32> = shape.iter().map(|&d| d as i32).collect();
    let shape = fbb.create_vector(&shape);
    let tensor = Tensor::create(
        fbb,
        &TensorArgs {
            shape: Some(shape),
            type_: TensorType::INT32,
            buffer: buffer_index,
            name: Some(name),
            quantization: Some(quant_offset),
            is_variable: false,
            sparsity: None,
            shape_signature: None,
            has_rank: false,
            variant_tensors: None,
        },
    );
    (tensor, buffer_index)
}
