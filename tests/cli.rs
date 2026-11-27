use std::process::Command;

#[test]
fn top_level_help_is_available() {
    let output = Command::new(env!("CARGO_BIN_EXE_relay"))
        .arg("--help")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run relay help");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
}

#[test]
fn check_validates_a_plaintext_development_configuration() {
    let config = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("minimal.toml");
    let output = Command::new(env!("CARGO_BIN_EXE_relay"))
        .args(["check", "--config"])
        .arg(config)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run relay check");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("configuration is valid"));
}

#[test]
fn version_command_reports_package_version_and_architecture() {
    let output = Command::new(env!("CARGO_BIN_EXE_relay"))
        .arg("version")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run relay version");
    assert!(output.status.success());
    let version = String::from_utf8_lossy(&output.stdout);
    assert!(version.contains(env!("CARGO_PKG_VERSION")));
    assert!(version.contains(std::env::consts::ARCH));
}
