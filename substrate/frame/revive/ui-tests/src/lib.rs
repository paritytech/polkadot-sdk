// mod ui;
mod pallet_dummy;
pub mod runtime;



#[test]
fn precompile_ui() {
	let t = trybuild::TestCases::new();
	// t.compile_fail("tests/ui/precompiles_ui.rs");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/ui/precompiles_ui.rs");
    t.compile_fail(path.to_str().unwrap());
}