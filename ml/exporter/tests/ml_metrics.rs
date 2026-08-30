//! Track D5.3: the int8 model metrics **through microflow** (`#[model]`) —
//! val accuracy + the confusion matrix in the py script's format, written to
//! `ml/models/model_a_metrics.txt`.
//!
//! `#[model]` bakes the file at COMPILE time: after regenerating the model,
//! re-run as
//!     touch ml/exporter/tests/ml_metrics.rs
//!     cargo test -p exporter --release --test ml_metrics -- --nocapture
//! (see ml/README.md).

use microflow::model;
use nalgebra::SMatrix;

#[model("ml/models/model_a.tflite")]
struct ModelA;

const CLASS_NAMES: [&str; 4] = ["idle", "run", "jam", "overload"];

#[test]
fn int8_val_metrics_through_microflow() {
    let text = std::fs::read_to_string("../../ml/models/model_a.val.csv")
        .expect("run the trainer pipeline first (ml/README.md)");
    let mut confusion = vec![vec![0usize; 4]; 4];
    let mut total = 0usize;
    let mut correct = 0usize;
    for line in text.lines().skip(1) {
        let mut fields = line.split(',');
        let label: usize = fields.next().unwrap().parse().unwrap();
        let values: Vec<f32> = fields.map(|v| v.parse().unwrap()).collect();
        assert_eq!(values.len(), 128, "the val window must hold 128 samples");
        let input: SMatrix<f32, 128, 1> = SMatrix::from_fn(|t, _| values[t]);
        let output = ModelA::predict(input);
        let pred = output
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        confusion[label][pred] += 1;
        correct += (pred == label) as usize;
        total += 1;
    }
    assert!(total > 0, "the val split is empty");
    let accuracy = correct as f32 / total as f32;
    println!("int8 val accuracy (microflow): {accuracy:.4}");
    let header: String = CLASS_NAMES.iter().map(|n| format!("{n:>10}")).collect();
    println!("confusion matrix (rows=true, cols=pred):");
    println!("       {header}");
    for (label, row) in CLASS_NAMES.iter().zip(&confusion) {
        let cells: String = row.iter().map(|v| format!("{v:>10}")).collect();
        println!("{label:>6} {cells}");
    }

    // The gate: the microflow kernel must agree with the interp reference's
    // 1.0000 within the track's 2% budget.
    assert!(
        accuracy >= 0.98,
        "int8 accuracy through microflow degraded: {accuracy}"
    );

    // Rewrite the metrics artifact in the py script's format (interp metrics
    // from the pipeline run stay in place; this appends the microflow check).
    let mut metrics = vec![
        "# microflow (#[model]) evaluation of model_a.tflite".to_string(),
        format!("int8 val accuracy: {accuracy:.4}"),
        format!("windows: val {total}"),
        "confusion matrix (rows=true, cols=pred):".to_string(),
        format!("       {header}"),
    ];
    for (label, row) in CLASS_NAMES.iter().zip(&confusion) {
        let cells: String = row.iter().map(|v| format!("{v:>10}")).collect();
        metrics.push(format!("{label:>6} {cells}"));
    }
    std::fs::write(
        "../../ml/models/model_a_metrics.txt",
        metrics.join("\n") + "\n",
    )
    .expect("write the microflow metrics");
    println!("metrics: ml/models/model_a_metrics.txt");
}
