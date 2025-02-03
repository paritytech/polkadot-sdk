//! This test is setup to run with the `native` provider and needs these binaries in your PATH
//! `polkadot`, `polkadot-prepare-worker`, `polkadot-execute-worker`, `parachain-template-node`.
//! You can follow these steps to compile and export the binaries:
//! `cargo build --release -features fast-runtime --bin polkadot --bin polkadot-execute-worker --bin
//! polkadot-prepare-worker`
//! `cargo build --package parachain-template-node --release`
//! `cargo build --package minimal-template-node --release`
//! `export PATH=<path-to-polkadot-sdk-repo>/target/release:$PATH
//!
//! There are also some tests related to omni node which run basaed on pre-generated chain specs,
//! so to be able to run them you would need to generate the right chain spec (just minimal and
//! parachain tests supported for now).
//!
//! You can run the following command to generate a minimal chainspec, once the runtime wasm file is
//! compiled:
//!`chain-spec-builder create --relay-chain <relay_chain_id> --para-id 1000 -r \
//!     <path_to_template_wasm_file> named-preset development`
//!
//! Once the files are generated, you must export an environment variable called
//! `CHAIN_SPECS_DIR` which should point to the absolute path of the directory
//! that holds the generated chain specs. The chain specs file names should be
//! `minimal_chain_spec.json` for minimal and `parachain_chain_spec.json` for parachain
//! templates.
//!
//! To start all tests here we should run:
//! `cargo test -p template-zombienet-tests --features zombienet`

#[cfg(feature = "zombienet")]
mod smoke {
	use std::path::PathBuf;

	use anyhow::anyhow;
	use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder, NetworkConfigExt};

	const CHAIN_SPECS_DIR_PATH: &str = "CHAIN_SPECS_DIR";
	const PARACHAIN_ID: u32 = 1000;

	#[inline]
	fn expect_env_var(var_name: &str) -> String {
		std::env::var(var_name)
			.unwrap_or_else(|_| panic!("{CHAIN_SPECS_DIR_PATH} environment variable is set. qed."))
	}

	#[derive(Default)]
	struct NetworkSpec {
		relaychain_cmd: &'static str,
		relaychain_spec_path: Option<PathBuf>,
		// TODO: update the type to something like Option<Vec<Arg>> after
		// `zombienet-sdk` exposes `shared::types::Arg`.
		relaychain_cmd_args: Option<Vec<(&'static str, &'static str)>>,
		para_cmd: Option<&'static str>,
		para_cmd_args: Option<Vec<(&'static str, &'static str)>>,
	}

	fn get_config(network_spec: NetworkSpec) -> Result<NetworkConfig, anyhow::Error> {
		let chain = if network_spec.relaychain_cmd == "polkadot" { "rococo-local" } else { "dev" };
		let config = NetworkConfigBuilder::new().with_relaychain(|r| {
			let mut r = r.with_chain(chain).with_default_command(network_spec.relaychain_cmd);
			if let Some(path) = network_spec.relaychain_spec_path {
				r = r.with_chain_spec_path(path);
			}

			if let Some(args) = network_spec.relaychain_cmd_args {
				r = r.with_default_args(args.into_iter().map(|arg| arg.into()).collect());
			}

			r.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		});

		let config = if let Some(para_cmd) = network_spec.para_cmd {
			config.with_parachain(|p| {
				let mut p = p.with_id(PARACHAIN_ID).with_default_command(para_cmd);
				if let Some(args) = network_spec.para_cmd_args {
					p = p.with_default_args(args.into_iter().map(|arg| arg.into()).collect());
				}
				p.with_collator(|n| n.with_name("collator"))
			})
		} else {
			config
		};

		config.build().map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn parachain_template_block_production_test() -> Result<(), anyhow::Error> {
		let _ = env_logger::try_init_from_env(
			env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
		);

		let config = get_config(NetworkSpec {
			relaychain_cmd: "polkadot",
			para_cmd: Some("parachain-template-node"),
			..Default::default()
		})?;

		let network = config.spawn_native().await?;

		// wait 6 blocks of the para
		let collator = network.get_node("collator")?;
		assert!(collator
			.wait_metric("block_height{status=\"finalized\"}", |b| b > 5_f64)
			.await
			.is_ok());

		Ok(())
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn solochain_template_block_production_test() -> Result<(), anyhow::Error> {
		let _ = env_logger::try_init_from_env(
			env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
		);

		let config = get_config(NetworkSpec {
			relaychain_cmd: "solochain-template-node",
			..Default::default()
		})?;

		let network = config.spawn_native().await?;

		// wait 6 blocks
		let alice = network.get_node("alice")?;
		assert!(alice
			.wait_metric("block_height{status=\"finalized\"}", |b| b > 5_f64)
			.await
			.is_ok());

		Ok(())
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn minimal_template_block_production_test() -> Result<(), anyhow::Error> {
		let _ = env_logger::try_init_from_env(
			env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
		);

		let config = get_config(NetworkSpec {
			relaychain_cmd: "minimal-template-node",
			..Default::default()
		})?;

		let network = config.spawn_native().await?;

		// wait 6 blocks
		let alice = network.get_node("alice")?;
		assert!(alice
			.wait_metric("block_height{status=\"finalized\"}", |b| b > 5_f64)
			.await
			.is_ok());

		Ok(())
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn omni_node_with_minimal_runtime_block_production_test() -> Result<(), anyhow::Error> {
		let _ = env_logger::try_init_from_env(
			env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
		);

		let chain_spec_path = expect_env_var(CHAIN_SPECS_DIR_PATH) + "/minimal_chain_spec.json";
		let config = get_config(NetworkSpec {
			relaychain_cmd: "polkadot-omni-node",
			relaychain_cmd_args: Some(vec![("--dev-block-time", "1000")]),
			relaychain_spec_path: Some(chain_spec_path.into()),
			..Default::default()
		})?;
		let network = config.spawn_native().await?;

		// wait 6 blocks
		let alice = network.get_node("alice")?;
		assert!(alice
			.wait_metric("block_height{status=\"finalized\"}", |b| b > 5_f64)
			.await
			.is_ok());

		Ok(())
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn omni_node_with_parachain_runtime_block_production_test() -> Result<(), anyhow::Error> {
		let _ = env_logger::try_init_from_env(
			env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
		);

		let chain_spec_path = expect_env_var(CHAIN_SPECS_DIR_PATH) + "/parachain_chain_spec.json";

		let config = get_config(NetworkSpec {
			relaychain_cmd: "polkadot",
			para_cmd: Some("polkadot-omni-node"),
			// Leaking the `String` to be able to use it below as a static str,
			// required by the `FromStr` implementation for zombienet-configuration
			// `Arg` type, which is not exposed yet through `zombienet-sdk`.
			para_cmd_args: Some(vec![("--chain", chain_spec_path.leak())]),
			..Default::default()
		})?;
		let network = config.spawn_native().await?;

		// wait 6 blocks
		let alice = network.get_node("collator")?;
		assert!(alice
			.wait_metric("block_height{status=\"finalized\"}", |b| b > 5_f64)
			.await
			.is_ok());

		Ok(())
	}
}
