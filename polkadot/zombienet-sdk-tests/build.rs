// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use sc_executor::WasmExecutor;
use std::{
	env, fs, path,
	path::{Path, PathBuf},
	process::Command,
};

macro_rules! debug_output {
    ($($tokens: tt)*) => {
       if env::var("ZOMBIE_METADATA_BUILD_DEBUG").is_ok() {
            println!("cargo:warning={}", format!($($tokens)*))
        }
    }
}

fn replace_dashes(k: &str) -> String {
	k.replace('-', "_")
}

fn make_env_key(k: &str) -> String {
	replace_dashes(&k.to_ascii_uppercase())
}

fn wasm_sub_path(chain: &str) -> String {
	let (package, runtime_name) =
		(format!("{chain}-runtime"), replace_dashes(&format!("{chain}-runtime")));

	format!("{package}/{runtime_name}.wasm")
}

fn find_wasm(chain: &str) -> Option<PathBuf> {
	const PROFILES: [&str; 2] = ["release", "testnet"];
	let manifest_path = env::var("CARGO_WORKSPACE_ROOT_DIR").unwrap();
	let manifest_path = manifest_path.strip_suffix('/').unwrap();
	debug_output!("manifest_path is  : {}", manifest_path);

	let sub_path = wasm_sub_path(chain);

	let profile = PROFILES.into_iter().find(|p| {
		let full_path = format!("{manifest_path}/target/{p}/wbuild/{sub_path}");
		debug_output!("checking wasm at : {}", full_path);
		matches!(path::PathBuf::from(&full_path).try_exists(), Ok(true))
	});

	debug_output!("profile is : {:?}", profile);
	profile.map(|profile| {
		PathBuf::from(&format!("{manifest_path}/target/{profile}/wbuild/{sub_path}"))
	})
}

// based on https://gist.github.com/s0me0ne-unkn0wn/bbd83fe32ce10327086adbf13e750eec
fn build_wasm(chain: &str) -> PathBuf {
	let package = format!("{chain}-runtime");

	let cargo = env::var("CARGO").unwrap();
	let target = env::var("TARGET").unwrap();
	let out_dir = env::var("OUT_DIR").unwrap();
	let target_dir = format!("{out_dir}/runtimes");
	let args = vec![
		"build",
		"-p",
		&package,
		"--profile",
		"release",
		"--target",
		&target,
		"--target-dir",
		&target_dir,
	];
	debug_output!("building metadata with args: {}", args.join(" "));
	Command::new(cargo)
		.env_remove("SKIP_WASM_BUILD") // force build to get the metadata
		.args(&args)
		.status()
		.unwrap();

	let wasm_path = &format!("{target_dir}/{target}/release/wbuild/{}", wasm_sub_path(chain));
	PathBuf::from(wasm_path)
}

type HostFunctions = (
	sp_io::allocator::HostFunctions,
	sp_io::logging::HostFunctions,
	sp_io::storage::HostFunctions,
	sp_io::hashing::HostFunctions,
);

fn generate_metadata_file(wasm_path: &Path, output_path: &Path) {
	let wasm_bytes = fs::read(wasm_path).expect("Failed to read WASM file");

	let executor = WasmExecutor::<HostFunctions>::builder()
		.with_allow_missing_host_functions(true)
		.build();

	let metadata =
		sc_runtime_utilities::fetch_latest_metadata_from_code_blob(&executor, wasm_bytes.into())
			.expect("Failed to fetch metadata from runtime");

	fs::write(output_path, &*metadata).expect("Failed to write metadata file");
}

fn fetch_metadata_file(chain: &str, output_path: &Path) {
	// First check if we have an explicit path to use
	let env_key = format!("{}_METADATA_FILE", make_env_key(chain));

	if let Ok(path_to_use) = env::var(env_key) {
		debug_output!("metadata file to use (from env): {}\n", path_to_use);
		let metadata_file = PathBuf::from(&path_to_use);
		fs::copy(metadata_file, output_path).unwrap();
	} else if let Some(exisiting_wasm) = find_wasm(chain) {
		debug_output!("exisiting wasm: {:?}", exisiting_wasm);
		// generate metadata
		generate_metadata_file(&exisiting_wasm, output_path);
	} else {
		// build runtime
		let wasm_path = build_wasm(chain);
		debug_output!("created wasm: {:?}", wasm_path);
		// genetate metadata
		generate_metadata_file(&wasm_path, output_path);
	}
}

fn main() {
	if env::var("CARGO_FEATURE_ZOMBIE_METADATA").is_err() {
		debug_output!("zombie-metadata feature not enabled, not need to check metadata files.");
		return;
	}

	// Ensure we have the needed metadata files in place to run zombienet tests
	let manifest_path = env::var("CARGO_MANIFEST_DIR").unwrap();
	const METADATA_DIR: &str = "metadata-files";
	const CHAINS: [&str; 2] = ["rococo", "coretime-rococo"];

	let metadata_path = format!("{manifest_path}/{METADATA_DIR}");

	for chain in CHAINS {
		let full_path = format!("{metadata_path}/{chain}-local.scale");
		let output_path = path::PathBuf::from(&full_path);

		match output_path.try_exists() {
			Ok(true) => {
				debug_output!("got: {}", full_path);
			},
			_ => {
				debug_output!("needs: {}", full_path);
				fetch_metadata_file(chain, &output_path);
			},
		};
	}

	substrate_build_script_utils::generate_cargo_keys();
	substrate_build_script_utils::rerun_if_git_head_changed();
	println!("cargo:rerun-if-changed={metadata_path}");
}
