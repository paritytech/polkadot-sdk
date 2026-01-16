use std::{process::Command, str};

const WASM_FILE_PATH: &str =
	"../../../../../target/release/wbuild/westend-runtime/westend-runtime-compact-compressed.wasm";

const FRAME_OMNI_BENCHER_PATH: &str = "../../../../../target/release/frame-omni-bencher";


fn install_frame_omni_bencher() -> &'static str {
	let _ = std::process::Command::new("cargo")
		.arg("build")
		.arg("-p")
		.arg("frame-omni-bencher")
		.arg("--release")
		.status()
		.expect("Failed to execute command");
	WASM_FILE_PATH
}

fn build_westend_runtime() -> &'static str {
	let _ = std::process::Command{"cargo"}
		.arg("build")
		.arg("--release")
		.arg("-p")
		.arg("westend-runtime")
		.arg("--features")
		.arg("runtime-benchmarks")
		.arg("--release")
		.status()
		.expect("Failed to execute command");
	WASM_FILE_PATH
}

fn pallet_utility_bench(wasm_file_path: &'static str, frame_omni_bencher_path: &'static str) {

	let _ = std::process::Command::new(frame_omni_bencher_path)
        .arg("v1")
        .arg("benchmark")
        .arg("pallet")
        .arg("--runtime")
        .arg(wasm_file_path)
        .arg("--pallet")
        .arg("pallet-utility")
        .arg("--extrinsic")
        .arg("")
		.expect("Failed to execute");
}

#[test]
#[docify::export]
fn test_pallet_utility_bench() {
	let output_2 = Command::new(install_frame_omni_bencher());
	let output_1 = Command::new(build_westend_runtime());
	let _ = Command::new(pallet_utility_bench(output_1, output_2));
}