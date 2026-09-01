//! The burn model A (track D4): `Conv1d(8,k=3) → ReLU → AvgPool(2) →
//! Conv1d(16,k=3) → ReLU → AvgPool(2) → Flatten → Linear(4)` on logits —
//! softmax is NOT part of the trainable graph (it is exported as the SOFTMAX
//! operator). A separate `forward_softmax` runs the full float inference for
//! parity checks.
//!
//! Layout facts pinned here (D4.2, the "subtle bugs" place):
//! - burn works channel-first `[B, C, L]`; the forward **transposes before
//!   flattening** so the FC input order is the TFLite row-major `(T, F)` —
//!   then `Linear [Out, In]` maps to the FC weights `[Out, In]` with **no**
//!   permutation at export;
//! - burn `Conv1d` weights are `[F, C, k]` → the exporter permutes the last
//!   two axes into the file's OHWI `[F, 1, k, C]`.

use burn::module::Module;
use burn::nn::conv::{Conv1d, Conv1dConfig};
use burn::nn::pool::{AvgPool1d, AvgPool1dConfig};
use burn::nn::{Linear, LinearConfig, PaddingConfig1d, Relu};
use burn::tensor::backend::Backend;
use burn::tensor::{Tensor, TensorData};

use crate::{CHANNELS, NUM_CLASSES, TIMESTEPS};

/// The trainable model (logits head).
#[derive(Module, Debug)]
pub struct ModelA<B: Backend> {
    conv1: Conv1d<B>,
    relu1: Relu,
    pool1: AvgPool1d,
    conv2: Conv1d<B>,
    relu2: Relu,
    pool2: AvgPool1d,
    fc: Linear<B>,
}

impl<B: Backend> ModelA<B> {
    pub fn init(device: &B::Device) -> Self {
        Self {
            conv1: Conv1dConfig::new(CHANNELS, 8, 3)
                .with_padding(PaddingConfig1d::Valid)
                .init(device),
            relu1: Relu::new(),
            pool1: AvgPool1dConfig::new(2).with_stride(2).init(),
            conv2: Conv1dConfig::new(8, 16, 3)
                .with_padding(PaddingConfig1d::Valid)
                .init(device),
            relu2: Relu::new(),
            pool2: AvgPool1dConfig::new(2).with_stride(2).init(),
            fc: LinearConfig::new(30 * 16, NUM_CLASSES).init(device),
        }
    }

    /// Logits `[B, NUM_CLASSES]` from windows `[B, C=1, T=128]`.
    pub fn forward(&self, x: Tensor<B, 3>) -> Tensor<B, 2> {
        let x = self.conv1.forward(x); // [B, 8, 126]
        let x = self.relu1.forward(x);
        let x = self.pool1.forward(x); // [B, 8, 63]
        let x = self.conv2.forward(x); // [B, 16, 61]
        let x = self.relu2.forward(x);
        let x = self.pool2.forward(x); // [B, 16, 30]
                                       // Channel-first → time-first, so flatten() reads (T, F) row-major —
                                       // the TFLite FC input order (see the module docs).
        self.fc.forward(x.swap_dims(1, 2).flatten(1, 2)) // [B, 4]
    }

    /// Softmax probabilities `[B, NUM_CLASSES]` (float parity / evaluation).
    pub fn forward_softmax(&self, x: Tensor<B, 3>) -> Tensor<B, 2> {
        softmax(self.forward(x))
    }
}

fn softmax<B: Backend>(logits: Tensor<B, 2>) -> Tensor<B, 2> {
    let max = logits.clone().max_dim(1).unsqueeze();
    let exps = logits.sub(max).exp();
    let sum = exps.clone().sum_dim(1);
    exps.div(sum)
}

