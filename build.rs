fn main() {
    slint_build::compile_with_config(
        "ui/gui.slint",
        slint_build::CompilerConfiguration::new()
            .with_style("fluent-dark".into())
            .with_debug_info(true),
    )
    .expect("Slint build failed");
}
