"""Week 3 (D5): train the node A model on synthetic windows, export int8.

Model A (plan section 6): the same architecture as the spike model —
Conv1D(8) -> AvgPool -> Conv1D(16) -> AvgPool -> Flatten -> Dense -> Softmax,
input (128, 1) raw current windows (WindowSpec(A) = 128 @ 1.6 kHz). The
operator structure therefore matches `ml/models/conv1d.tflite` exactly (the
same converter), which is what the week-3 parser expects.

Determinism: seeds are fixed; a re-run yields the same model.

Dataset: CSVs produced by
    cargo run -p line-simulator -- --scenario scenarios/<s>.toml \
        --seed <n> --dataset tmp/ds_<s>_<n>.csv
with header `label,state,x000..x127` (labels: 0=idle, 1=run, 2=jam,
3=overload — pinned by `MachineState::class_index`).

The classes are imbalanced by construction (run dominates; the scenarios are
physical); training uses class weights, and the val confusion matrix is
printed — the honest measure.

Run (from the repo root):
    tmp/venv312/bin/python ml/scripts/train_model_a.py \
        --datasets tmp/ds_base_1.csv tmp/ds_downtime_1.csv ...

Artifacts:
    ml/models/model_a.tflite   — full-integer int8 model
    ml/models/model_a_ops.txt  — operator dump (structure check)
    ml/models/model_a_val.npz  — the val split (for the parity test, D6)
    ml/models/model_a_metrics.txt — accuracy, confusion matrix, seeds
"""

import argparse
import pathlib

import numpy as np
import tensorflow as tf

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
MODELS_DIR = REPO_ROOT / "ml" / "models"

TIMESTEPS = 128
CHANNELS = 1
NUM_CLASSES = 4  # 0=idle, 1=run, 2=jam, 3=overload (MachineState::class_index)
CLASS_NAMES = ["idle", "run", "jam", "overload"]

SEED = 2026
EPOCHS = 30
BATCH_SIZE = 64
VAL_FRACTION = 0.15


def load_datasets(paths: list[pathlib.Path]) -> tuple[np.ndarray, np.ndarray]:
    xs, ys = [], []
    for path in paths:
        raw = np.loadtxt(path, delimiter=",", skiprows=1)
        if raw.ndim == 1:
            raw = raw[None, :]
        labels = raw[:, 0].astype(np.int64)
        windows = raw[:, 2:].astype(np.float32).reshape(-1, TIMESTEPS, CHANNELS)
        xs.append(windows)
        ys.append(labels)
    return np.concatenate(xs), np.concatenate(ys)


def split(x: np.ndarray, y: np.ndarray) -> tuple[np.ndarray, ...]:
    """Deterministic split: per-class shuffling with the fixed seed, so both
    halves keep the class profile."""
    rng = np.random.default_rng(SEED)
    train_idx, val_idx = [], []
    for label in range(NUM_CLASSES):
        idx = np.flatnonzero(y == label)
        rng.shuffle(idx)
        cut = max(1, int(len(idx) * (1.0 - VAL_FRACTION))) if len(idx) > 1 else len(idx)
        train_idx.append(idx[:cut])
        val_idx.append(idx[cut:])
    train_idx = np.concatenate(train_idx)
    val_idx = np.concatenate(val_idx)
    return x[train_idx], y[train_idx], x[val_idx], y[val_idx]


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


def class_weights(y: np.ndarray) -> dict[int, float]:
    counts = np.bincount(y, minlength=NUM_CLASSES).astype(np.float64)
    total = counts.sum()
    # Inverse frequency, normalized so the weights average to 1.
    weights = total / (NUM_CLASSES * np.maximum(counts, 1.0))
    return {label: float(weight) for label in range(NUM_CLASSES)}


def representative_dataset(x_train: np.ndarray):
    rng = np.random.default_rng(SEED)
    for _ in range(256):
        yield [x_train[rng.integers(0, len(x_train))][None, ...]]


def quantize(model: tf.keras.Model, x_train: np.ndarray, fc_per_tensor: bool) -> bytes:
    converter = tf.lite.TFLiteConverter.from_keras_model(model)
    converter.optimizations = [tf.lite.Optimize.DEFAULT]
    converter.representative_dataset = lambda: representative_dataset(x_train)
    converter.target_spec.supported_ops = [tf.lite.OpsSet.TFLITE_BUILTINS_INT8]
    converter.inference_input_type = tf.int8
    converter.inference_output_type = tf.int8
    if fc_per_tensor:
        # Workaround for runtimes without per-channel FC (spec §2.4): not
        # needed for the microflow fork (per-channel since week 3, §3.3).
        converter._experimental_disable_per_channel = True
    return converter.convert()