/// Extracts the trained weights as the exporter's float model (D4.5): burn
/// `Conv1d` weights arrive `[F, C, k]`, `Linear` weights `[Out, In]`.
pub fn to_float_model<B: Backend>(model: &ModelA<B>) -> exporter::quant::FloatModel {
    exporter::quant::FloatModel {
        timesteps: TIMESTEPS,
        channels: CHANNELS,
        conv1: exporter::quant::FloatConv {
            filters: 8,
            kernel: 3,
            weights: to_vec_f32(&model.conv1.weight.val().into_data()),
            bias: model
                .conv1
                .bias
                .as_ref()
                .map(|b| to_vec_f32(&b.val().into_data()))
                .unwrap_or_else(|| vec![0.0; 8]),
        },
        pool1: 2,
        conv2: exporter::quant::FloatConv {
            filters: 16,
            kernel: 3,
            weights: to_vec_f32(&model.conv2.weight.val().into_data()),
            bias: model
                .conv2
                .bias
                .as_ref()
                .map(|b| to_vec_f32(&b.val().into_data()))
                .unwrap_or_else(|| vec![0.0; 16]),
        },
        pool2: 2,
        fc: exporter::quant::FloatFc {
            out_units: NUM_CLASSES,
            in_units: 30 * 16,
            // burn 0.21 Linear stores [d_input, d_output]; the TFLite FC
            // weights are [Out, In] (F6) — transpose at the export seam.
            weights: {
                let burn_w = to_vec_f32(&model.fc.weight.val().into_data());
                let (inp, out) = (30 * 16, NUM_CLASSES);
                (0..out * inp)
                    .map(|idx| {
                        let (j, i) = (idx / inp, idx % inp);
                        burn_w[i * out + j]
                    })
                    .collect()
            },
            bias: model
                .fc
                .bias
                .as_ref()
                .map(|b| to_vec_f32(&b.val().into_data()))
                .unwrap_or_else(|| vec![0.0; NUM_CLASSES]),
        },
    }
}

/// `TensorData` → `Vec<f32>` (row-major, burn's memory order).
pub fn to_vec_f32(data: &TensorData) -> Vec<f32> {
    data.iter::<f32>().collect()
}

