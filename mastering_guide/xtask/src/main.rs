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
    let dst_dir = std::path::PathBuf::from(format!("target/{profile}/bundled"));
    std::fs::create_dir_all(&dst_dir).unwrap();
    let dst = dst_dir.join("MasteringGuide.clap");
    std::fs::copy(&src, &dst).expect("Failed to copy .clap file");
    println!("Bundled: {}", dst.display());
}
