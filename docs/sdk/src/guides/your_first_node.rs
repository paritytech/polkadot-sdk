//! # Your first Node
//!
//! In this guide, you will learn how to run a runtime, possibly generated in
//! [`your_first_runtime`], in a node. Within the context of this guide, we will focus on running a
//! solo-chain node.
//!
//! In the next section, we will first covert some background knowledge related to the node software
//! before getting to the practical parts. If you want to skip ahead to running the runtime of the
//! previous guide in a node, you can jump to the [Running with Omni Node](#todo) section.
//!
//! ## Node Consideration
//!
//! This is a good point to take a step back, and recap some of the software components that make up
//! the node. Most importantly, the node is composed of:
//!
//! * Consensus Engine
//! * Chain Specification
//! * RPC server, Database, P2P networking, Transaction Pool etc.
//!
//! To learn more about the node, see [`crate::reference_docs::wasm_meta_protocol`].
//!
//! Our main focus will be on the former two.
//!
//! ### Consensus Engine
//!
//! In any given substrate-based chain, both the node and the runtime will have their inherit some
//! information about what consensus engine is going to be used.
//!
//! In practice, the majority of the implementation of any consensus engine is in the node side, but
//! the runtime also typically needs to expose a custom runtime-api to enable the particular
//! consensus engine to work, and that particular runtime-api is implemented by a pallet
//! corresponding to that consensus engine.
//!
//! For example, taking a snippet from [`solochain-template-runtime`], the runtime has to provide
//! this additional runtime-api, if the node software is configured to use Aura:
//!
//! ```ignore
//! impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
//!     fn slot_duration() -> sp_consensus_aura::SlotDuration {
//!         ...
//!     }
//!     fn authorities() -> Vec<AuraId> {
//!         ...
//!     }
//! }
//! ````
//!
//! For simplicity, we can break down "consensus" into two main parts:
//!
//! * Block Authoring: Deciding who gets to produce the next block.
//! * Finality: Deciding when a block is considered final.
//!
//! For block authoring, there are a number of options:
//!
//! * [`sc_consensus_manual_seal`]: Useful for testing, where any node can produce a block.
//! * [`sc_consensus_aura`]/[`pallet_aura`]: A simple round-robin block authoring mechanism.
//! * [`sc_consensus_babe`]/[`pallet_babe`]: A more advanced block authoring mechanism, capable of
//!   anonymizing the next block author.
//! * [`sc_consensus_pow`]: Proof of Work block authoring.
//!
//! For finality, there is one main option shipped with polkadot-sdk:
//!
//! * [`sc_consensus_grandpa`]/[`pallet_grandpa`]: A finality gadget that uses a voting mechanism to
//!   decide when a block
//!
//! **Within the context of this guide, what matters the most is that the node and the runtime must
//! have matching consensus components.**
//!
//! For example, [`your_first_runtime`] uses has no consensus related code, and therefore can only
//! be executed with a node that uses [`sc_consensus_manual_seal`].
//!
//! ### Chain Specification
//!
//! TODO: brief intro into chain, spec, why it matters, how the node can be linked to it. but then
//! forward to [`sc_chain_spec`].
//!
//! ### Node Types
//!
//! This then brings us to explore what options are available to you in terms of node software when
//! using polkadot-sdk. Historically, the one and only way has been to use templates, but we expect
//! more options to be released in 2024.
//!
//! #### Using a Full Node via Templates
//!
//! In this option, your project will contain the full runtime+node software, and the two components
//! are aware of each other's details. For example, in any given template, both the node and the
//! runtime are configured to use the same, and correct consensus.
//!
//! This usually entails a lot of boilerplate code, especially on the node side, and therefore using
//! one of our many [`crate::polkadot_sdk::templates`] is the recommended way to get started with
//! this.
//!
//! The advantage of this option is that you will have full control over customization of your node
//! side components. The downside is that there is more code to maintain, especially when it comes
//! to keeping up with new releases.
//!
//! While we will not do it here, it should be a great learning step to try and integrate the
//! runtime built in this guide thus far into the [`minimal_template_runtime`], and then run it with
//! `minimal_template_node`.
//!
//! #### Using an omni-Node
//!
//! An omni-node is a new term in the polkadot-sdk (see
//! [here](https://github.com/paritytech/polkadot-sdk/pull/3597/) and
//! [here](https://github.com/paritytech/polkadot-sdk/issues/5)) and refers to a node that is
//! capable of running any runtime, so long as a certain set of assumptions are met. One of the most
//! important of such assumptions is that the consensus engine, as explained above, must match
//! between the node and runtime.
//!
//! Therefore we expect to have "one omni-node per consensus type".
//!
//! The end goal with the omni-nodes is for developers to not need to maintain any node software and
//! download binary which can run their runtime.
//!
//! Given polkadot-sdk's path toward totally [deprecating the native
//! runtime](https://github.com/paritytech/polkadot-sdk/issues/62) from one another, using an
//! omni-node is the natural evolution. Read more in
//! [`crate::reference_docs::wasm_meta_protocol#native-runtime`].
//!
//! ## Running Your First (omni) Node.
//!
//! For the rest of this tutorial, we will only focus on running the runtime of the previous guide
//! using the `minimal-omni-node`. This section will be updated with more info once the omni-node
//! feature is fully merged.
//!
//! ```ignore
//! $ cargo build --release -p polkadot-sdk-docs-packages-guides-first-runtime
//! # assuming the `minimal-omni-node` binary is available from compiling it from
//! # https://github.com/paritytech/polkadot-sdk/pull/3597
//! $ ./minimal-omni-node\
//! 	--tmp \
//! 	--runtime ./target/release/wbuild/polkadot-sdk-docs-packages-guides-first-runtime/polkadot_sdk_docs_packages_guides_first_runtime.wasm \
//! 	-l runtime=debug
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
// #[ignore = "will not work until we have good omni-nodes in this repo; wait for #3597"]
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
	let test_duration = 15;
	let expected_blocks = 10;

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
			.spawn()
			.unwrap()
	}
	let mut node_process = get_node_process(omni_node_path, runtime_blob_path);

	std::thread::sleep(std::time::Duration::from_secs(test_duration));
	let stderr = node_process.stderr.take().unwrap();

	kill(Pid::from_raw(node_process.id().try_into().unwrap()), SIGINT).unwrap();
	assert!(node_process.wait().unwrap().success());

	// ensure in stderr there is at least one line containing: "Imported #10"
	assert!(
		BufReader::new(stderr)
			.lines()
			.any(|l| l.unwrap().contains("Imported #{:expected_blocks}")),
		"failed to find {} imported blocks in the output.",
		expected_blocks,
	);
}
