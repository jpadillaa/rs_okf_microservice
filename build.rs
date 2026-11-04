use std::process::Command;
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-env-changed=OKF_BUILD_COMMIT");
    // Sirve también con .git como archivo (worktree).
    for file in ["HEAD", "index", "packed-refs"] {
        if let Some(path) = git(&["rev-parse", "--git-path", file]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(&["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={path}");
    }
    let commit = std::env::var("OKF_BUILD_COMMIT")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| git(&["rev-parse", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    assert!(
        commit == "unknown"
            || (commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit())),
        "OKF_BUILD_COMMIT debe ser un SHA de 40 caracteres o unknown"
    );
    println!("cargo:rustc-env=OKF_BUILD_COMMIT={commit}");
}
