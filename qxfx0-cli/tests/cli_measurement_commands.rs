use std::process::Command;

#[test]
fn measurement_commands_are_present_and_exit_successfully() {
    let binary = env!("CARGO_BIN_EXE_qxfx0");

    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("benchmark"));
    assert!(help.contains("renderer-audit"));

    let benchmark = Command::new(binary)
        .args(["benchmark", "--samples", "1", "--warmup", "0", "--json"])
        .output()
        .unwrap();
    assert!(
        benchmark.status.success(),
        "{}",
        String::from_utf8_lossy(&benchmark.stderr)
    );

    let audit = Command::new(binary)
        .args(["renderer-audit", "--opening-words", "3", "--json"])
        .output()
        .unwrap();
    assert!(
        audit.status.success(),
        "{}",
        String::from_utf8_lossy(&audit.stderr)
    );
}
