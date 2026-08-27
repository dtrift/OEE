//! Simulator integration test: full scenario run -> CSV -> repeat -> diff.

use std::process::Command;

/// Two runs with the same seed produce a bit-identical CSV (week-1 gate).
#[test]
fn deterministic_csv() {
    let bin = env!("CARGO_BIN_EXE_line-simulator");
    let out = |path: &str| {
        let status = Command::new(bin)
            .args([
                "--scenario",
                concat!(env!("CARGO_MANIFEST_DIR"), "/../scenarios/base.toml"),
                "--seed",
                "42",
                "--out",
                path,
            ])
            .status()
            .expect("spawn simulator");
        assert!(status.success());
    };
    let dir = std::env::temp_dir();
    let a = dir.join("oee_sim_test_a.csv");
    let b = dir.join("oee_sim_test_b.csv");
    out(a.to_str().unwrap());
    out(b.to_str().unwrap());
    let csv_a = std::fs::read(&a).expect("read a");
    let csv_b = std::fs::read(&b).expect("read b");
    assert_eq!(csv_a, csv_b, "two runs with seed=42 must match bit-for-bit");

    // Smoke: header and non-empty body.
    let text = String::from_utf8(csv_a).unwrap();
    assert!(text.starts_with("t_ms,current_a,state\n"));
    assert!(
        text.lines().count() > 90_000,
        "too few lines: {}",
        text.lines().count()
    );
}
