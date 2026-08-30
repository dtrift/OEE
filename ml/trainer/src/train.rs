//! The manual training loop (track D4): burn's Adam, no Learner
//! infrastructure — batches are sliced in memory (the dataset is tiny).
//!
//! Determinism: `B::seed(SEED)` before init and before the epoch shuffles;
//! the same command on the same data must produce bit-identical weights
//! (the D6 double-run sha256 gate).

use burn::nn::loss::{CrossEntropyLoss, CrossEntropyLossConfig};
use burn::optim::{AdamConfig, GradientsParams, Optimizer};
use burn::tensor::backend::AutodiffBackend;
use burn::tensor::{ElementConversion, Int, Tensor};
use rand::seq::SliceRandom;
use rand::SeedableRng;

use crate::data::Window;
use crate::model::{windows_to_tensor, ModelA};

/// Trains and returns the fitted model.
///
/// # Arguments
/// * `epochs`, `batch_size`, `lr` — the py script's defaults (30 / 64 / 1e-3)
#[allow(clippy::too_many_arguments)]
pub fn train<B: AutodiffBackend>(
    device: &B::Device,
    train: &[Window],
    epochs: usize,
    batch_size: usize,
    lr: f64,
    class_weights: [f32; crate::NUM_CLASSES],
) -> ModelA<B> {
    B::seed(device, crate::SEED);
    let mut model = ModelA::<B>::init(device);
    let mut optimizer = AdamConfig::new().with_beta_1(0.9).with_beta_2(0.999).init();
    let loss_fn: CrossEntropyLoss<B> = CrossEntropyLossConfig::new()
        .with_weights(Some(class_weights.to_vec()))
        .init(device);

    let mut labels: Vec<usize> = train.iter().map(|w| w.label).collect();
    let mut order: Vec<usize> = (0..train.len()).collect();
    let mut rng = rand::rngs::StdRng::seed_from_u64(crate::SEED);

    for epoch in 0..epochs {
        order.shuffle(&mut rng);
        let mut total_loss = 0.0f32;
        let mut batches = 0usize;
        for chunk in order.chunks(batch_size) {
            let rows: Vec<Vec<f32>> = chunk.iter().map(|&i| train[i].values.clone()).collect();
            let x = windows_to_tensor::<B>(&rows, device);
            let y = Tensor::<B, 1, Int>::from_data(
                burn::tensor::TensorData::new(
                    chunk.iter().map(|&i| labels[i] as i64).collect::<Vec<_>>(),
                    [chunk.len()],
                ),
                device,
            );
            let logits = model.forward(x);
            let loss = loss_fn.forward(logits, y);
            let grads = loss.backward();
            let grads = GradientsParams::from_grads(grads, &model);
            model = optimizer.step(lr, model, grads);
            total_loss += loss.into_scalar().elem::<f32>();
            batches += 1;
        }
        println!(
            "epoch {}/{}: loss {:.4}",
            epoch + 1,
            epochs,
            total_loss / batches.max(1) as f32
        );
    }
    let _ = &mut labels;
    model
}
