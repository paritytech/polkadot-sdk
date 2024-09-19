use crate::generator::TypeGenerator;
use anyhow::Context;
use std::path::Path;

#[macro_use]
extern crate lazy_static;
mod generator;
mod open_rpc;
mod printer;

fn main() -> anyhow::Result<()> {
	let specs = generator::read_specs()?;

	let mut generator = TypeGenerator::new();
	generator.collect_extra_type("TransactionUnsigned");

	let out_dir = if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
		Path::new(&dir).join("../src/api")
	} else {
		"..".into()
	}
	.canonicalize()?;

	let out = out_dir.join("rpc_methods.rs");
	println!("Generating rpc_methods at {out:?}");
	format_and_write_file(&out, &generator.generate_rpc_methods(&specs))
		.with_context(|| format!("Failed to generate code to {out:?}"))?;

	let out = std::fs::canonicalize(out_dir.join("rpc_types.rs"))?;
	println!("Generating rpc_types at {out:?}");
	format_and_write_file(&out, &generator.generate_types(&specs))
		.with_context(|| format!("Failed to generate code to {out:?}"))?;

	Ok(())
}

fn format_and_write_file(path: &Path, content: &str) -> anyhow::Result<()> {
	use std::{io::Write, process::*};
	let mut rustfmt = Command::new("rustup")
		.args(&["run", "nightly", "rustfmt"])
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.spawn()?;

	let stdin = rustfmt.stdin.as_mut().expect("Failed to open stdin");
	stdin.write_all(content.as_bytes())?;

	// Capture the formatted output from rustfmt
	let output = rustfmt.wait_with_output()?;

	if !output.status.success() {
		anyhow::bail!("rustfmt failed: {}", String::from_utf8_lossy(&output.stderr));
	}

	let code = String::from_utf8_lossy(&output.stdout).to_string();
	std::fs::write(path, code).expect("Unable to write file");
	Ok(())
}
