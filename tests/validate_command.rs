use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_okf-microservice");
#[test]
fn validate_command_reports_success_and_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let attempt = tmp.path().join("attempt");
    std::fs::create_dir(&attempt).unwrap();
    let converted = Command::new(BIN)
        .arg("convert")
        .arg("examples/docx/breve.docx")
        .arg(&attempt)
        .arg("breve.docx")
        .output()
        .unwrap();
    assert!(
        converted.status.success(),
        "{}",
        String::from_utf8_lossy(&converted.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&converted.stdout).unwrap();
    let bundle = result["bundle"].as_str().unwrap();
    assert!(
        Command::new(BIN)
            .args(["validate", bundle])
            .output()
            .unwrap()
            .status
            .success()
    );
    std::fs::write(bundle, b"not a zip").unwrap();
    let bad = Command::new(BIN)
        .args(["validate", bundle])
        .output()
        .unwrap();
    assert!(!bad.status.success() && !bad.stderr.is_empty());
}
