use std::process::Command;

#[test]
fn no_arguments_prints_input_device_listing_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_vollminputd"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("--instance <NAME>"));
    assert!(stderr.contains("--list-input-devices"));
    assert!(stderr.contains("列出可选的音频输入设备"));
}

#[test]
fn help_flag_prints_help_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_vollminputd"))
        .arg("--help")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success());
    assert!(stderr.contains("--list-input-devices"));
}
