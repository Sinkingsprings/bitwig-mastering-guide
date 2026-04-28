use std::path::PathBuf;
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bundle") => bundle(&args[1..]),
        Some(cmd) => eprintln!("Unknown command: {cmd}"),
        None => eprintln!("Usage: cargo xtask bundle [--release]"),
    }
}

fn bundle(args: &[String]) {
    let release = args.contains(&"--release".to_string());
    let mut cmd = Command::new("cargo");
    cmd.args(["build", "--package", "mastering_guide"]);
    if release {
        cmd.arg("--release");
    }
    let status = cmd.status().expect("Failed to run cargo build");
    if !status.success() {
        std::process::exit(1);
    }

    let profile = if release { "release" } else { "debug" };
    let src = format!("target/{profile}/libmastering_guide.so");

    // Staging copy (target/…/bundled/)
    let dst_dir = PathBuf::from(format!("target/{profile}/bundled"));
    std::fs::create_dir_all(&dst_dir).unwrap();
    let staged = dst_dir.join("MasteringGuide.clap");
    std::fs::copy(&src, &staged).expect("Failed to copy .clap to bundled/");
    println!("Bundled:   {}", staged.display());

    // Install to ~/.clap/ so Bitwig picks it up on next scan/restart
    let install_dir = PathBuf::from(std::env::var("HOME").expect("HOME not set")).join(".clap");
    std::fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("MasteringGuide.clap");
    std::fs::copy(&src, &installed).expect("Failed to copy .clap to ~/.clap/");
    println!("Installed: {}", installed.display());
}