def dump_operators(tflite_model: bytes) -> str:
    interpreter = tf.lite.Interpreter(model_content=tflite_model)
    interpreter.allocate_tensors()
    lines = [f"# model_a.tflite operator dump (week 3, D5)"]
    for op in interpreter._get_ops_details():
        lines.append(f"op[{op['index']}] {op['op_name']}")
    lines.append("")
    for detail in interpreter.get_input_details() + interpreter.get_output_details():
        lines.append(f"# {detail['name']} shape={list(detail['shape'])} dtype={detail['dtype']}")
    return "\n".join(lines) + "\n"


def confusion(y_true: np.ndarray, y_pred: np.ndarray) -> np.ndarray:
    matrix = np.zeros((NUM_CLASSES, NUM_CLASSES), dtype=np.int64)
    for true, pred in zip(y_true, y_pred):
        matrix[true, pred] += 1
    return matrix


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--datasets",
        nargs="+",
        type=pathlib.Path,
        required=True,
        help="dataset CSVs from line-simulator --dataset",
    )
    parser.add_argument("--epochs", type=int, default=EPOCHS)
    parser.add_argument(
        "--fc-per-tensor",
        action="store_true",
        help="disable per-channel FC quantization (spec 2.4 workaround)",
    )
    parser.add_argument("--output-stem", default="model_a")
    args = parser.parse_args()

    tf.keras.utils.set_random_seed(SEED)

    x, y = load_datasets(args.datasets)
    print(f"dataset: {len(x)} windows, classes {np.bincount(y, minlength=NUM_CLASSES)}")
    x_train, y_train, x_val, y_val = split(x, y)
    print(
        f"split: train {len(x_train)} ({np.bincount(y_train, minlength=NUM_CLASSES)}), "
        f"val {len(x_val)} ({np.bincount(y_val, minlength=NUM_CLASSES)})"
    )

    model = build_model()
    model.compile(
        optimizer=tf.keras.optimizers.Adam(learning_rate=1e-3),
        loss="sparse_categorical_crossentropy",
        metrics=["accuracy"],
    )
    model.fit(
        x_train,
        y_train,
        validation_data=(x_val, y_val),
        epochs=args.epochs,
        batch_size=BATCH_SIZE,
        class_weight=class_weights(y_train),
        verbose=2,
    )

    tflite_model = quantize(model, x_train, args.fc_per_tensor)
    stem = args.output_stem
    tflite_path = MODELS_DIR / f"{stem}.tflite"
    tflite_path.write_bytes(tflite_model)
    print(f"\nSaved: {tflite_path} ({len(tflite_model)} bytes)")
    (MODELS_DIR / f"{stem}_ops.txt").write_text(dump_operators(tflite_model))

    # int8 evaluation on the val split (the parity input for D6).
    interpreter = tf.lite.Interpreter(model_content=tflite_model)
    interpreter.allocate_tensors()
    inp = interpreter.get_input_details()[0]
    out = interpreter.get_output_details()[0]
    scale, zero_point = inp["quantization"]
    y_pred = []
    for window in x_val:
        q = np.clip(np.round(window / scale) + zero_point, -128, 127).astype(np.int8)
        interpreter.set_tensor(inp["index"], q[None, ...])
        interpreter.invoke()
        y_pred.append(np.argmax(interpreter.get_tensor(out["index"])[0]))
    y_pred = np.array(y_pred)
    accuracy = float((y_pred == y_val).mean())
    matrix = confusion(y_val, y_pred)
    print(f"\nint8 val accuracy: {accuracy:.4f}")
    print("confusion matrix (rows=true, cols=pred):")
    header = "       " + "".join(f"{name:>10}" for name in CLASS_NAMES)
    print(header)
    for label, row in zip(CLASS_NAMES, matrix):
        print(f"{label:>6} " + "".join(f"{v:>10}" for v in row))

    if accuracy > 0.995:
        print(
            "\nWARNING: ~100% accuracy — synthetic data is too clean; raise the "
            "simulator noise/drift (plan section 11) and rebuild the dataset."
        )

    np.savez(
        MODELS_DIR / f"{stem}_val.npz",
        x=x_val.astype(np.float32),
        y=y_val.astype(np.int64),
    )
    metrics = [
        f"seed: {SEED}",
        f"datasets: {[str(p) for p in args.datasets]}",
        f"windows: train {len(x_train)}, val {len(x_val)}",
        f"int8 val accuracy: {accuracy:.4f}",
        "confusion matrix (rows=true, cols=pred):",
        header,
    ]
    metrics.extend(f"{label:>6} " + "".join(f"{v:>10}" for v in row) for label, row in zip(CLASS_NAMES, matrix))
    (MODELS_DIR / f"{stem}_metrics.txt").write_text("\n".join(metrics) + "\n")
    print(f"metrics: {MODELS_DIR / f'{stem}_metrics.txt'}")


if __name__ == "__main__":
    main()
