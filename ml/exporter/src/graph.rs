//! A typed, validating builder of the minimal Conv1D graph (track D1).
//!
//! The graph is exactly the six real operators of the week-3 spike model —
//! `CONV_2D → AVERAGE_POOL_2D → CONV_2D → AVERAGE_POOL_2D → FULLY_CONNECTED →
//! SOFTMAX` — with **no** EXPAND_DIMS/RESHAPE/Flatten wrappers: the microflow
//! parser normalizes rank-3 inputs to `(1, 1, T, C)` and unfolds FC inputs to
//! `(1, N)` itself (`fork/docs/conv1d-spec.md` §2.1/§2.2), which this writer
//! relies on deliberately (a live test of the parser's generality).
//!
//! Every `add_*` step knows its input shape and validates the chain, failing
//! with an operator-context message in the spirit of the fork's parser
//! ("Conv1D (op 1): filters expect 8 channels, the input has 1").

/// Quantization parameters of one tensor (scales + zero points, per the TFLite
/// schema: `zero_point` is an i64 vector; per-tensor = one entry).
#[derive(Clone, Debug, PartialEq)]
pub struct TensorQuant {
    pub scale: Vec<f32>,
    pub zero_point: Vec<i64>,
}

impl TensorQuant {
    /// Per-tensor parameters (activations, F7).
    pub fn per_tensor(scale: f32, zero_point: i64) -> Self {
        Self {
            scale: vec![scale],
            zero_point: vec![zero_point],
        }
    }

    /// Symmetric per-channel parameters (weights, F5): zero points all 0.
    pub fn symmetric(scales: Vec<f32>) -> Self {
        let zero_point = vec![0; scales.len()];
        Self {
            scale: scales,
            zero_point,
        }
    }
}

/// A quantized Conv1D layer (file-level layout).
#[derive(Clone, Debug)]
pub struct Conv1d {
    /// Output length after the valid convolution.
    pub out_len: usize,
    /// Filters `F`.
    pub filters: usize,
    /// Kernel size `k`.
    pub kernel: usize,
    /// Input channels `C`.
    pub in_chans: usize,
    /// OHWI row-major weights `[F, 1, k, C]`, int8.
    pub weights: Vec<i8>,
    /// Per-channel weight scales (F entries, zp = 0, F5).
    pub weight_scales: Vec<f32>,
    /// int32 bias in accumulator units (F entries).
    pub bias: Vec<i32>,
    /// Per-channel bias scales, `scale_x * scale_w[f]` (the TFLite convention
    /// that makes the macro's accumulator conversion an identity).
    pub bias_scales: Vec<f32>,
    /// Output activation quantization (per-tensor).
    pub out_quant: TensorQuant,
    /// RELU fused into the operator (the TFLite CONV_2D options way).
    pub fused_relu: bool,
}

/// A quantized average-pool layer (file-level layout).
#[derive(Clone, Debug)]
pub struct AvgPool1d {
    /// Pool size `p` (stride `p` too, the Keras `AveragePooling1D` default).
    pub pool: usize,
    /// Output length after valid pooling.
    pub out_len: usize,
    /// Output activation quantization. The writer's convention copies the
    /// input's: the pool then requantizes with ratio 1 (like the TF-converted
    /// file, where pools share the convolution's scale).
    pub out_quant: TensorQuant,
}

/// A quantized fully-connected layer (file-level layout).
#[derive(Clone, Debug)]
pub struct Fc {
    /// Output units.
    pub out_units: usize,
    /// Input units (the flattened `(T, F)` product).
    pub in_units: usize,
    /// Row-major weights `[Out, In]`, int8 (F6).
    pub weights: Vec<i8>,
    /// Per-channel weight scales (Out entries, zp = 0, F6).
    pub weight_scales: Vec<f32>,
    /// int32 bias in accumulator units (Out entries).
    pub bias: Vec<i32>,
    /// Per-channel bias scales, `scale_x * scale_w[j]`.
    pub bias_scales: Vec<f32>,
    /// Output activation quantization (per-tensor).
    pub out_quant: TensorQuant,
}

/// The softmax operator (quantization on the output tensor; the kernel reads
/// only the output's scale/zero point).
#[derive(Clone, Debug)]
pub struct Softmax {
    pub out_quant: TensorQuant,
}

/// One real operator of the minimal graph.
#[derive(Clone, Debug)]
pub enum Layer {
    Conv1d(Conv1d),
    AvgPool1d(AvgPool1d),
    Fc(Fc),
    Softmax(Softmax),
}

/// The built graph: a rank-3 input + the operator chain.
#[derive(Clone, Debug)]
pub struct ModelGraph {
    /// Input shape `(1, T, C)` as declared in the file.
    pub input_shape: [usize; 3],
    pub input_quant: TensorQuant,
    pub layers: Vec<Layer>,
}

