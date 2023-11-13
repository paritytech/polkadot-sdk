fn main() {
	get_wasmtime_version();
}

pub fn get_wasmtime_version() {
	// we only care about the root of the tree
	match std::process::Command::new("cargo")
		.args(&["tree", "--package=wasmtime", "--depth=0"])
		.output()
	{
		Ok(out) if out.status.success() => {
			// wasmtime vX.X.X
			let version = String::from_utf8_lossy(&out.stdout);
			if let Some(version) = version.strip_prefix("wasmtime v") {
				println!("cargo:rustc-env=SUBSTRATE_WASMTIME_VERSION={}", version);
			} else {
				println!("cargo:warning=build.rs: unexpected result {}", version);
			}
		},
		Ok(out) => println!(
			"cargo:warning=build.rs: `cargo tree` {}",
			String::from_utf8_lossy(&out.stderr),
		),
		Err(err) => {
			println!("cargo:warning=build.rs: Could not run `cargo tree`: {}", err);
		},
	}
}
