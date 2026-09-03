//! The one-command pipeline (track D5): train → export float weights →
//! calibrate on the train split → PTQ → write `.tflite` → interp self-check.
//!
//! ```text
//! cargo run -p trainer --release --bin train -- \
//!     --datasets tmp/ds_base_1.csv ... --calib 256 \
//!     --out ml/models/model_a.tflite
//! ```

use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::data::{calibration_windows, class_weights, load_datasets, split, write_val_csv};
use crate::train::train;
use crate::TaskSpec;

pub struct PipelineArgs {
    pub datasets: Vec<PathBuf>,
    pub epochs: usize,
    pub calib: usize,
    pub out: PathBuf,
    pub float_out: PathBuf,
    pub val_csv: PathBuf,
}

pub struct PipelineReport {
    pub float_val_accuracy: f32,
    pub int8_val_accuracy: f32,
    pub confusion: Vec<Vec<usize>>,
    pub sha256: String,
    pub val_windows: usize,
}

pub fn run<B: burn::tensor::backend::AutodiffBackend>(
    spec: &TaskSpec,
    args: PipelineArgs,
) -> anyhow::Result<PipelineReport> {
    let windows = load_datasets(spec, &args.datasets).map_err(anyhow::Error::msg)?;
    let counts = class_counts(spec, &windows);
    println!("dataset: {} windows, classes {:?}", windows.len(), counts);
    let (train_windows, val) = split(spec, &windows);
    println!(
        "split: train {} ({:?}), val {} ({:?})",
        train_windows.len(),
        class_counts(spec, &train_windows),
        val.len(),
        class_counts(spec, &val)
    );
    write_val_csv(spec, &val, &args.val_csv).map_err(anyhow::Error::msg)?;

    let device = B::Device::default();
    let weights = class_weights(
        spec,
        &train_windows.iter().map(|w| w.label).collect::<Vec<_>>(),
    );
    let model = train::<B>(
        &device,
        spec,
        &train_windows,
        args.epochs,
        64,
        1e-3,
        weights,
    );

    let float_model = crate::model::to_float_model(spec, &model);
    exporter::weights::write_float_model(&float_model, &args.float_out)
        .map_err(anyhow::Error::msg)?;
    println!("float weights: {}", args.float_out.display());

    let calib = calibration_windows(&train_windows, args.calib);
    let graph = exporter::quant::quantize(&float_model, &calib).map_err(anyhow::Error::msg)?;
    let bytes = exporter::writer::write(&graph);
    std::fs::write(&args.out, &bytes).context("writing the .tflite")?;
    println!("int8 model: {} ({} bytes)", args.out.display(), bytes.len());
    let dump = exporter::dumper::dump_bytes(&bytes).map_err(anyhow::Error::msg)?;
    let dump_path = args.out.with_extension("ops.txt");
    std::fs::write(&dump_path, dump).context("writing the ops dump")?;
    println!("ops dump: {}", dump_path.display());

    // Self-check: the interp reference over the quantized file on val windows.
    let interp =
        exporter::interp::InterpModel::from_bytes(bytes.clone()).map_err(anyhow::Error::msg)?;
    let (float_acc, int8_acc, confusion) = evaluate::<B>(spec, &interp, &model, &val, &device);
    println!("float (burn) val accuracy: {float_acc:.4}");
    println!("int8 (interp) val accuracy: {int8_acc:.4}");
    print_confusion(spec, &confusion);

    let sha = sha256_hex(&bytes);
    println!("sha256: {sha}");
    Ok(PipelineReport {
        float_val_accuracy: float_acc,
        int8_val_accuracy: int8_acc,
        confusion,
        sha256: sha,
        val_windows: val.len(),
    })
}

/// Evaluates both heads on the val split: the burn float model and the
/// quantized model through the interp reference.
fn evaluate<B: burn::tensor::backend::Backend>(
    spec: &TaskSpec,
    interp: &exporter::interp::InterpModel,
    model: &crate::model::ModelCnn<B>,
    val: &[crate::data::Window],
    device: &B::Device,
) -> (f32, f32, Vec<Vec<usize>>) {
    let rows: Vec<Vec<f32>> = val.iter().map(|w| w.values.clone()).collect();
    let probs = model.forward_softmax(crate::model::windows_to_tensor::<B>(spec, &rows, device));
    let float_probs: Vec<f32> = probs.into_data().iter::<f32>().collect();
    let classes = spec.num_classes;
    let mut confusion = vec![vec![0usize; classes]; classes];
    let mut int8_ok = 0usize;
    let mut float_ok = 0usize;
    for (n, window) in val.iter().enumerate() {
        let out = interp.run(&window.values).expect("interp runs");
        let pred = argmax(&out.probabilities);
        let float_pred = argmax(&float_probs[n * classes..(n + 1) * classes]);
        confusion[window.label][pred] += 1;
        int8_ok += (pred == window.label) as usize;
        float_ok += (float_pred == window.label) as usize;
    }
    let n = val.len().max(1) as f32;
    (float_ok as f32 / n, int8_ok as f32 / n, confusion)
}

fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(usize::MAX)
}

fn class_counts(spec: &TaskSpec, windows: &[crate::data::Window]) -> Vec<usize> {
    let mut counts = vec![0usize; spec.num_classes];
    for w in windows {
        counts[w.label] += 1;
    }
    counts
}

fn print_confusion(spec: &TaskSpec, confusion: &[Vec<usize>]) {
    let header: String = spec
        .class_names
        .iter()
        .map(|n| format!("{n:>10}"))
        .collect();
    println!("confusion matrix (rows=true, cols=pred):");
    println!("       {header}");
    for (label, row) in spec.class_names.iter().zip(confusion) {
        let cells: String = row.iter().map(|v| format!("{v:>10}")).collect();
        println!("{label:>6} {cells}");
    }
}

/// sha256 of the model bytes (the determinism gate artifact).
pub fn sha256_hex(bytes: &[u8]) -> String {
    // A tiny, dependency-free SHA-256 (the track's "zero new dependencies"
    // rule; FIPS 180-4, verified against known vectors by the test below).
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut message = bytes.to_vec();
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for block in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

#[allow(unused)]
fn unused_path_assert(p: &Path) {
    let _ = p;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let million_a = vec![b'a'; 1_000_000];
        assert_eq!(
            sha256_hex(&million_a),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
}
