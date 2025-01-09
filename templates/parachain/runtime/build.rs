use std::process::Command;

#[cfg(all(feature = "std", feature = "metadata-hash"))]
#[docify::export(template_enable_metadata_hash)]
fn main() {
	println!("cargo:rerun-if-changed=../../parachain/runtime/src/genesis_config_presets.rs");
	substrate_wasm_builder::WasmBuilder::init_with_defaults()
		.enable_metadata_hash("UNIT", 12)
		.build();
	let docify_command = Command::new("cargo build")
		.args(&["build"])
		.current_dir("../../")
		.output()
		.expect("Failed to execute docify");

	// Print the output of the docify command for debugging
	println!("docify output: {:?}", docify_command);

	println!("cargo:rerun-if-changed=../../parachain/runtime/src/genesis_config_presets.rs");

	if !docify_command.status.success() {
		eprintln!("Failed to run docify: {:?}", docify_command);
		std::process::exit(1);
	}
}

#[cfg(all(feature = "std", not(feature = "metadata-hash")))]
fn main() {
	substrate_wasm_builder::WasmBuilder::build_using_defaults();
}

/// The wasm builder is deactivated when compiling
/// this crate for wasm to speed up the compilation.
#[cfg(not(feature = "std"))]
fn main() {}
