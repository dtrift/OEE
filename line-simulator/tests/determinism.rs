//! Интеграционный тест симулятора: полный прогон сценария → CSV → повтор → diff.

use std::process::Command;

/// Два прогона с одним seed дают побитово одинаковый CSV (гейт недели 1).
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
    assert_eq!(
        csv_a, csv_b,
        "два прогона с seed=42 обязаны совпасть побитово"
    );

    // Смоук: заголовок и непустое тело.
    let text = String::from_utf8(csv_a).unwrap();
    assert!(text.starts_with("t_ms,current_a,state\n"));
    assert!(
        text.lines().count() > 90_000,
        "слишком мало строк: {}",
        text.lines().count()
    );
}
