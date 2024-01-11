#![cfg_attr(substrate_runtime, no_std, no_main)]

#[cfg(substrate_runtime)]
mod fixture;

#[cfg(not(substrate_runtime))]
mod binary {
	include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(not(substrate_runtime))]
pub fn binary() -> &'static [u8] {
	let _ = binary::WASM_BINARY_BLOATY;
	binary::WASM_BINARY.unwrap()
}
