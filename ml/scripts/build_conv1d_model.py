"""Spike D3 (week 1): Keras Conv1D serialization to .tflite.

Builds the model from OEE plan section 6:
    Conv1D(8) -> AvgPool -> Conv1D(16) -> AvgPool -> FC -> Softmax, input (T, C).
Full-integer int8 quantization, operator dump via tf.lite.Interpreter.

Weights are untrained — the spike is about serialization, not accuracy
(training: weeks 3-4).

Run (from the repo root):
    tmp/venv312/bin/python ml/scripts/build_conv1d_model.py

Artifacts:
    ml/models/conv1d.tflite     — quantized model
    ml/models/conv1d_ops.txt    — operator and tensor dump (a fact for the spec)
"""

import pathlib

import numpy as np
import tensorflow as tf

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
MODELS_DIR = REPO_ROOT / "ml" / "models"

# Current-signal window parameters: 128 samples, 1 channel (mono current).
# Spike choice: ~80 ms at 1.6 kHz = 4 periods of 50 Hz. The real choice is weeks 3-4.
TIMESTEPS = 128
CHANNELS = 1
NUM_CLASSES = 4  # idle / run / jam / overload


# Classes differ in the amplitude of the 50 Hz sine envelope — a rough
# approximation of the future signal from plan section 4; good enough for a
# representative dataset.
def _synthetic_current(rng: np.random.Generator, amplitude: float) -> np.ndarray:
    t = np.arange(TIMESTEPS, dtype=np.float32) / 1600.0
    signal = amplitude * np.sin(2 * np.pi * 50.0 * t)
    signal += 0.2 * amplitude * np.sin(2 * np.pi * 150.0 * t)
    signal += rng.normal(0.0, 0.05 * max(amplitude, 0.1), TIMESTEPS)
    return signal.astype(np.float32)


def representative_dataset():
    rng = np.random.default_rng(42)
    amplitudes = [0.05, 0.5, 1.0, 1.6]  # idle / run / jam / overload
    for _ in range(200):
        amplitude = amplitudes[rng.integers(0, len(amplitudes))]
        yield [_synthetic_current(rng, amplitude).reshape(1, TIMESTEPS, CHANNELS)]


def build_model() -> tf.keras.Model:
    model = tf.keras.Sequential(
        [
            tf.keras.layers.Input(shape=(TIMESTEPS, CHANNELS)),
            tf.keras.layers.Conv1D(8, kernel_size=3, padding="valid"),
            tf.keras.layers.ReLU(),
            tf.keras.layers.AveragePooling1D(pool_size=2),
            tf.keras.layers.Conv1D(16, kernel_size=3, padding="valid"),
            tf.keras.layers.ReLU(),
            tf.keras.layers.AveragePooling1D(pool_size=2),
            tf.keras.layers.Flatten(),
            tf.keras.layers.Dense(NUM_CLASSES),
            tf.keras.layers.Softmax(),
        ]
    )
    model.build()
    return model


def quantize(model: tf.keras.Model) -> bytes:
    converter = tf.lite.TFLiteConverter.from_keras_model(model)
    converter.optimizations = [tf.lite.Optimize.DEFAULT]
    converter.representative_dataset = representative_dataset
    converter.target_spec.supported_ops = [tf.lite.OpsSet.TFLITE_BUILTINS_INT8]
    converter.inference_input_type = tf.int8
    converter.inference_output_type = tf.int8
    return converter.convert()


def dump_operators(tflite_model: bytes) -> str:
    interpreter = tf.lite.Interpreter(model_content=tflite_model)
    interpreter.allocate_tensors()
    tensors = {t["index"]: t for t in interpreter.get_tensor_details()}

    def fmt_tensor(idx: int) -> str:
        if idx < 0:
            return f"#{idx} (optional/none)"
        t = tensors.get(idx)
        if t is None:
            return f"#{idx} (not found in tensor details)"
        quant = t["quantization"]
        if quant is not None and (np.ndim(quant[0]) > 0 or quant[0]):
            q = f", scale={quant[0]}, zp={quant[1]}"
        else:
            q = ""
        return f"#{idx} {t['name']} shape={list(t['shape'])} dtype={t['dtype']}{q}"

    lines = ["# conv1d.tflite operator dump (serialization fact, week-1 item D3)", ""]
    for op in interpreter._get_ops_details():
        lines.append(f"op[{op['index']}] {op['op_name']}")
        for i in op["inputs"]:
            lines.append(f"  in  {fmt_tensor(i)}")
        for o in op["outputs"]:
            lines.append(f"  out {fmt_tensor(o)}")
    lines.append("")
    lines.append("# Global subgraph input/output")
    for i in interpreter.get_input_details():
        lines.append(f"  input  {fmt_tensor(i['index'])}")
    for o in interpreter.get_output_details():
        lines.append(f"  output {fmt_tensor(o['index'])}")
    return "\n".join(lines) + "\n"


def sanity_check_inference(model: tf.keras.Model, tflite_model: bytes) -> None:
    """Cross-check .tflite against Keras: if bias/layout got lost in conversion, we will see it."""
    interpreter = tf.lite.Interpreter(model_content=tflite_model)
    interpreter.allocate_tensors()
    inp = interpreter.get_input_details()[0]
    out = interpreter.get_output_details()[0]
    rng = np.random.default_rng(7)
    max_diff = 0.0
    for _ in range(5):
        amplitude = [0.05, 0.5, 1.0, 1.6][rng.integers(0, 4)]
        x = _synthetic_current(rng, amplitude).reshape(1, TIMESTEPS, CHANNELS)
        x_q = np.clip(np.round(x[0] / inp["quantization"][0]) + inp["quantization"][1],
                      -128, 127).astype(np.int8).reshape(1, TIMESTEPS, CHANNELS)
        ref = model(x, training=False).numpy()[0]
        interpreter.set_tensor(inp["index"], x_q)
        interpreter.invoke()
        got = interpreter.get_tensor(out["index"])[0].astype(np.float32)
        got = (got.astype(np.int32) - out["quantization"][1]) * out["quantization"][0]
        max_diff = max(max_diff, float(np.abs(ref - got).max()))
        print(f"  keras argmax={ref.argmax()}, tflite argmax={got.argmax()}, "
              f"max|diff|={np.abs(ref - got).max():.4f}")
    print(f"sanity: max|diff|={max_diff:.4f} "
          f"(threshold 0.05 — int8 quantization without QAT gives a noticeable error)")


def main() -> None:
    MODELS_DIR.mkdir(parents=True, exist_ok=True)
    model = build_model()
    model.summary(print_fn=print)
    tflite_model = quantize(model)

    tflite_path = MODELS_DIR / "conv1d.tflite"
    tflite_path.write_bytes(tflite_model)
    print(f"\nSaved: {tflite_path} ({len(tflite_model)} bytes)")

    dump = dump_operators(tflite_model)
    dump_path = MODELS_DIR / "conv1d_ops.txt"
    dump_path.write_text(dump)
    print(dump)
    print("# Sanity: tflite vs keras")
    sanity_check_inference(model, tflite_model)


if __name__ == "__main__":
    main()
