"""Бонус Д3 (неделя 1): dense-модель для полного малого цикла Keras -> tflite -> Rust.

FC + Softmax на входе ранга 1 (без свёрток) — минимальная модель, которую должен
проглотить форк microflow как есть. Проверяет, что наш TF экспортирует парсящийся
форком .tflite, до всякого Conv1D.

Запуск (от корня репо):
    tmp/venv312/bin/python ml/scripts/build_dense_model.py

Артефакт:
    ml/models/dense.tflite
"""

import pathlib

import numpy as np
import tensorflow as tf

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
MODELS_DIR = REPO_ROOT / "ml" / "models"

INPUT_DIM = 8
NUM_CLASSES = 4


def representative_dataset():
    rng = np.random.default_rng(42)
    for _ in range(100):
        yield [rng.normal(0.0, 1.0, (1, INPUT_DIM)).astype(np.float32)]


def build_model() -> tf.keras.Model:
    # bias_initializer ненулевой: нулевой bias конвертер TF 2.21 выбрасывает
    # (FULLY_CONNECTED получает optional-вход -1), а парсер форка ожидает
    # bias третьим входом. Факт зафиксирован в spike/conv1d-serialization.md.
    model = tf.keras.Sequential(
        [
            tf.keras.layers.Input(shape=(INPUT_DIM,)),
            tf.keras.layers.Dense(16, activation="relu",
                                  bias_initializer="random_normal"),
            tf.keras.layers.Dense(NUM_CLASSES,
                                  bias_initializer="random_normal"),
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
    # Веса FC по умолчанию квантуются per-channel, а runtime форка поддерживает
    # только per-tensor (QUANTS=1). Для спайка отключаем per-channel.
    converter._experimental_disable_per_channel = True
    return converter.convert()


def main() -> None:
    MODELS_DIR.mkdir(parents=True, exist_ok=True)
    model = build_model()
    model.summary(print_fn=print)
    tflite_model = quantize(model)

    tflite_path = MODELS_DIR / "dense.tflite"
    tflite_path.write_bytes(tflite_model)
    print(f"\nSaved: {tflite_path} ({len(tflite_model)} bytes)")

    report = tf.lite.experimental.Analyzer.analyze(model_content=tflite_model)
    print(report)


if __name__ == "__main__":
    main()
