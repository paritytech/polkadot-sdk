#[cfg(all(feature = "std", feature = "metadata-hash"))]
#[docify::export(template_enable_metadata_hash)]
fn main() {
	substrate_wasm_builder::WasmBuilder::init_with_defaults()
		.enable_metadata_hash("UNIT", 12)
		.append_to_rust_flags("-Clink-args=--initial-memory=67108864 --max-memory=67108864")
		.build();
}

#[cfg(all(feature = "std", not(feature = "metadata-hash")))]
fn main() {
	substrate_wasm_builder::WasmBuilder::init_with_defaults()
		.append_to_rust_flags("-Clink-args=--initial-memory=67108864")
		.append_to_rust_flags("-Clink-args=--max-memory=67108864")
		.build();
}

/// The wasm builder is deactivated when compiling
/// this crate for wasm to speed up the compilation.
#[cfg(not(feature = "std"))]
fn main() {}
