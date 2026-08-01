use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn git_version() -> String {
    let commit = Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let Some(commit) = commit else {
        return "unknown".to_string();
    };
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .is_ok_and(|o| !o.stdout.is_empty());
    if dirty {
        format!("{commit}-dirty")
    } else {
        commit
    }
}

fn watch_git_dir() {
    let Ok(out) = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
    else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    println!("cargo:rerun-if-changed={}", dir.join("HEAD").display());
    println!("cargo:rerun-if-changed={}", dir.join("index").display());
}

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");

    println!("cargo:rustc-env=GIT_VERSION={}", git_version());
    watch_git_dir();
}
