use chrono::Local;
use std::env;
use std::process::Command;

fn main() {
    let build_time = Local::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);

    let rustc_version = env::var("RUSTC").unwrap_or_default();
    println!("cargo:rustc-env=RUSTC_VERSION={}", rustc_version);

    let git_hash = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", git_hash);

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=HOSTNAME={}", hostname);

    let profile = env::var("PROFILE").unwrap_or_default();
    println!("cargo:rustc-env=BUILD_PROFILE={}", profile);
}
