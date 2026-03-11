use std::process::Command;

fn main() {
    // Embed git commit hash (short) at compile time.
    // Falls back to GIT_HASH env var (for Docker builds without .git/).
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .or_else(|| std::env::var("GIT_HASH").ok())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_HASH={hash}");

    // Embed build date (YYYYMMDD).
    // Falls back to BUILD_DATE env var (for Docker builds).
    let date = Command::new("date")
        .arg("+%Y%m%d")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .or_else(|| std::env::var("BUILD_DATE").ok())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_DATE={date}");

    // Only re-run if git HEAD changes or build.rs itself changes.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=build.rs");
}
