//! # Your first Node
//!
//! In this guide, you will learn how to run a runtime, possibly generated in
//! [`your_first_runtime`], in a node. Within the context of this guide, we will focus on running a
//! solo-chain node.
//!
//! First, make sure to read all the content covered in
//! [`crate::reference_docs::node_and_chain_spec`]. Once done, you should be familiar with the two
//! main options of launching a node:
//!
//! 1. A template
//! 2. An Omni Node
//!
//! While we will not do it here, it should be a great learning step to try and integrate the
//! runtime built in this guide thus far into the [`minimal_template_runtime`], and then run it with
//! `minimal_template_node`. This will be the former approach.o.
//!
//! In the next section, we will use the latter approach for simplicity. [`your_first_runtime`] uses
//! has no consensus related code, and therefore can only be executed with a node that uses
//! [`sc_consensus_manual_seal`], namely `minima-omni-node`.
//!
//! ## Running Your First (omni) Node.
//!
//! For the rest of this tutorial, we will only focus on running the runtime of the previous guide
//! using the `minimal-omni-node`. This section will be updated with more info once the omni-node
//! feature is fully merged.
//!
//! The process of running a runtime with an omni-node is quite trivial:
//!
//! * compile the runtime (with `--release`)
//! * pass it to the omni-node as the `--runtime`.
//! * Note that we use the [`--tmp`](sc_cli::RunCmd::tmp) flag which spins up a new temporary
//!   database each time.
//! * We enable all `runtime` logs, which helps with the visibility of what is happening in the
//!   runtime. If you now add `frame::log::info!(target: "runtime", ...)` logs in your runtime, you
//!   will see them in the output.
//!
//! ```ignore
//! $ cargo build --release -p polkadot-sdk-docs-packages-guides-first-runtime
//! # assuming the `minimal-omni-node` binary is available from compiling it from
//! # https://github.com/paritytech/polkadot-sdk/pull/3597
//! $ ./minimal-omni-node\
//!     --tmp \
//!     --runtime ./target/release/wbuild/polkadot-sdk-docs-packages-guides-first-runtime/polkadot_sdk_docs_packages_guides_first_runtime.wasm \
//!     -l runtime=debug
//! ```
//!
//! Or the equivalent from the tests of this crate:
#![doc = docify::embed!("./src/guides/your_first_node.rs", node_process)]
//!
//! The only detail that we will cover for now is that to enable an easy way for the runtime to
//! generate some initial state, as it greatly makes the development process easier. For the time
//! being, please pay attention to the implementation of [`build_config`] in the corresponding
//! `Runtime`. For now, we always populate the chian with some funds in all of the accounts named in
//! [`sp_keyring`]. UIs such as Polkadot-JS-Apps are pre-configured to detect and show the balance
//! of these accounts.
//!
//! Once launched, you can navigate to <https://polkadot.js.org/apps/>, connect it to
//! `ws://localhost:9944` and see the chain in action. Please see the corresponding [documentation
//! of Polkadot-JS-Apps](https://polkadot.js.org/docs) for more information.
//!
//! [`build_config`]: polkadot_sdk_docs_packages_guides_first_runtime::Runtime#method.build_config

#[test]
#[ignore = "will not work until we have good omni-nodes in this repo; wait for #3597"]
fn run_omni_node() {
	use nix::{
		sys::signal::{kill, Signal::SIGINT},
		unistd::Pid,
	};
	use std::{
		io::{BufRead, BufReader},
		process,
	};

	// TODO: after #3597 we can easily get this from `cargo_bin`.
	let omni_node_path = "../../minimal-omni-node";
	let runtime_blob_path = "../../target/release/wbuild/polkadot-sdk-docs-packages-guides-first-runtime/polkadot_sdk_docs_packages_guides_first_runtime.wasm";

	if !std::path::Path::new(runtime_blob_path).exists() {
		// run `cargo build --release -p polkadot-sdk-docs-packages-guides-first-runtime`
		assert_cmd::Command::new("cargo")
			.arg("build")
			.arg("--release")
			.arg("-p")
			.arg("polkadot-sdk-docs-packages-guides-first-runtime")
			.assert()
			.success();
	}

	// run the following for 30s
	#[docify::export_content(node_process)]
	fn get_node_process(omni_node_path: &str, runtime_blob_path: &str) -> process::Child {
		process::Command::new(omni_node_path)
			// set current dir to two parents.
			.arg("--tmp")
			.args(["--runtime", runtime_blob_path])
			.args(["-l", "runtime=debug"])
			.args(["--consensus", "manual-seal-1000"])
			.stderr(process::Stdio::piped())
			.spawn()
			.unwrap()
	}
	let mut node_process = get_node_process(omni_node_path, runtime_blob_path);

	std::thread::sleep(std::time::Duration::from_secs(15));
	let stderr = node_process.stderr.take().unwrap();

	kill(Pid::from_raw(node_process.id().try_into().unwrap()), SIGINT).unwrap();
	assert!(node_process.wait().unwrap().success());

	// ensure in stderr there is at least one line containing: "Imported #10"
	assert!(
		BufReader::new(stderr).lines().any(|l| { l.unwrap().contains("Imported #10") }),
		"failed to find 10 imported blocks in the output.",
	);
}
