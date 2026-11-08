//! Runs every `testbench/scenarios/*.tbx` as part of `cargo test`.

use std::path::PathBuf;

use tapboxd::bench::{Bench, runner};

#[tokio::test]
async fn all_scenarios_pass() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testbench");
    let bench = Bench::load(&dir).expect("testbench loads");
    let mut scenarios: Vec<PathBuf> = std::fs::read_dir(dir.join("scenarios"))
        .expect("scenarios dir")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "tbx"))
        .collect();
    scenarios.sort();
    assert!(!scenarios.is_empty());

    let mut failures = Vec::new();
    for path in &scenarios {
        if let Err(err) = runner::run_scenario(&bench, path).await {
            failures.push(format!("{err:#}"));
        }
    }
    assert!(failures.is_empty(), "failing scenarios:\n{}", failures.join("\n"));
}