/// Build-time shape state, mirroring the parser's normalization (§2.2):
/// convolutions work on the effective rank-4 `(1, 1, T, C)`, FC/softmax on
/// the effective rank-2 `(1, N)`.
#[derive(Clone, Copy, Debug)]
enum Cursor {
    /// Effective `(1, 1, T, C)`.
    Conv { timesteps: usize, chans: usize },
    /// Effective `(1, N)` (after FC/softmax, or a folded flatten).
    Flat { units: usize },
}

/// The validating builder (track D1).
pub struct GraphBuilder {
    input_shape: Option<[usize; 3]>,
    input_quant: Option<TensorQuant>,
    cursor: Option<Cursor>,
    layers: Vec<Layer>,
}

impl Default for GraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self {
            input_shape: None,
            input_quant: None,
            cursor: None,
            layers: Vec::new(),
        }
    }

    fn op_index(&self) -> usize {
        self.layers.len()
    }

    /// Declares the rank-3 input `(1, T, C)` (the Keras Conv1D serialization,
    /// F1). Internally normalized to `(1, 1, T, C)` like the parser does.
    pub fn add_input(
        &mut self,
        shape: [usize; 3],
        quant: TensorQuant,
    ) -> Result<&mut Self, String> {
        let op = "input";
        if shape[0] != 1 {
            return Err(format!(
                "{op}: the batch dimension must be 1, got {shape:?}"
            ));
        }
        if shape[1] == 0 || shape[2] == 0 {
            return Err(format!("{op}: zero-sized input {shape:?}"));
        }
        if quant.scale.len() != 1 || quant.zero_point.len() != 1 {
            return Err(format!(
                "{op}: per-tensor quantization expected (1 scale/zero point), got {} entries",
                quant.scale.len()
            ));
        }
        self.input_shape = Some(shape);
        self.input_quant = Some(quant);
        self.cursor = Some(Cursor::Conv {
            timesteps: shape[1],
            chans: shape[2],
        });
        Ok(self)
    }

    /// Appends a quantized Conv1D (valid padding, stride 1 — the model A
    /// geometry). `weights` are OHWI row-major `[F, 1, k, C]`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_conv_1d(
        &mut self,
        filters: usize,
        kernel: usize,
        weights: Vec<i8>,
        weight_scales: Vec<f32>,
        bias: Vec<i32>,
        bias_scales: Vec<f32>,
        out_quant: TensorQuant,
        fused_relu: bool,
    ) -> Result<&mut Self, String> {
        let index = self.op_index();
        let op = format!("Conv1D (op {index})");
        let (timesteps, chans) = match self.cursor {
            Some(Cursor::Conv { timesteps, chans }) => (timesteps, chans),
            Some(Cursor::Flat { units }) => {
                return Err(format!(
                    "{op}: the input is flat ({units} units), a rank-4 (1, 1, T, C) tensor was \
                     expected — convolutions must come before the fully-connected layer"
                ))
            }
            None => return Err(format!("{op}: no input declared yet")),
        };
        if filters == 0 || kernel == 0 {
            return Err(format!(
                "{op}: zero-sized convolution (filters {filters}, kernel {kernel})"
            ));
        }
        if timesteps < kernel {
            return Err(format!(
                "{op}: T ({timesteps}) < kernel ({kernel}) with Valid padding would produce an \
                 empty output, which is forbidden"
            ));
        }
        let expected = filters * kernel * chans;
        if weights.len() != expected {
            return Err(format!(
                "{op}: filters ({filters}, 1, {kernel}, {chans}) hold {expected} values, the \
                 weights carry {}",
                weights.len()
            ));
        }
        if weight_scales.len() != filters {
            return Err(format!(
                "{op}: per-channel weights need {filters} scales, got {}",
                weight_scales.len()
            ));
        }
        if bias.len() != filters || bias_scales.len() != filters {
            return Err(format!(
                "{op}: the bias needs {filters} values and scales, got {} and {}",
                bias.len(),
                bias_scales.len()
            ));
        }
        if out_quant.scale.len() != 1 {
            return Err(format!(
                "{op}: per-tensor output quantization expected, got {} scales",
                out_quant.scale.len()
            ));
        }
        let out_len = timesteps - kernel + 1;
        self.layers.push(Layer::Conv1d(Conv1d {
            out_len,
            filters,
            kernel,
            in_chans: chans,
            weights,
            weight_scales,
            bias,
            bias_scales,
            out_quant,
            fused_relu,
        }));
        self.cursor = Some(Cursor::Conv {
            timesteps: out_len,
            chans: filters,
        });
        Ok(self)
    }

    /// Appends an average pool (valid padding, filter `(1, p)`, stride `(1, p)`).
    pub fn add_avg_pool(
        &mut self,
        pool: usize,
        out_quant: TensorQuant,
    ) -> Result<&mut Self, String> {
        let index = self.op_index();
        let op = format!("AvgPool1D (op {index})");
        let (timesteps, chans) = match self.cursor {
            Some(Cursor::Conv { timesteps, chans }) => (timesteps, chans),
            Some(Cursor::Flat { units }) => {
                return Err(format!(
                    "{op}: the input is flat ({units} units), a rank-4 tensor was expected"
                ))
            }
            None => return Err(format!("{op}: no input declared yet")),
        };
        if pool == 0 {
            return Err(format!("{op}: pool size must be positive"));
        }
        let out_len = (timesteps.saturating_sub(pool)) / pool + 1;
        if timesteps < pool || out_len == 0 {
            return Err(format!(
                "{op}: T ({timesteps}) < pool ({pool}) with Valid padding would produce an \
                 empty output, which is forbidden"
            ));
        }
        if out_quant.scale.len() != 1 {
            return Err(format!(
                "{op}: per-tensor output quantization expected, got {} scales",
                out_quant.scale.len()
            ));
        }
        self.layers.push(Layer::AvgPool1d(AvgPool1d {
            pool,
            out_len,
            out_quant,
        }));
        self.cursor = Some(Cursor::Conv {
            timesteps: out_len,
            chans,
        });
        Ok(self)
    }

    /// Appends a fully-connected layer; the input is the flattened
    /// `(T, F)` product of the current conv cursor (or a previous flat
    /// cursor — the parser unfolds rank > 2 inputs to `(1, N)` itself, §2.1).
    #[allow(clippy::too_many_arguments)]
    pub fn add_fc(
        &mut self,
        out_units: usize,
        weights: Vec<i8>,
        weight_scales: Vec<f32>,
        bias: Vec<i32>,
        bias_scales: Vec<f32>,
        out_quant: TensorQuant,
    ) -> Result<&mut Self, String> {
        let index = self.op_index();
        let op = format!("FullyConnected (op {index})");
        let in_units = match self.cursor {
            Some(Cursor::Conv { timesteps, chans }) => timesteps * chans,
            Some(Cursor::Flat { units }) => units,
            None => return Err(format!("{op}: no input declared yet")),
        };
        if out_units == 0 {
            return Err(format!("{op}: zero output units"));
        }
        let expected = out_units * in_units;
        if weights.len() != expected {
            return Err(format!(
                "{op}: weights ({out_units}, {in_units}) hold {expected} values, the buffer \
                 carries {}",
                weights.len()
            ));
        }
        if weight_scales.len() != out_units {
            return Err(format!(
                "{op}: per-channel weights need {out_units} scales, got {}",
                weight_scales.len()
            ));
        }
        if bias.len() != out_units || bias_scales.len() != out_units {
            return Err(format!(
                "{op}: the bias needs {out_units} values and scales, got {} and {}",
                bias.len(),
                bias_scales.len()
            ));
        }
        if out_quant.scale.len() != 1 {
            return Err(format!(
                "{op}: per-tensor output quantization expected, got {} scales",
                out_quant.scale.len()
            ));
        }
        self.layers.push(Layer::Fc(Fc {
            out_units,
            in_units,
            weights,
            weight_scales,
            bias,
            bias_scales,
            out_quant,
        }));
        self.cursor = Some(Cursor::Flat { units: out_units });
        Ok(self)
    }

    /// Appends the softmax operator (the export of the trained logits head).
    pub fn add_softmax(&mut self, out_quant: TensorQuant) -> Result<&mut Self, String> {
        let index = self.op_index();
        let op = format!("Softmax (op {index})");
        match self.cursor {
            Some(Cursor::Flat { .. }) => {}
            Some(Cursor::Conv { timesteps, chans }) => {
                return Err(format!(
                    "{op}: the input is rank-4 (1, 1, {timesteps}, {chans}), softmax expects a \
                     flat one"
                ))
            }
            None => return Err(format!("{op}: no input declared yet")),
        }
        if out_quant.scale.len() != 1 {
            return Err(format!(
                "{op}: per-tensor output quantization expected, got {} scales",
                out_quant.scale.len()
            ));
        }
        self.layers.push(Layer::Softmax(Softmax { out_quant }));
        Ok(self)
    }

    /// Freezes the graph.
    pub fn build(self) -> Result<ModelGraph, String> {
        let input_shape = self.input_shape.ok_or("the graph has no input declared")?;
        match self.layers.last() {
            Some(Layer::Softmax(_)) => {}
            Some(last) => {
                return Err(format!(
                    "the graph must end with softmax (the quantized probabilities), got {:?}",
                    kind_of(last)
                ))
            }
            None => return Err("the graph has no operators".into()),
        }
        Ok(ModelGraph {
            input_shape,
            input_quant: self.input_quant.expect("checked in add_input"),
            layers: self.layers,
        })
    }
}

