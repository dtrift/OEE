"""Week 3: parity fixtures for .tflite models against tf.lite.Interpreter
(spec §5.3).

Runs the interpreter on deterministic windows and writes a fixture file
consumed by the Rust parity tests in `fork/microflow/tests/` (±1 quantum
tolerance: the requant operation order may differ between implementations —
round_ties_even in the fork vs the reference kernel).

Two modes:
- default: synthetic current windows (the D3 conv1d.tflite case);
- `--windows-npz`: windows and labels from an .npz with `x` (float32,
  (N, 128, 1)) — e.g. `ml/models/model_a_val.npz` written by
  `train_model_a.py` (the D6 case); `--limit` caps the case count.

Run (from the repo root):
    tmp/venv312/bin/python ml/scripts/dump_parity_fixtures.py \
        --model ml/models/conv1d.tflite \
        --out fork/microflow/tests/golden/parity/conv1d.txt
"""

import argparse
import pathlib

import numpy as np
import tensorflow as tf

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent

TIMESTEPS = 128
CHANNELS = 1

# Mirrors the simulator's current signal (plan section 4): 50 Hz + harmonics.
AMPLITUDES = [0.05, 0.5, 1.0, 1.6]


def _synthetic_current(rng: np.random.Generator, amplitude: float, phase: float) -> np.ndarray:
    t = np.arange(TIMESTEPS, dtype=np.float32) / 1600.0
    signal = amplitude * np.sin(2 * np.pi * 50.0 * t + phase)
    signal += 0.15 * amplitude * np.sin(2 * np.pi * 150.0 * t + phase)
    signal += 0.07 * amplitude * np.sin(2 * np.pi * 250.0 * t + phase)
    signal += rng.normal(0.0, 0.05 * max(amplitude, 0.1), TIMESTEPS)
    return signal.astype(np.float32)


def synthetic_windows():
    """32 deterministic windows: 4 amplitudes x 2 phases x 4 noise draws."""
    rng = np.random.default_rng(42)
    for amplitude in AMPLITUDES:
        for phase in (0.0, np.pi / 3):
            for _ in range(4):
                yield _synthetic_current(rng, amplitude, phase)


def npz_windows(path: pathlib.Path, limit: int):
    data = np.load(path)
    x = data["x"].astype(np.float32)
    rng = np.random.default_rng(42)
    rng.shuffle(x)
    yield from x[:limit]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--model", type=pathlib.Path, default=REPO_ROOT / "ml" / "models" / "conv1d.tflite"
    )
    parser.add_argument(
        "--out",
        type=pathlib.Path,
        default=REPO_ROOT / "fork" / "microflow" / "tests" / "golden" / "parity" / "conv1d.txt",
    )
    parser.add_argument("--windows-npz", type=pathlib.Path, default=None)
    parser.add_argument("--limit", type=int, default=64)
    args = parser.parse_args()

    interpreter = tf.lite.Interpreter(model_path=str(args.model))
    interpreter.allocate_tensors()
    inp = interpreter.get_input_details()[0]
    out = interpreter.get_output_details()[0]
    input_scale, input_zero_point = inp["quantization"]
    output_scale, output_zero_point = out["quantization"]
    print(f"model: {args.model.name}")
    print(f"  input  scale={input_scale!r} zp={input_zero_point}")
    print(f"  output scale={output_scale!r} zp={output_zero_point}")

    lines = [
        f"# {args.model.name} parity fixtures (spec 5.3)",
        "# DO NOT EDIT: regenerate with",
        "#   tmp/venv312/bin/python ml/scripts/dump_parity_fixtures.py",
        "# header: input_scale input_zp output_scale output_zp",
        f"{input_scale!r} {input_zero_point} {output_scale!r} {output_zero_point}",
    ]

    windows = (
        npz_windows(args.windows_npz, args.limit)
        if args.windows_npz
        else synthetic_windows()
    )
    case = 0
    for window in windows:
        x_q = np.clip(
            np.round(window / input_scale) + input_zero_point, -128, 127
        ).astype(np.int8).reshape(1, TIMESTEPS, CHANNELS)
        interpreter.set_tensor(inp["index"], x_q)
        interpreter.invoke()
        y_q = interpreter.get_tensor(out["index"]).astype(np.int32).ravel()
        lines.append("input")
        lines.extend(str(int(v)) for v in x_q.ravel())
        lines.append("output")
        lines.extend(str(int(v)) for v in y_q)
        case += 1
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text("\n".join(lines) + "\n")
    print(f"written {case} cases to {args.out}")


if __name__ == "__main__":
    main()
