//! The float-weights mini-format `model_a.float` (track D0): a JSON header
//! line with the architecture + a f32 LE blob, no serde — zero new
//! dependencies. The trainer writes it after burn training; the exporter's
//! PTQ reads it back. Deterministic byte-for-byte on the same weights.
//!
//! ```text
//! {"format":"oee-float-weights","version":1,"timesteps":128,"channels":1,
//!  "conv1":{"filters":8,"kernel":3},"pool1":2,
//!  "conv2":{"filters":16,"kernel":3},"pool2":2,"fc":{"out":4}}
//! <conv1.w [F,C,k]><conv1.b [F]><conv2.w><conv2.b><fc.w [Out,In]><fc.b>
//! ```

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;

use crate::quant::{FloatConv, FloatFc, FloatModel};

/// Reads the float model from disk.
pub fn read_float_model(path: &Path) -> Result<FloatModel, String> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut header = String::new();
    reader
        .read_line(&mut header)
        .map_err(|e| format!("cannot read the header of {}: {e}", path.display()))?;
    let dims = parse_header(&header)?;
    let (f1, k1) = dims.conv1;
    let (f2, k2) = dims.conv2;

    let mut blob = Vec::new();
    reader
        .read_to_end(&mut blob)
        .map_err(|e| format!("cannot read the weights of {}: {e}", path.display()))?;
    let expected =
        (f1 * dims.channels * k1 + f1 + f2 * f1 * k2 + f2 + dims.fc_out * dims.fc_in + dims.fc_out)
            * 4;
    if blob.len() != expected {
        return Err(format!(
            "{} holds {} bytes of weights, the header implies {expected}",
            path.display(),
            blob.len()
        ));
    }
    let mut values = Vec::with_capacity(blob.len() / 4);
    for chunk in blob.chunks_exact(4) {
        values.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    let mut at = 0usize;
    let mut take = |n: usize| {
        let out = values[at..at + n].to_vec();
        at += n;
        out
    };

    let c1 = dims.channels;

    Ok(FloatModel {
        timesteps: dims.timesteps,
        channels: c1,
        conv1: FloatConv {
            filters: f1,
            kernel: k1,
            weights: take(f1 * c1 * k1),
            bias: take(f1),
        },
        pool1: dims.pool1,
        conv2: FloatConv {
            filters: f2,
            kernel: k2,
            weights: take(f2 * f1 * k2),
            bias: take(f2),
        },
        pool2: dims.pool2,
        fc: FloatFc {
            out_units: dims.fc_out,
            in_units: dims.fc_in,
            weights: take(dims.fc_out * dims.fc_in),
            bias: take(dims.fc_out),
        },
    })
}

/// Writes the float model to disk.
pub fn write_float_model(model: &FloatModel, path: &Path) -> Result<(), String> {
    let header = format!(
        "{{\"format\":\"oee-float-weights\",\"version\":1,\"timesteps\":{},\"channels\":{},\"conv1\":{{\"filters\":{},\"kernel\":{}}},\"pool1\":{},\"conv2\":{{\"filters\":{},\"kernel\":{}}},\"pool2\":{},\"fc\":{{\"out\":{},\"in\":{}}}}}\n",
        model.timesteps,
        model.channels,
        model.conv1.filters,
        model.conv1.kernel,
        model.pool1,
        model.conv2.filters,
        model.conv2.kernel,
        model.pool2,
        model.fc.out_units,
        model.fc.in_units
    );
    let mut blob = Vec::new();
    for v in model
        .conv1
        .weights
        .iter()
        .chain(&model.conv1.bias)
        .chain(&model.conv2.weights)
        .chain(&model.conv2.bias)
        .chain(&model.fc.weights)
        .chain(&model.fc.bias)
    {
        blob.extend_from_slice(&v.to_le_bytes());
    }
    let mut out = std::fs::File::create(path)
        .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    out.write_all(header.as_bytes())
        .and_then(|_| out.write_all(&blob))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(())
}

struct Header {
    timesteps: usize,
    channels: usize,
    conv1: (usize, usize),
    pool1: usize,
    conv2: (usize, usize),
    pool2: usize,
    fc_out: usize,
    fc_in: usize,
}

/// A minimal JSON number/key scanner for the fixed header shape (no serde,
/// per the track's anti-scope). Repeated keys ("filters", "kernel") are
/// resolved by occurrence: conv1 first, conv2 second.
fn parse_header(header: &str) -> Result<Header, String> {
    let num = |key: &str, occurrence: usize| -> Result<usize, String> {
        let marker = format!("\"{key}\":");
        let mut search_from = 0usize;
        let mut count = 0usize;
        while let Some(rel) = header[search_from..].find(&marker) {
            let at = search_from + rel;
            if count == occurrence {
                let rest = &header[at + marker.len()..];
                let end = rest
                    .find(|c: char| !c.is_ascii_digit())
                    .ok_or_else(|| format!("the '{key}' field has no value"))?;
                return rest[..end]
                    .parse()
                    .map_err(|e| format!("bad '{key}' value: {e}"));
            }
            count += 1;
            search_from = at + marker.len();
        }
        Err(format!(
            "the header has no occurrence {occurrence} of '{key}'"
        ))
    };
    Ok(Header {
        timesteps: num("timesteps", 0)?,
        channels: num("channels", 0)?,
        conv1: (num("filters", 0)?, num("kernel", 0)?),
        pool1: num("pool1", 0)?,
        conv2: (num("filters", 1)?, num("kernel", 1)?),
        pool2: num("pool2", 0)?,
        fc_out: num("out", 0)?,
        fc_in: num("in", 0)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toy_model() -> FloatModel {
        FloatModel {
            timesteps: 8,
            channels: 1,
            conv1: FloatConv {
                filters: 2,
                kernel: 3,
                weights: vec![0.1; 6],
                bias: vec![0.01, -0.02],
            },
            pool1: 2,
            conv2: FloatConv {
                filters: 3,
                kernel: 3,
                weights: vec![-0.2; 18],
                bias: vec![0.0; 3],
            },
            pool2: 2,
            fc: FloatFc {
                out_units: 4,
                in_units: 1,
                weights: vec![0.5; 4],
                bias: vec![1.0, -1.0, 0.25, -0.25],
            },
        }
    }

    #[test]
    fn float_format_roundtrip() {
        let dir = std::env::temp_dir().join("exporter_float_format");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("toy.float");
        let model = toy_model();
        write_float_model(&model, &path).unwrap();
        let read = read_float_model(&path).unwrap();
        assert_eq!(read.timesteps, model.timesteps);
        assert_eq!(read.channels, model.channels);
        assert_eq!(read.conv1.weights, model.conv1.weights);
        assert_eq!(read.conv1.bias, model.conv1.bias);
        assert_eq!(read.conv2.weights, model.conv2.weights);
        assert_eq!(read.fc.out_units, model.fc.out_units);
        assert_eq!(read.fc.in_units, model.fc.in_units);
        assert_eq!(read.fc.weights, model.fc.weights);
        assert_eq!(read.fc.bias, model.fc.bias);
        // Determinism: a rewrite yields identical bytes.
        let first = std::fs::read(&path).unwrap();
        write_float_model(&model, &path).unwrap();
        assert_eq!(first, std::fs::read(&path).unwrap());
    }

    #[test]
    fn header_missing_field_is_reported() {
        let err = match parse_header("{\"timesteps\":8}") {
            Err(e) => e,
            Ok(_) => panic!("the truncated header must fail"),
        };
        assert!(err.contains("channels"), "{err}");
    }
}
