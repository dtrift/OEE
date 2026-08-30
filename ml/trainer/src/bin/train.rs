//! Track D5: the one-command pipeline.
//!
//!     cargo run -p trainer --release --bin train -- \
//!         --datasets tmp/ds_base_1.csv tmp/ds_base_2.csv ... \
//!         --calib 256 --out ml/models/model_a.tflite
//!
//! Artifacts (next to the .tflite): `model_a.float` (float weights),
//! `model_a_val.csv` (the val split, the metrics/parity input), the ops dump.
//! After this, re-run the microflow-side checks (they bake the file at
//! compile time — see ml/README.md):
//!     touch ml/exporter/tests/model_a_parity.rs nodes/src/a.rs
//!     cargo test -p exporter --test model_a_parity --release
//!     cargo test -p exporter --test ml_metrics --release -- --nocapture

use std::path::PathBuf;

use clap::Parser;

/// Trains model A in burn and exports the int8 `.tflite` (Rust-only pipeline).
#[derive(Parser, Debug)]
#[command(about, long_about = None)]
struct Args {
    /// Dataset CSVs from `line-simulator --dataset` (repeat the flag or
    /// pass several values: --datasets a.csv b.csv).
    #[arg(long, required = true, num_args = 1..)]
    datasets: Vec<PathBuf>,
    /// Calibration windows for PTQ (drawn from the train split, seed 2026).
    #[arg(long, default_value_t = 256)]
    calib: usize,
    /// Training epochs.
    #[arg(long, default_value_t = 30)]
    epochs: usize,
    /// The int8 model path.
    #[arg(long, default_value = "ml/models/model_a.tflite")]
    out: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let float_out = args.out.with_extension("float");
    let val_csv = args.out.with_extension("val.csv");
    let report = trainer::pipeline::run::<burn::backend::Autodiff<burn::backend::ndarray::NdArray>>(
        trainer::pipeline::PipelineArgs {
            datasets: args.datasets,
            epochs: args.epochs,
            calib: args.calib,
            out: args.out.clone(),
            float_out,
            val_csv,
        },
    )?;

    // The metrics file in the py script's format (int8 through microflow is a
    // separate test target — ml_metrics — since #[model] is compile-time).
    let mut metrics = vec![
        format!("seed: {}", trainer::SEED),
        format!("float val accuracy: {:.4}", report.float_val_accuracy),
        format!(
            "int8 val accuracy (interp): {:.4}",
            report.int8_val_accuracy
        ),
        format!("windows: val {}", report.val_windows),
        format!("sha256: {}", report.sha256),
        "confusion matrix (rows=true, cols=pred):".to_string(),
    ];
    let header: String = trainer::CLASS_NAMES
        .iter()
        .map(|n| format!("{n:>10}"))
        .collect();
    metrics.push(format!("       {header}"));
    for (label, row) in trainer::CLASS_NAMES.iter().zip(&report.confusion) {
        let cells: String = row.iter().map(|v| format!("{v:>10}")).collect();
        metrics.push(format!("{label:>6} {cells}"));
    }
    let metrics_path = args.out.with_extension("metrics.txt");
    std::fs::write(&metrics_path, metrics.join("\n") + "\n")?;
    println!("metrics: {}", metrics_path.display());
    Ok(())
}
