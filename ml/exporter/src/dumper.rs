//! A human-readable structure dump of a `.tflite` (track D1): our analog of
//! the week-1 `conv1d_ops.txt`, for eyeballing and diffing the rust-born file
//! against the TF-converted one. Parses through the same vendored reader the
//! `#[model]` macro uses, so whatever dumps cleanly is exactly what the
//! compiler sees.

use crate::vendor::tflite::{root_as_model, BuiltinOperator, OperatorCode, Tensor};
use flatbuffers::{ForwardsUOffset, Vector};

/// Kind resolution mirroring the macro's `builtin_kind` (codes < 128 also live
/// in the deprecated byte field).
fn builtin_kind(codes: Vector<ForwardsUOffset<OperatorCode>>, index: usize) -> BuiltinOperator {
    let code = codes.get(index);
    let builtin = code.builtin_code();
    if builtin == BuiltinOperator::ADD && code.deprecated_builtin_code() != 0 {
        BuiltinOperator(code.deprecated_builtin_code() as i32)
    } else {
        builtin
    }
}

fn describe_tensor(tensor: &Tensor) -> String {
    let shape: Vec<i32> = tensor.shape().unwrap_or_default().iter().collect();
    let quant = tensor.quantization();
    let (scale, zp) = match quant {
        Some(q) => {
            let scale: Vec<f32> = q.scale().unwrap_or_default().iter().collect();
            let zp: Vec<i64> = q.zero_point().unwrap_or_default().iter().collect();
            let scales = if scale.len() == 1 {
                format!("{}", scale[0])
            } else {
                format!(
                    "[{} values, {}..{}]",
                    scale.len(),
                    scale[0],
                    scale[scale.len() - 1]
                )
            };
            let zps = if zp.len() == 1 {
                format!("{}", zp[0])
            } else {
                format!("[{} values]", zp.len())
            };
            (scales, zps)
        }
        None => ("-".to_string(), "-".to_string()),
    };
    format!(
        "shape={:?} dtype={:?}, scale={}, zp={}",
        shape,
        tensor.type_(),
        scale,
        zp
    )
}

/// Dumps the parsed structure of a `.tflite` byte stream.
pub fn dump_bytes(bytes: &[u8]) -> Result<String, String> {
    let model = root_as_model(bytes).map_err(|e| format!("invalid flatbuffers model: {e:?}"))?;
    let subgraph = model
        .subgraphs()
        .and_then(|s| if !s.is_empty() { Some(s.get(0)) } else { None })
        .ok_or("the model has no subgraphs")?;
    let tensors = subgraph.tensors().ok_or("the subgraph has no tensors")?;
    let operators = subgraph.operators().unwrap_or_default();
    let codes = model
        .operator_codes()
        .ok_or("the model has no operator codes")?;
    let buffers = model.buffers().unwrap_or_default();

    let mut lines = Vec::new();
    lines.push(format!(
        "# dump: {} operators, {} tensors, {} buffers, version {}",
        operators.len(),
        tensors.len(),
        buffers.len(),
        model.version()
    ));

    for (index, operator) in operators.iter().enumerate() {
        let kind = builtin_kind(codes, operator.opcode_index() as usize);
        lines.push(format!("op[{index}] {kind:?}"));
        for i in operator.inputs().unwrap_or_default().iter() {
            if i < 0 {
                lines.push(format!("  in  #{i} (optional/none)"));
                continue;
            }
            let tensor = tensors.get(i as usize);
            lines.push(format!(
                "  in  #{i} {} {}",
                tensor.name().unwrap_or("<unnamed>"),
                describe_tensor(&tensor)
            ));
        }
        for i in operator.outputs().unwrap_or_default().iter() {
            let tensor = tensors.get(i as usize);
            lines.push(format!(
                "  out #{i} {} {}",
                tensor.name().unwrap_or("<unnamed>"),
                describe_tensor(&tensor)
            ));
        }
    }

    lines.push(String::new());
    lines.push("# Global subgraph input/output".to_string());
    for i in subgraph.inputs().unwrap_or_default().iter() {
        let tensor = tensors.get(i as usize);
        lines.push(format!(
            "  input  #{i} {} {}",
            tensor.name().unwrap_or("<unnamed>"),
            describe_tensor(&tensor)
        ));
    }
    for i in subgraph.outputs().unwrap_or_default().iter() {
        let tensor = tensors.get(i as usize);
        lines.push(format!(
            "  output #{i} {} {}",
            tensor.name().unwrap_or("<unnamed>"),
            describe_tensor(&tensor)
        ));
    }
    Ok(lines.join("\n") + "\n")
}

/// Dumps a `.tflite` file from disk.
pub fn dump_file(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    dump_bytes(&bytes)
}