fn kind_of(layer: &Layer) -> &'static str {
    match layer {
        Layer::Conv1d(_) => "Conv1d",
        Layer::AvgPool1d(_) => "AvgPool1d",
        Layer::Fc(_) => "Fc",
        Layer::Softmax(_) => "Softmax",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quant(scale: f32, zp: i64) -> TensorQuant {
        TensorQuant::per_tensor(scale, zp)
    }

    #[test]
    fn builds_the_six_operator_chain() {
        let mut b = GraphBuilder::new();
        b.add_input([1, 128, 1], quant(0.01, 1)).unwrap();
        b.add_conv_1d(
            8,
            3,
            vec![0; 8 * 3],
            vec![0.02; 8],
            vec![0; 8],
            vec![0.0002; 8],
            quant(0.004, -128),
            true,
        )
        .unwrap()
        .add_avg_pool(2, quant(0.004, -128))
        .unwrap()
        .add_conv_1d(
            16,
            3,
            vec![0; 16 * 3 * 8],
            vec![0.01; 16],
            vec![0; 16],
            vec![0.00004; 16],
            quant(0.003, -128),
            true,
        )
        .unwrap()
        .add_avg_pool(2, quant(0.003, -128))
        .unwrap()
        .add_fc(
            4,
            vec![0; 4 * 480],
            vec![0.05; 4],
            vec![0; 4],
            vec![0.00015; 4],
            quant(0.0024, -40),
        )
        .unwrap()
        .add_softmax(quant(1.0 / 256.0, -128))
        .unwrap();
        let graph = b.build().unwrap();
        assert_eq!(graph.input_shape, [1, 128, 1]);
        assert_eq!(graph.layers.len(), 6);
        let Layer::Conv1d(ref c) = graph.layers[0] else {
            panic!("first layer must be a convolution");
        };
        assert_eq!(c.out_len, 126);
        let Layer::AvgPool1d(ref p) = graph.layers[1] else {
            panic!("second layer must be a pool");
        };
        assert_eq!(p.out_len, 63);
        let Layer::Conv1d(ref c) = graph.layers[2] else {
            panic!("third layer must be a convolution");
        };
        assert_eq!(c.out_len, 61);
        let Layer::AvgPool1d(ref p) = graph.layers[3] else {
            panic!("fourth layer must be a pool");
        };
        assert_eq!(p.out_len, 30);
        let Layer::Fc(ref f) = graph.layers[4] else {
            panic!("fifth layer must be fully connected");
        };
        assert_eq!(f.in_units, 480);
    }

    #[test]
    fn conv_channel_mismatch_is_an_error_with_context() {
        let mut b = GraphBuilder::new();
        b.add_input([1, 128, 1], quant(0.01, 1)).unwrap();
        let err = match b.add_conv_1d(
            8,
            3,
            vec![0; 8 * 3 * 2],
            vec![0.02; 8],
            vec![0; 8],
            vec![0.0002; 8],
            quant(0.004, -128),
            true,
        ) {
            Err(e) => e,
            Ok(_) => panic!("the channel mismatch must fail"),
        };
        assert!(
            err.contains("Conv1D (op 0)") && err.contains("24 values, the weights carry 48"),
            "error must carry the operator context: {err}"
        );
    }

    #[test]
    fn kernel_larger_than_input_is_rejected() {
        let mut b = GraphBuilder::new();
        b.add_input([1, 2, 1], quant(0.01, 1)).unwrap();
        let err = match b.add_conv_1d(
            1,
            3,
            vec![0; 3],
            vec![0.1],
            vec![0],
            vec![0.001],
            quant(0.1, 0),
            true,
        ) {
            Err(e) => e,
            Ok(_) => panic!("T < kernel must fail"),
        };
        assert!(err.contains("T (2) < kernel (3)"), "{err}");
    }

    #[test]
    fn graph_must_end_with_softmax() {
        let mut b = GraphBuilder::new();
        b.add_input([1, 8, 1], quant(0.01, 1)).unwrap();
        b.add_conv_1d(
            2,
            3,
            vec![0; 6],
            vec![0.1; 2],
            vec![0; 2],
            vec![0.001; 2],
            quant(0.1, 0),
            true,
        )
        .unwrap();
        let err = b.build().unwrap_err();
        assert!(err.contains("softmax"), "{err}");
    }
}
