//! Track D6.1: generates the parity fixtures for the rust-born model — the
//! same format `fork/microflow/tests/conv1d_parity.rs` reads, but the source
//! of expectations is the interp reference, not the TF interpreter:
//!
//! ```text
//! <input_scale> <input_zp> <output_scale> <output_zp>
//! input
//! <128 int8 values>
//! output
//! <4 int8 values>
//! ```
//!
//! Run from the repo root after the trainer pipeline:
//!     cargo run -p exporter --release --bin parity_gen

use exporter::interp::InterpModel;

fn main() {
    let model_path = "ml/models/model_a.tflite";
    let val_path = "ml/models/model_a.val.csv";
    let out_path = "ml/models/model_a_parity.txt";
    let cases = std::env::args()
        .nth(1)
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(64);

    let interp = InterpModel::from_file(std::path::Path::new(model_path)).unwrap();
    let (in_scale, in_zp, out_scale, out_zp) = io_quants(&interp);

    let text = std::fs::read_to_string(val_path).unwrap();
    let mut fixture = format!("{in_scale} {in_zp} {out_scale} {out_zp}\n");
    let mut written = 0usize;
    // One case per class first, then every Nth window — a compact, diverse set.
    let rows: Vec<&str> = text.lines().skip(1).collect();
    let stride = (rows.len() / cases.max(1)).max(1);
    for (n, line) in rows.iter().enumerate() {
        if n % stride != 0 && n >= 4 {
            continue;
        }
        let mut fields = line.split(',');
        let _label: usize = fields.next().unwrap().parse().unwrap();
        let values: Vec<f32> = fields.map(|v| v.parse().unwrap()).collect();
        let out = interp.run(&values).unwrap();
        fixture.push_str("input\n");
        let input_q: Vec<i8> = values
            .iter()
            .map(|&v| ((v / in_scale).round_ties_even() + in_zp as f32).clamp(-128.0, 127.0) as i8)
            .collect();
        for q in &input_q {
            fixture.push_str(&format!("{q}\n"));
        }
        fixture.push_str("output\n");
        for q in &out.quantized_output {
            fixture.push_str(&format!("{q}\n"));
        }
        written += 1;
        if written >= cases {
            break;
        }
    }
    std::fs::write(out_path, fixture).unwrap();
    println!("wrote {out_path}: {written} cases from {val_path}");
}

/// The input/output quantization of the model (the fixture header).
fn io_quants(interp: &InterpModel) -> (f32, i32, f32, i32) {
    let _ = interp;
    let bytes = std::fs::read("ml/models/model_a.tflite").unwrap();
    let model = exporter::tflite::root_as_model(&bytes).unwrap();
    let subgraph = model.subgraphs().unwrap().get(0);
    let tensors = subgraph.tensors().unwrap();
    let input = tensors.get(subgraph.inputs().unwrap().get(0) as usize);
    let output = tensors.get(subgraph.outputs().unwrap().get(0) as usize);
    let iq = input.quantization().unwrap();
    let oq = output.quantization().unwrap();
    (
        iq.scale().unwrap().get(0),
        iq.zero_point().unwrap().get(0) as i32,
        oq.scale().unwrap().get(0),
        oq.zero_point().unwrap().get(0) as i32,
    )
}
