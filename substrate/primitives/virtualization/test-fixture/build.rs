#[cfg(not(substrate_runtime))]
fn main() {
	std::env::set_var("SUBSTRATE_RUNTIME_TARGET", "riscv");
	substrate_wasm_builder::WasmBuilder::new().with_current_project().build()
}

#[cfg(substrate_runtime)]
fn main() {}
