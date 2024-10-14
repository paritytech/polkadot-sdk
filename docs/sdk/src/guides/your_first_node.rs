//! # Your first Node
//!
//! In this guide, you will learn how to run a runtime, such as the one created in
//! [`your_first_runtime`], in a node. Within the context of this guide, we will focus on running
//! the runtime with an [`omni-node`]. Please first read this page to learn about the OmniNode, and
//! other options when it comes to running a node.
//!
//! [`your_first_runtime`] is a runtime with no consensus related code, and therefore can only be
//! executed with a node that also expects no consensus ([`sc_consensus_manual_seal`]).
//! `polkadot_omni_node`'s [`--dev-block-time`] precisely does this.
//!
//! ## Running The Omni Node
//!
//! The `polkadot-omni-node` can either be downloaded from the latest [Release](https://github.com/paritytech/polkadot-sdk/releases/) of `polkadot-sdk`,
//! or installed using `cargo`:
//!
//! ```text
//! cargo install polkadot-omni-node
//! ```
//!
//! Dump:
//! ```text
//! ./chain-spec-builder create --para-id 42 --relay-chain dontcare --runtime polkadot_sdk_docs_first_runtime.wasm named-preset development
//! ./polkadot-omni-node --tmp --dev-block-time 100 --chain polkadot_sdk_docs_first_runtime.json
//! ./
//! ```
//!
//! [`runtime`]: crate::reference_docs::glossary#runtime
//! [`node`]: crate::reference_docs::glossary#node
//! [`build_config`]: first_runtime::Runtime#method.build_config
//! [`omni-node`]: crate::reference_docs::omni_node
//! [`--dev-block-time`]: (polkadot_omni_node_lib::cli::Cli::dev_block_time)

#[cfg(test)]
mod tests {
	use nix::{
		sys::signal::{kill, Signal::SIGINT},
		unistd::Pid,
	};
	use sp_genesis_builder::PresetId;
	use std::{
		env,
		io::{BufRead, BufReader},
		path::Path,
		process,
	};

	const PARA_RUNTIME_PATH: &'static str =
		"target/release/wbuild/parachain-template-runtime/parachain_template_runtime.wasm";
	const FIRST_RUNTIME_PATH: &'static str =
		"target/release/wbuild/polkadot-sdk-docs-first-runtime/polkadot_sdk_docs_first_runtime.wasm";

	// TODO: `CARGO_MANIFEST_DIR`
	// TODO: minimal runtime should also be tested.

	const CHAIN_SPEC_BUILDER: &'static str = "target/release/chain-spec-builder";
	const OMNI_NODE: &'static str = "target/release/polkadot-omni-node";

	fn maybe_build_runtimes() {
		if !Path::new(PARA_RUNTIME_PATH).exists() {
			println!("Building parachain runtime...");
			assert_cmd::Command::new("cargo")
				.arg("build")
				.arg("--release")
				.arg("-p")
				.arg("parachain-template-runtime")
				.assert()
				.success();
		}
		if !Path::new(FIRST_RUNTIME_PATH).exists() {
			println!("Building first runtime...");
			assert_cmd::Command::new("cargo")
				.arg("build")
				.arg("--release")
				.arg("-p")
				.arg("polkadot-sdk-docs-first-runtime")
				.assert()
				.success();
		}
		assert!(Path::new(FIRST_RUNTIME_PATH).exists(), "runtime must now exist!");
		assert!(Path::new(PARA_RUNTIME_PATH).exists(), "runtime must now exist!");
	}

	fn maybe_build_chain_spec_builder() {
		// build chain-spec-builder if it does not exist
		if !Path::new(CHAIN_SPEC_BUILDER).exists() {
			println!("Building chain-spec-builder...");
			assert_cmd::Command::new("cargo")
				.arg("build")
				.arg("--release")
				.arg("-p")
				.arg("staging-chain-spec-builder")
				.assert()
				.success();
		}
		assert!(Path::new(CHAIN_SPEC_BUILDER).exists(), "chain-spec-builder must now exist!");
	}

	fn maybe_build_omni_node() {
		// build polkadot-omni-node if it does not exist
		if !Path::new(OMNI_NODE).exists() {
			println!("Building omni-node...");
			assert_cmd::Command::new("cargo")
				.arg("build")
				.arg("--release")
				.arg("-p")
				.arg("polkadot-omni-node")
				.assert()
				.success();
		}
		assert!(Path::new(OMNI_NODE).exists(), "dev-omni-node must now exist!");
	}

	fn ensure_node_process_works(node_cmd: &mut process::Command) {
		let mut node_process = node_cmd.spawn().unwrap();

		std::thread::sleep(std::time::Duration::from_secs(15));
		let stderr = node_process.stderr.take().unwrap();

		kill(Pid::from_raw(node_process.id().try_into().unwrap()), SIGINT).unwrap();
		let exit_status = node_process.wait().unwrap();
		println!("Exit status: {:?}", exit_status);

		// ensure in stderr there is at least one line containing: "Imported #10"
		assert!(
			BufReader::new(stderr).lines().any(|l| { l.unwrap().contains("Imported #10") }),
			"failed to find 10 imported blocks in the output.",
		);
	}

	fn test_runtime_preset(runtime: &'static str, maybe_preset: Option<PresetId>) {
		// set working directory to project root, 2 parents
		std::env::set_current_dir(std::env::current_dir().unwrap().join("../..")).unwrap();
		// last segment of cwd must now be `polkadot-sdk`
		assert!(dbg!(std::env::current_dir().unwrap()).ends_with("polkadot-sdk"));

		maybe_build_runtimes();
		maybe_build_chain_spec_builder();
		maybe_build_omni_node();

		process::Command::new(CHAIN_SPEC_BUILDER)
			.arg("create")
			.args(["-r", runtime])
			.args(match maybe_preset {
				Some(preset) => vec!["named-preset".to_string(), preset.to_string()],
				None => vec!["default".to_string()],
			})
			.stderr(process::Stdio::piped())
			.spawn()
			.unwrap();

		// join current dir and chain_spec.json
		let chain_spec_file = std::env::current_dir().unwrap().join("chain_spec.json");

		let mut binding = process::Command::new(OMNI_NODE);
		let node_cmd = &mut binding
			.arg("--tmp")
			.arg("--dev-block-time 500")
			.args(["--chain", chain_spec_file.to_str().unwrap()])
			.stderr(process::Stdio::piped());

		ensure_node_process_works(node_cmd);

		// delete chain_spec.json
		std::fs::remove_file(chain_spec_file).unwrap();
	}

	#[test]
	fn parachain_runtime_works() {
		test_runtime_preset(PARA_RUNTIME_PATH, None);
		test_runtime_preset(PARA_RUNTIME_PATH, Some(sp_genesis_builder::DEV_RUNTIME_PRESET));
		test_runtime_preset(
			PARA_RUNTIME_PATH,
			Some(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
		);
	}

	#[test]
	fn guide_first_runtime_works() {
		test_runtime_preset(PARA_RUNTIME_PATH, None);
		test_runtime_preset(PARA_RUNTIME_PATH, Some(sp_genesis_builder::DEV_RUNTIME_PRESET));
		test_runtime_preset(
			PARA_RUNTIME_PATH,
			Some(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
		);
	}
}