/// Windows `[B, C=1, T]` from flat `(t, c)` row-major rows.
pub fn windows_to_tensor<B: Backend>(rows: &[Vec<f32>], device: &B::Device) -> Tensor<B, 3> {
    let batch = rows.len();
    let values: Vec<f32> = rows.iter().flatten().copied().collect();
    Tensor::from_data(
        TensorData::new(values, [batch, CHANNELS, TIMESTEPS]),
        device,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::ndarray::NdArray;
    use burn::tensor::backend::Backend;
    use exporter::quant::FloatConv;

    type TestBackend = NdArray;

    /// Ground truth probe: a Constant(1) conv over a known input — burns
    /// exact conv semantics, no ambiguity from random weights.
    #[test]
    fn conv1d_constant_weights_semantics() {
        use burn::nn::conv::Conv1dConfig;
        use burn::nn::Initializer;
        let device = <TestBackend as burn::tensor::backend::BackendTypes>::Device::default();
        let conv = Conv1dConfig::new(2, 1, 2)
            .with_initializer(Initializer::Constant { value: 1.0 })
            .with_bias(false)
            .init::<TestBackend>(&device);
        // Input [1, C=2, L=3]: c0 = [1, 2, 3], c1 = [10, 20, 30].
        let input = Tensor::<TestBackend, 3>::from_data(
            TensorData::new(vec![1.0f32, 2.0, 3.0, 10.0, 20.0, 30.0], [1, 2, 3]),
            &device,
        );
        let out: Vec<f32> = conv.forward(input).into_data().iter::<f32>().collect();
        println!("constant-conv out: {out:?}");
        // Cross-correlation, w[f][c][k] = 1: out[t] = (1+2)+(10+20) = 33,
        // (2+3)+(20+30) = 55 → [33, 55].
        assert_eq!(out, vec![33.0, 55.0]);
    }

    #[test]
    fn avgpool_semantics() {
        use burn::nn::pool::AvgPool1dConfig;
        let device = <TestBackend as burn::tensor::backend::BackendTypes>::Device::default();
        let pool = AvgPool1dConfig::new(2).with_stride(2).init();
        // [1, 1, 5] = [1, 2, 3, 4, 5]: valid p=2 s=2 → floor((5-2)/2)+1 = 2 → [1.5, 3.5].
        let input = Tensor::<TestBackend, 3>::from_data(
            TensorData::new(vec![1.0f32, 2.0, 3.0, 4.0, 5.0], [1, 1, 5]),
            &device,
        );
        let out: Vec<f32> = pool.forward(input).into_data().iter::<f32>().collect();
        println!("pool out: {out:?}");
        assert_eq!(out, vec![1.5, 3.5]);
    }

    #[test]
    fn linear_semantics() {
        use burn::module::Param;
        use burn::nn::LinearConfig;
        let device = <TestBackend as burn::tensor::backend::BackendTypes>::Device::default();
        let mut linear = LinearConfig::new(2, 2).init::<TestBackend>(&device);
        // burn 0.21: the weight is [d_input, d_output]; w = [[1, 2], [3, 4]],
        // b = [10, 20]; x = [1, 2] → y0 = 1·1 + 2·3 + 10 = 17,
        // y1 = 1·2 + 2·4 + 20 = 30 (O = IW — the export seam must transpose).
        let w: Tensor<TestBackend, 2> = Tensor::from_data(
            TensorData::new(vec![1.0f32, 2.0, 3.0, 4.0], [2, 2]),
            &device,
        );
        let b: Tensor<TestBackend, 1> =
            Tensor::from_data(TensorData::new(vec![10.0f32, 20.0], [2]), &device);
        let id = linear.weight.id;
        let bias_id = linear.bias.as_ref().unwrap().id;
        linear.weight = Param::initialized(id, w);
        linear.bias = Some(Param::initialized(bias_id, b));
        let x = Tensor::<TestBackend, 2>::from_data(
            TensorData::new(vec![1.0f32, 2.0], [1, 2]),
            &device,
        );
        let out: Vec<f32> = linear.forward(x).into_data().iter::<f32>().collect();
        println!("linear out: {out:?}");
        assert_eq!(out, vec![17.0, 30.0]);
    }

    #[test]
    fn tensor_data_iteration_is_row_major() {
        let device = <TestBackend as burn::tensor::backend::BackendTypes>::Device::default();
        let t = Tensor::<TestBackend, 2>::from_data(
            TensorData::new(vec![1.0f32, 2.0, 3.0, 4.0], [2, 2]),
            &device,
        );
        let got: Vec<f32> = t.into_data().iter::<f32>().collect();
        assert_eq!(got, vec![1.0, 2.0, 3.0, 4.0]);
    }

    /// The layout contract (D4.2): the exporter's float forward (quant.rs's
    /// calibration math and debug tooling) must reproduce burn's forward on
    /// the same weights. Any divergence here corrupts PTQ calibration.
    #[test]
    fn burn_forward_matches_the_export_layout() {
        let device = <TestBackend as burn::tensor::backend::BackendTypes>::Device::default();
        TestBackend::seed(&device, 2026);
        let model = ModelA::<TestBackend>::init(&device);
        let float = to_float_model(&model);

        // A deterministic window with real variation.
        let window: Vec<f32> = (0..TIMESTEPS)
            .map(|t| {
                let ts = t as f32 / 1600.0;
                0.8 * (2.0 * core::f32::consts::PI * 50.0 * ts).sin()
                    + 0.2 * (2.0 * core::f32::consts::PI * 150.0 * ts).sin()
            })
            .collect();

        // burn side.
        let burn_logits: Vec<f32> = model
            .forward(windows_to_tensor::<TestBackend>(
                std::slice::from_ref(&window),
                &device,
            ))
            .into_data()
            .iter::<f32>()
            .collect();

        // The reference float chain (same math as quant.rs's calibration).
        let conv = |c: &FloatConv, x: &[f32], t: usize, ch: usize| {
            let out_t = t - c.kernel + 1;
            let mut out = vec![0.0f32; out_t * c.filters];
            for ti in 0..out_t {
                for f in 0..c.filters {
                    let mut acc = c.bias[f];
                    for ki in 0..c.kernel {
                        for ci in 0..ch {
                            acc += x[(ti + ki) * ch + ci]
                                * c.weights[f * ch * c.kernel + ci * c.kernel + ki];
                        }
                    }
                    out[ti * c.filters + f] = acc.max(0.0);
                }
            }
            out
        };
        let pool = |x: &[f32], t: usize, f: usize, p: usize| {
            let out_t = (t - p) / p + 1;
            (0..out_t * f)
                .map(|i| {
                    let (ti, fi) = (i / f, i % f);
                    let mut sum = 0.0;
                    for j in 0..p {
                        sum += x[(ti * p + j) * f + fi];
                    }
                    sum / p as f32
                })
                .collect::<Vec<_>>()
        };
        let c1 = conv(&float.conv1, &window, TIMESTEPS, CHANNELS);
        let p1 = pool(&c1, 126, 8, 2);
        let c2 = conv(&float.conv2, &p1, 63, 8);
        let flat = pool(&c2, 61, 16, 2);
        let logits: Vec<f32> = (0..NUM_CLASSES)
            .map(|j| {
                let mut acc = float.fc.bias[j];
                for (i, &v) in flat.iter().enumerate() {
                    acc += v * float.fc.weights[j * float.fc.in_units + i];
                }
                acc
            })
            .collect();

        println!("burn  {burn_logits:?}");
        println!("ref   {logits:?}");
        for (a, b) in burn_logits.iter().zip(&logits) {
            assert!((a - b).abs() < 1e-3, "burn {a} vs reference {b}");
        }
    }
}
