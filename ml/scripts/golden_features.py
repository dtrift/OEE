"""Week 3 (D4): golden features — numpy side of the parity safety net.

Generates one fixed window (deterministic, seeded) and writes it together
with the numpy-computed features to a fixture consumed by the Rust test
`features-cli/tests/golden_features.rs`:
- integer features (zero-crossings): bit-for-bit;
- float features (RMS, peak, spectrum): ±1e-6.

The feature definitions mirror `features-cli/src/features.rs` exactly:
- rms  = sqrt(mean(x^2)), sequential float32 summation;
- peak = max(|x|);
- zero_crossings = count of x[i-1] * x[i] < 0 (strict);
- spectrum = Goertzel magnitudes at 50/150/250 Hz, float32 recurrence,
  magnitude = sqrt(max(power, 0)) / N.

Run (system python3 with numpy is enough):
    python3 ml/scripts/golden_features.py
"""

import pathlib

import numpy as np

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
OUT_PATH = REPO_ROOT / "features-cli" / "tests" / "golden" / "features.txt"

SAMPLE_RATE_HZ = 1600.0
TIMESTEPS = 128
MAINS_HZ = np.float32(50.0)
HARMONICS = (1, 3, 5)


def fixed_window() -> np.ndarray:
    """The fixed window: 50 Hz + harmonics + seeded noise, amplitude 1.2."""
    rng = np.random.default_rng(2026)
    t = np.arange(TIMESTEPS, dtype=np.float32) / np.float32(SAMPLE_RATE_HZ)
    signal = 1.2 * np.sin(2 * np.pi * MAINS_HZ * t)
    signal += 0.15 * 1.2 * np.sin(2 * np.pi * np.float32(3.0) * MAINS_HZ * t)
    signal += 0.07 * 1.2 * np.sin(2 * np.pi * np.float32(5.0) * MAINS_HZ * t)
    # numpy sin on float32 arrays computes in float32 in-place; the noise is
    # drawn in float64 and cast (the same cast happens in the simulator path).
    signal += rng.normal(0.0, 0.05, TIMESTEPS).astype(np.float32)
    return signal.astype(np.float32)


def features(window: np.ndarray) -> dict:
    x = window.astype(np.float32)
    sum_squares = np.float32(0.0)
    peak = np.float32(0.0)
    crossings = 0
    for i in range(len(x)):
        sum_squares += np.float32(x[i] * x[i])
        peak = max(peak, np.abs(x[i]))
        if i > 0 and np.float32(x[i - 1] * x[i]) < 0:
            crossings += 1
    rms = np.sqrt(sum_squares / np.float32(len(x)))

    spectrum = []
    n = np.float32(len(x))
    for harmonic in HARMONICS:
        frequency = np.float32(MAINS_HZ * np.float32(harmonic))
        coefficient = np.float32(
            2.0
            * np.cos(np.float32(2.0) * np.float32(np.pi) * frequency / np.float32(SAMPLE_RATE_HZ))
        )
        s1 = np.float32(0.0)
        s2 = np.float32(0.0)
        for sample in x:
            s0 = np.float32(sample + np.float32(s1 * coefficient) - s2)
            s2 = s1
            s1 = s0
        power = np.float32(s1 * s1 + s2 * s2 - np.float32(s1 * s2) * coefficient)
        spectrum.append(np.sqrt(np.maximum(power, np.float32(0.0))) / n)

    return {
        "rms": float(rms),
        "peak": float(peak),
        "zero_crossings": crossings,
        "spectrum": [float(m) for m in spectrum],
    }


def main() -> None:
    window = fixed_window()
    feats = features(window)
    lines = [
        "# golden features fixture (week 3, D4; seed 2026)",
        "# DO NOT EDIT: regenerate with python3 ml/scripts/golden_features.py",
        "# window: 128 float32 samples (9 significant digits round-trip)",
    ]
    lines.append("window")
    lines.extend(f"{v:.9e}" for v in window)
    lines.append("features")
    lines.append(f"rms {feats['rms']:.9e}")
    lines.append(f"peak {feats['peak']:.9e}")
    lines.append(f"zero_crossings {feats['zero_crossings']}")
    lines.extend(f"spectrum[{i}] {m:.9e}" for i, m in enumerate(feats["spectrum"]))
    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUT_PATH.write_text("\n".join(lines) + "\n")
    print(f"window: rms={feats['rms']:.6f} peak={feats['peak']:.6f} "
          f"zc={feats['zero_crossings']} spectrum={ [round(m, 6) for m in feats['spectrum']] }")
    print(f"written {OUT_PATH}")


if __name__ == "__main__":
    main()
