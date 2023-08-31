// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
	chain_spec,
	cli::{Cli, RelayChainCli, Subcommand},
	service::{new_partial, Block},
};
use cumulus_primitives_core::ParaId;
use frame_benchmarking_cli::{BenchmarkCmd, SUBSTRATE_REFERENCE_HARDWARE};
use log::info;
use parachains_common::{AssetHubPolkadotAuraId, AuraId};
use sc_cli::{
	ChainSpec, CliConfiguration, DefaultConfigurationValues, ImportParams, KeystoreParams,
	NetworkParams, Result, SharedParams, SubstrateCli,
};
use sc_service::config::{BasePath, PrometheusConfig};
use sp_runtime::traits::AccountIdConversion;
use std::{net::SocketAddr, path::PathBuf};

/// Helper enum that is used for better distinction of different parachain/runtime configuration
/// (it is based/calculated on ChainSpec's ID attribute)
#[derive(Debug, PartialEq, Default)]
enum Runtime {
	/// This is the default runtime (actually based on rococo)
	#[default]
	Default,
	Shell,
	Seedling,
	AssetHubPolkadot,
	AssetHubKusama,
	AssetHubWestend,
	Penpal(ParaId),
	ContractsRococo,
	CollectivesPolkadot,
	CollectivesWestend,
	Glutton,
	BridgeHub(chain_spec::bridge_hubs::BridgeHubRuntimeType),
}

trait RuntimeResolver {
	fn runtime(&self) -> Runtime;
}

impl RuntimeResolver for dyn ChainSpec {
	fn runtime(&self) -> Runtime {
		runtime(self.id())
	}
}

/// Implementation, that can resolve [`Runtime`] from any json configuration file
impl RuntimeResolver for PathBuf {
	fn runtime(&self) -> Runtime {
		#[derive(Debug, serde::Deserialize)]
		struct EmptyChainSpecWithId {
			id: String,
		}

		let file = std::fs::File::open(self).expect("Failed to open file");
		let reader = std::io::BufReader::new(file);
		let chain_spec: EmptyChainSpecWithId = serde_json::from_reader(reader)
			.expect("Failed to read 'json' file with ChainSpec configuration");

		runtime(&chain_spec.id)
	}
}

fn runtime(id: &str) -> Runtime {
	let id = id.replace('_', "-");
	let (_, id, para_id) = extract_parachain_id(&id);

	if id.starts_with("shell") {
		Runtime::Shell
	} else if id.starts_with("seedling") {
		Runtime::Seedling
	} else if id.starts_with("asset-hub-polkadot") | id.starts_with("statemint") {
		Runtime::AssetHubPolkadot
	} else if id.starts_with("asset-hub-kusama") | id.starts_with("statemine") {
		Runtime::AssetHubKusama
	} else if id.starts_with("asset-hub-westend") | id.starts_with("westmint") {
		Runtime::AssetHubWestend
	} else if id.starts_with("penpal") {
		Runtime::Penpal(para_id.unwrap_or(ParaId::new(0)))
	} else if id.starts_with("contracts-rococo") {
		Runtime::ContractsRococo
	} else if id.starts_with("collectives-polkadot") {
		Runtime::CollectivesPolkadot
	} else if id.starts_with("collectives-westend") {
		Runtime::CollectivesWestend
	} else if id.starts_with(chain_spec::bridge_hubs::BridgeHubRuntimeType::ID_PREFIX) {
		Runtime::BridgeHub(
			id.parse::<chain_spec::bridge_hubs::BridgeHubRuntimeType>()
				.expect("Invalid value"),
		)
	} else if id.starts_with("glutton") {
		Runtime::Glutton
	} else {
		log::warn!("No specific runtime was recognized for ChainSpec's id: '{}', so Runtime::default() will be used", id);
		Runtime::default()
	}
}

fn load_spec(id: &str) -> std::result::Result<Box<dyn ChainSpec>, String> {
	let (id, _, para_id) = extract_parachain_id(id);
	Ok(match id {
		// - Defaul-like
		"staging" =>
			Box::new(chain_spec::rococo_parachain::staging_rococo_parachain_local_config()),
		"tick" =>
			Box::new(chain_spec::rococo_parachain::RococoParachainChainSpec::from_json_bytes(
				&include_bytes!("../chain-specs/tick.json")[..],
			)?),
		"trick" =>
			Box::new(chain_spec::rococo_parachain::RococoParachainChainSpec::from_json_bytes(
				&include_bytes!("../chain-specs/trick.json")[..],
			)?),
		"track" =>
			Box::new(chain_spec::rococo_parachain::RococoParachainChainSpec::from_json_bytes(
				&include_bytes!("../chain-specs/track.json")[..],
			)?),

		// -- Starters
		"shell" => Box::new(chain_spec::shell::get_shell_chain_spec()),
		"seedling" => Box::new(chain_spec::seedling::get_seedling_chain_spec()),

		// -- Asset Hub Polkadot
		"asset-hub-polkadot-dev" | "statemint-dev" =>
			Box::new(chain_spec::asset_hubs::asset_hub_polkadot_development_config()),
		"asset-hub-polkadot-local" | "statemint-local" =>
			Box::new(chain_spec::asset_hubs::asset_hub_polkadot_local_config()),
		// the chain spec as used for generating the upgrade genesis values
		"asset-hub-polkadot-genesis" | "statemint-genesis" =>
			Box::new(chain_spec::asset_hubs::asset_hub_polkadot_config()),
		// the shell-based chain spec as used for syncing
		"asset-hub-polkadot" | "statemint" =>
			Box::new(chain_spec::asset_hubs::AssetHubPolkadotChainSpec::from_json_bytes(
				&include_bytes!("../chain-specs/asset-hub-polkadot.json")[..],
			)?),

		// -- Asset Hub Kusama
		"asset-hub-kusama-dev" | "statemine-dev" =>
			Box::new(chain_spec::asset_hubs::asset_hub_kusama_development_config()),
		"asset-hub-kusama-local" | "statemine-local" =>
			Box::new(chain_spec::asset_hubs::asset_hub_kusama_local_config()),
		// the chain spec as used for generating the upgrade genesis values
		"asset-hub-kusama-genesis" | "statemine-genesis" =>
			Box::new(chain_spec::asset_hubs::asset_hub_kusama_config()),
		// the shell-based chain spec as used for syncing
		"asset-hub-kusama" | "statemine" =>
			Box::new(chain_spec::asset_hubs::AssetHubKusamaChainSpec::from_json_bytes(
				&include_bytes!("../chain-specs/asset-hub-kusama.json")[..],
			)?),

		// -- Asset Hub Westend
		"asset-hub-westend-dev" | "westmint-dev" =>
			Box::new(chain_spec::asset_hubs::asset_hub_westend_development_config()),
		"asset-hub-westend-local" | "westmint-local" =>
			Box::new(chain_spec::asset_hubs::asset_hub_westend_local_config()),
		// the chain spec as used for generating the upgrade genesis values
		"asset-hub-westend-genesis" | "westmint-genesis" =>
			Box::new(chain_spec::asset_hubs::asset_hub_westend_config()),
		// the shell-based chain spec as used for syncing
		"asset-hub-westend" | "westmint" =>
			Box::new(chain_spec::asset_hubs::AssetHubWestendChainSpec::from_json_bytes(
				&include_bytes!("../chain-specs/asset-hub-westend.json")[..],
			)?),

		// -- Polkadot Collectives
		"collectives-polkadot-dev" =>
			Box::new(chain_spec::collectives::collectives_polkadot_development_config()),
		"collectives-polkadot-local" =>
			Box::new(chain_spec::collectives::collectives_polkadot_local_config()),
		"collectives-polkadot" =>
			Box::new(chain_spec::collectives::CollectivesPolkadotChainSpec::from_json_bytes(
				&include_bytes!("../chain-specs/collectives-polkadot.json")[..],
			)?),
		"collectives-westend" =>
			Box::new(chain_spec::collectives::CollectivesPolkadotChainSpec::from_json_bytes(
				&include_bytes!("../chain-specs/collectives-westend.json")[..],
			)?),

		// -- Contracts on Rococo
		"contracts-rococo-dev" =>
			Box::new(chain_spec::contracts::contracts_rococo_development_config()),
		"contracts-rococo-local" =>
			Box::new(chain_spec::contracts::contracts_rococo_local_config()),
		"contracts-rococo-genesis" => Box::new(chain_spec::contracts::contracts_rococo_config()),
		"contracts-rococo" =>
			Box::new(chain_spec::contracts::ContractsRococoChainSpec::from_json_bytes(
				&include_bytes!("../chain-specs/contracts-rococo.json")[..],
			)?),

		// -- BridgeHub
		bridge_like_id
			if bridge_like_id
				.starts_with(chain_spec::bridge_hubs::BridgeHubRuntimeType::ID_PREFIX) =>
			bridge_like_id
				.parse::<chain_spec::bridge_hubs::BridgeHubRuntimeType>()
				.expect("invalid value")
				.load_config()?,

		// -- Penpall
		"penpal-kusama" => Box::new(chain_spec::penpal::get_penpal_chain_spec(
			para_id.expect("Must specify parachain id"),
			"kusama-local",
		)),
		"penpal-polkadot" => Box::new(chain_spec::penpal::get_penpal_chain_spec(
			para_id.expect("Must specify parachain id"),
			"polkadot-local",
		)),

		// -- Glutton
		"glutton-kusama-dev" => Box::new(chain_spec::glutton::glutton_development_config(
			para_id.expect("Must specify parachain id"),
		)),
		"glutton-kusama-local" => Box::new(chain_spec::glutton::glutton_local_config(
			para_id.expect("Must specify parachain id"),
		)),
		// the chain spec as used for generating the upgrade genesis values
		"glutton-kusama-genesis" => Box::new(chain_spec::glutton::glutton_config(
			para_id.expect("Must specify parachain id"),
		)),

		// -- Fallback (generic chainspec)
		"" => {
			log::warn!("No ChainSpec.id specified, so using default one, based on rococo-parachain runtime");
			Box::new(chain_spec::rococo_parachain::rococo_parachain_local_config())
		},

		// -- Loading a specific spec from disk
		path => {
			let path: PathBuf = path.into();
			match path.runtime() {
				Runtime::AssetHubPolkadot => Box::new(
					chain_spec::asset_hubs::AssetHubPolkadotChainSpec::from_json_file(path)?,
				),
				Runtime::AssetHubKusama =>
					Box::new(chain_spec::asset_hubs::AssetHubKusamaChainSpec::from_json_file(path)?),
				Runtime::AssetHubWestend => Box::new(
					chain_spec::asset_hubs::AssetHubWestendChainSpec::from_json_file(path)?,
				),
				Runtime::CollectivesPolkadot | Runtime::CollectivesWestend => Box::new(
					chain_spec::collectives::CollectivesPolkadotChainSpec::from_json_file(path)?,
				),
				Runtime::Shell =>
					Box::new(chain_spec::shell::ShellChainSpec::from_json_file(path)?),
				Runtime::Seedling =>
					Box::new(chain_spec::seedling::SeedlingChainSpec::from_json_file(path)?),
				Runtime::ContractsRococo =>
					Box::new(chain_spec::contracts::ContractsRococoChainSpec::from_json_file(path)?),
				Runtime::BridgeHub(bridge_hub_runtime_type) =>
					bridge_hub_runtime_type.chain_spec_from_json_file(path)?,
				Runtime::Penpal(_para_id) =>
					Box::new(chain_spec::penpal::PenpalChainSpec::from_json_file(path)?),
				Runtime::Glutton =>
					Box::new(chain_spec::glutton::GluttonChainSpec::from_json_file(path)?),
				Runtime::Default => Box::new(
					chain_spec::rococo_parachain::RococoParachainChainSpec::from_json_file(path)?,
				),
			}
		},
	})
}

/// Extracts the normalized chain id and parachain id from the input chain id.
/// (H/T to Phala for the idea)
/// E.g. "penpal-kusama-2004" yields ("penpal-kusama", Some(2004))
fn extract_parachain_id(id: &str) -> (&str, &str, Option<ParaId>) {
	const KUSAMA_TEST_PARA_PREFIX: &str = "penpal-kusama-";
	const POLKADOT_TEST_PARA_PREFIX: &str = "penpal-polkadot-";

	const GLUTTON_PARA_DEV_PREFIX: &str = "glutton-kusama-dev-";
	const GLUTTON_PARA_LOCAL_PREFIX: &str = "glutton-kusama-local-";
	const GLUTTON_PARA_GENESIS_PREFIX: &str = "glutton-kusama-genesis-";

	let (norm_id, orig_id, para) = if let Some(suffix) = id.strip_prefix(KUSAMA_TEST_PARA_PREFIX) {
		let para_id: u32 = suffix.parse().expect("Invalid parachain-id suffix");
		(&id[..KUSAMA_TEST_PARA_PREFIX.len() - 1], id, Some(para_id))
	} else if let Some(suffix) = id.strip_prefix(POLKADOT_TEST_PARA_PREFIX) {
		let para_id: u32 = suffix.parse().expect("Invalid parachain-id suffix");
		(&id[..POLKADOT_TEST_PARA_PREFIX.len() - 1], id, Some(para_id))
	} else if let Some(suffix) = id.strip_prefix(GLUTTON_PARA_DEV_PREFIX) {
		let para_id: u32 = suffix.parse().expect("Invalid parachain-id suffix");
		(&id[..GLUTTON_PARA_DEV_PREFIX.len() - 1], id, Some(para_id))
	} else if let Some(suffix) = id.strip_prefix(GLUTTON_PARA_LOCAL_PREFIX) {
		let para_id: u32 = suffix.parse().expect("Invalid parachain-id suffix");
		(&id[..GLUTTON_PARA_LOCAL_PREFIX.len() - 1], id, Some(para_id))
	} else if let Some(suffix) = id.strip_prefix(GLUTTON_PARA_GENESIS_PREFIX) {
		let para_id: u32 = suffix.parse().expect("Invalid parachain-id suffix");
		(&id[..GLUTTON_PARA_GENESIS_PREFIX.len() - 1], id, Some(para_id))
	} else {
		(id, id, None)
	};

	(norm_id, orig_id, para.map(Into::into))
}

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Polkadot parachain".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"Polkadot parachain\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relaychain node.\n\n\
		{} [parachain-args] -- [relaychain-args]",
			Self::executable_name()
		)
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/paritytech/cumulus/issues/new".into()
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn ChainSpec>, String> {
		load_spec(id)
	}
}

impl SubstrateCli for RelayChainCli {
	fn impl_name() -> String {
		"Polkadot parachain".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"Polkadot parachain\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relay chain node.\n\n\
		{} [parachain-args] -- [relay_chain-args]",
			Self::executable_name()
		)
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/paritytech/cumulus/issues/new".into()
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn ChainSpec>, String> {
		polkadot_cli::Cli::from_iter([RelayChainCli::executable_name()].iter()).load_spec(id)
	}
}

/// Creates partial components for the runtimes that are supported by the benchmarks.
macro_rules! construct_benchmark_partials {
	($config:expr, |$partials:ident| $code:expr) => {
		match $config.chain_spec.runtime() {
			Runtime::AssetHubKusama => {
				let $partials = new_partial::<asset_hub_kusama_runtime::RuntimeApi, _>(
					&$config,
					crate::service::aura_build_import_queue::<_, AuraId>,
				)?;
				$code
			},
			Runtime::AssetHubWestend => {
				let $partials = new_partial::<asset_hub_westend_runtime::RuntimeApi, _>(
					&$config,
					crate::service::aura_build_import_queue::<_, AuraId>,
				)?;
				$code
			},
			Runtime::AssetHubPolkadot => {
				let $partials = new_partial::<asset_hub_polkadot_runtime::RuntimeApi, _>(
					&$config,
					crate::service::aura_build_import_queue::<_, AssetHubPolkadotAuraId>,
				)?;
				$code
			},
			Runtime::BridgeHub(bridge_hub_runtime_type) => match bridge_hub_runtime_type {
				chain_spec::bridge_hubs::BridgeHubRuntimeType::Polkadot |
				chain_spec::bridge_hubs::BridgeHubRuntimeType::PolkadotLocal |
				chain_spec::bridge_hubs::BridgeHubRuntimeType::PolkadotDevelopment => {
					let $partials = new_partial::<chain_spec::bridge_hubs::polkadot::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, AuraId>,
					)?;
					$code
				},
				chain_spec::bridge_hubs::BridgeHubRuntimeType::Kusama |
				chain_spec::bridge_hubs::BridgeHubRuntimeType::KusamaLocal |
				chain_spec::bridge_hubs::BridgeHubRuntimeType::KusamaDevelopment => {
					let $partials = new_partial::<chain_spec::bridge_hubs::kusama::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, AuraId>,
					)?;
					$code
				},
				chain_spec::bridge_hubs::BridgeHubRuntimeType::Westend => {
					let $partials = new_partial::<chain_spec::bridge_hubs::westend::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, AuraId>,
					)?;
					$code
				},
				chain_spec::bridge_hubs::BridgeHubRuntimeType::Rococo |
				chain_spec::bridge_hubs::BridgeHubRuntimeType::RococoLocal |
				chain_spec::bridge_hubs::BridgeHubRuntimeType::RococoDevelopment => {
					let $partials = new_partial::<chain_spec::bridge_hubs::rococo::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, AuraId>,
					)?;
					$code
				},
				chain_spec::bridge_hubs::BridgeHubRuntimeType::Wococo |
				chain_spec::bridge_hubs::BridgeHubRuntimeType::WococoLocal => {
					let $partials = new_partial::<chain_spec::bridge_hubs::wococo::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, AuraId>,
					)?;
					$code
				},
			},
			Runtime::CollectivesPolkadot | Runtime::CollectivesWestend => {
				let $partials = new_partial::<collectives_polkadot_runtime::RuntimeApi, _>(
					&$config,
					crate::service::aura_build_import_queue::<_, AuraId>,
				)?;
				$code
			},
			_ => Err("The chain is not supported".into()),
		}
	};
}

macro_rules! construct_async_run {
	(|$components:ident, $cli:ident, $cmd:ident, $config:ident| $( $code:tt )* ) => {{
		let runner = $cli.create_runner($cmd)?;
		match runner.config().chain_spec.runtime() {
			Runtime::AssetHubWestend => {
				runner.async_run(|$config| {
					let $components = new_partial::<asset_hub_westend_runtime::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, AuraId>,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::AssetHubKusama => {
				runner.async_run(|$config| {
					let $components = new_partial::<asset_hub_kusama_runtime::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, AuraId>,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::AssetHubPolkadot => {
				runner.async_run(|$config| {
					let $components = new_partial::<asset_hub_polkadot_runtime::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, AssetHubPolkadotAuraId>,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::CollectivesPolkadot | Runtime::CollectivesWestend => {
				runner.async_run(|$config| {
					let $components = new_partial::<collectives_polkadot_runtime::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, AuraId>,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::Shell => {
				runner.async_run(|$config| {
					let $components = new_partial::<shell_runtime::RuntimeApi, _>(
						&$config,
						crate::service::shell_build_import_queue,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::Seedling => {
				runner.async_run(|$config| {
					let $components = new_partial::<seedling_runtime::RuntimeApi, _>(
						&$config,
						crate::service::shell_build_import_queue,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::ContractsRococo => {
				runner.async_run(|$config| {
					let $components = new_partial::<contracts_rococo_runtime::RuntimeApi, _>(
						&$config,
						crate::service::contracts_rococo_build_import_queue,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::BridgeHub(bridge_hub_runtime_type) => {
				 match bridge_hub_runtime_type {
					chain_spec::bridge_hubs::BridgeHubRuntimeType::Polkadot |
					chain_spec::bridge_hubs::BridgeHubRuntimeType::PolkadotLocal |
					chain_spec::bridge_hubs::BridgeHubRuntimeType::PolkadotDevelopment => {
						runner.async_run(|$config| {
							let $components = new_partial::<chain_spec::bridge_hubs::polkadot::RuntimeApi, _>(
								&$config,
								crate::service::aura_build_import_queue::<_, AuraId>,
							)?;

							let task_manager = $components.task_manager;
							{ $( $code )* }.map(|v| (v, task_manager))
						})
					},
					chain_spec::bridge_hubs::BridgeHubRuntimeType::Kusama |
					chain_spec::bridge_hubs::BridgeHubRuntimeType::KusamaLocal |
					chain_spec::bridge_hubs::BridgeHubRuntimeType::KusamaDevelopment => {
						runner.async_run(|$config| {
							let $components = new_partial::<chain_spec::bridge_hubs::kusama::RuntimeApi, _>(
								&$config,
								crate::service::aura_build_import_queue::<_, AuraId>,
							)?;

							let task_manager = $components.task_manager;
							{ $( $code )* }.map(|v| (v, task_manager))
						})
					},
					chain_spec::bridge_hubs::BridgeHubRuntimeType::Westend => {
						runner.async_run(|$config| {
							let $components = new_partial::<chain_spec::bridge_hubs::westend::RuntimeApi, _>(
								&$config,
								crate::service::aura_build_import_queue::<_, AuraId>,
							)?;

							let task_manager = $components.task_manager;
							{ $( $code )* }.map(|v| (v, task_manager))
						})
					},
					chain_spec::bridge_hubs::BridgeHubRuntimeType::Rococo |
					chain_spec::bridge_hubs::BridgeHubRuntimeType::RococoLocal |
					chain_spec::bridge_hubs::BridgeHubRuntimeType::RococoDevelopment => {
						runner.async_run(|$config| {
							let $components = new_partial::<chain_spec::bridge_hubs::rococo::RuntimeApi, _>(
								&$config,
								crate::service::aura_build_import_queue::<_, AuraId>,
							)?;

							let task_manager = $components.task_manager;
							{ $( $code )* }.map(|v| (v, task_manager))
						})
					},
					chain_spec::bridge_hubs::BridgeHubRuntimeType::Wococo |
					chain_spec::bridge_hubs::BridgeHubRuntimeType::WococoLocal => {
						runner.async_run(|$config| {
							let $components = new_partial::<chain_spec::bridge_hubs::wococo::RuntimeApi, _>(
								&$config,
								crate::service::aura_build_import_queue::<_, AuraId>,
							)?;

							let task_manager = $components.task_manager;
							{ $( $code )* }.map(|v| (v, task_manager))
						})
					}
				}
			},
			Runtime::Penpal(_) | Runtime::Default => {
				runner.async_run(|$config| {
					let $components = new_partial::<
						rococo_parachain_runtime::RuntimeApi,
						_,
					>(
						&$config,
						crate::service::rococo_parachain_build_import_queue,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::Glutton => {
				runner.async_run(|$config| {
					let $components = new_partial::<glutton_runtime::RuntimeApi, _>(
						&$config,
						crate::service::shell_build_import_queue,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			}
		}
	}}
}

/// Parse command line arguments into service configuration.
pub fn run() -> Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::BuildSpec(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Some(Subcommand::CheckBlock(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, components.import_queue))
			})
		},
		Some(Subcommand::ExportBlocks(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, config.database))
			})
		},
		Some(Subcommand::ExportState(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, config.chain_spec))
			})
		},
		Some(Subcommand::ImportBlocks(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, components.import_queue))
			})
		},
		Some(Subcommand::Revert(cmd)) => construct_async_run!(|components, cli, cmd, config| {
			Ok(cmd.run(components.client, components.backend, None))
		}),
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;

			runner.sync_run(|config| {
				let polkadot_cli = RelayChainCli::new(
					&config,
					[RelayChainCli::executable_name()].iter().chain(cli.relaychain_args.iter()),
				);

				let polkadot_config = SubstrateCli::create_configuration(
					&polkadot_cli,
					&polkadot_cli,
					config.tokio_handle.clone(),
				)
				.map_err(|err| format!("Relay chain argument error: {}", err))?;

				cmd.run(config, polkadot_config)
			})
		},
		Some(Subcommand::ExportGenesisState(cmd)) =>
			construct_async_run!(|components, cli, cmd, config| {
				Ok(async move { cmd.run(&*config.chain_spec, &*components.client) })
			}),
		Some(Subcommand::ExportGenesisWasm(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|_config| {
				let spec = cli.load_spec(&cmd.shared_params.chain.clone().unwrap_or_default())?;
				cmd.run(&*spec)
			})
		},
		Some(Subcommand::Benchmark(cmd)) => {
			let runner = cli.create_runner(cmd)?;

			// Switch on the concrete benchmark sub-command-
			match cmd {
				BenchmarkCmd::Pallet(cmd) =>
					if cfg!(feature = "runtime-benchmarks") {
						runner.sync_run(|config| cmd.run::<Block, ()>(config))
					} else {
						Err("Benchmarking wasn't enabled when building the node. \
				You can enable it with `--features runtime-benchmarks`."
							.into())
					},
				BenchmarkCmd::Block(cmd) => runner.sync_run(|config| {
					construct_benchmark_partials!(config, |partials| cmd.run(partials.client))
				}),
				#[cfg(not(feature = "runtime-benchmarks"))]
				BenchmarkCmd::Storage(_) =>
					return Err(sc_cli::Error::Input(
						"Compile with --features=runtime-benchmarks \
						to enable storage benchmarks."
							.into(),
					)
					.into()),
				#[cfg(feature = "runtime-benchmarks")]
				BenchmarkCmd::Storage(cmd) => runner.sync_run(|config| {
					construct_benchmark_partials!(config, |partials| {
						let db = partials.backend.expose_db();
						let storage = partials.backend.expose_storage();

						cmd.run(config, partials.client.clone(), db, storage)
					})
				}),
				BenchmarkCmd::Machine(cmd) =>
					runner.sync_run(|config| cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone())),
				// NOTE: this allows the Client to leniently implement
				// new benchmark commands without requiring a companion MR.
				#[allow(unreachable_patterns)]
				_ => Err("Benchmarking sub-command unsupported".into()),
			}
		},
		Some(Subcommand::TryRuntime) => Err("The `try-runtime` subcommand has been migrated to a standalone CLI (https://github.com/paritytech/try-runtime-cli). It is no longer being maintained here and will be removed entirely some time after January 2024. Please remove this subcommand from your runtime and use the standalone CLI.".into()),
		Some(Subcommand::Key(cmd)) => Ok(cmd.run(&cli)?),
		None => {
			let runner = cli.create_runner(&cli.run.normalize())?;
			let collator_options = cli.run.collator_options();

			runner.run_node_until_exit(|config| async move {
				// If Statemint (Statemine, Westmint, Rockmine) DB exists and we're using the
				// asset-hub chain spec, then rename the base path to the new chain ID. In the case
				// that both file paths exist, the node will exit, as the user must decide (by
				// deleting one path) the information that they want to use as their DB.
				let old_name = match config.chain_spec.id() {
				     "asset-hub-polkadot" => Some("statemint"),
				     "asset-hub-kusama" => Some("statemine"),
				     "asset-hub-westend" => Some("westmint"),
				     "asset-hub-rococo" => Some("rockmine"),
				     _ => None,
				};

				if let Some(old_name) = old_name {
				    let new_path = config.base_path.config_dir(config.chain_spec.id());
				    let old_path = config.base_path.config_dir(old_name);

				    if old_path.exists() && new_path.exists() {
				         return Err(format!(
							"Found legacy {} path {} and new asset-hub path {}. Delete one path such that only one exists.",
							old_name, old_path.display(), new_path.display()
						).into())
				    }

				    if old_path.exists() {
				        std::fs::rename(old_path.clone(), new_path.clone())?;
						info!(
							"Statemint renamed to Asset Hub. The filepath with associated data on disk has been renamed from {} to {}.",
							old_path.display(), new_path.display()
						);
				    }
				}

				let hwbench = (!cli.no_hardware_benchmarks).then_some(
					config.database.path().map(|database_path| {
						let _ = std::fs::create_dir_all(database_path);
						sc_sysinfo::gather_hwbench(Some(database_path))
					})).flatten();

				let para_id = chain_spec::Extensions::try_get(&*config.chain_spec)
					.map(|e| e.para_id)
					.ok_or("Could not find parachain extension in chain-spec.")?;

				let polkadot_cli = RelayChainCli::new(
					&config,
					[RelayChainCli::executable_name()].iter().chain(cli.relaychain_args.iter()),
				);

				let id = ParaId::from(para_id);

				let parachain_account =
					AccountIdConversion::<polkadot_primitives::AccountId>::into_account_truncating(&id);

				let tokio_handle = config.tokio_handle.clone();
				let polkadot_config =
					SubstrateCli::create_configuration(&polkadot_cli, &polkadot_cli, tokio_handle)
						.map_err(|err| format!("Relay chain argument error: {}", err))?;

				info!("Parachain id: {:?}", id);
				info!("Parachain Account: {}", parachain_account);
				info!("Is collating: {}", if config.role.is_authority() { "yes" } else { "no" });

				match config.chain_spec.runtime() {
					Runtime::AssetHubPolkadot => crate::service::start_generic_aura_node::<
						asset_hub_polkadot_runtime::RuntimeApi,
						AssetHubPolkadotAuraId,
					>(config, polkadot_config, collator_options, id, hwbench)
					.await
					.map(|r| r.0)
					.map_err(Into::into),
					Runtime::AssetHubKusama => crate::service::start_generic_aura_node::<
						asset_hub_kusama_runtime::RuntimeApi,
						AuraId,
					>(config, polkadot_config, collator_options, id, hwbench)
					.await
					.map(|r| r.0)
					.map_err(Into::into),
					Runtime::AssetHubWestend => crate::service::start_generic_aura_node::<
						asset_hub_westend_runtime::RuntimeApi,
						AuraId,
					>(config, polkadot_config, collator_options, id, hwbench)
					.await
					.map(|r| r.0)
					.map_err(Into::into),
					Runtime::CollectivesPolkadot | Runtime::CollectivesWestend =>
						crate::service::start_generic_aura_node::<
							collectives_polkadot_runtime::RuntimeApi,
							AuraId,
						>(config, polkadot_config, collator_options, id, hwbench)
						.await
						.map(|r| r.0)
						.map_err(Into::into),
					Runtime::Shell =>
						crate::service::start_shell_node::<shell_runtime::RuntimeApi>(
							config,
							polkadot_config,
							collator_options,
							id,
							hwbench,
						)
						.await
						.map(|r| r.0)
						.map_err(Into::into),
					Runtime::Seedling => crate::service::start_shell_node::<
						seedling_runtime::RuntimeApi,
					>(config, polkadot_config, collator_options, id, hwbench)
					.await
					.map(|r| r.0)
					.map_err(Into::into),
					Runtime::ContractsRococo => crate::service::start_contracts_rococo_node(
						config,
						polkadot_config,
						collator_options,
						id,
						hwbench,
					)
					.await
					.map(|r| r.0)
					.map_err(Into::into),
					Runtime::BridgeHub(bridge_hub_runtime_type) => match bridge_hub_runtime_type {
						chain_spec::bridge_hubs::BridgeHubRuntimeType::Polkadot |
						chain_spec::bridge_hubs::BridgeHubRuntimeType::PolkadotLocal |
						chain_spec::bridge_hubs::BridgeHubRuntimeType::PolkadotDevelopment =>
							crate::service::start_generic_aura_node::<
								chain_spec::bridge_hubs::polkadot::RuntimeApi,
								AuraId,
							>(config, polkadot_config, collator_options, id, hwbench)
								.await
								.map(|r| r.0),
						chain_spec::bridge_hubs::BridgeHubRuntimeType::Kusama |
						chain_spec::bridge_hubs::BridgeHubRuntimeType::KusamaLocal |
						chain_spec::bridge_hubs::BridgeHubRuntimeType::KusamaDevelopment =>
							crate::service::start_generic_aura_node::<
								chain_spec::bridge_hubs::kusama::RuntimeApi,
								AuraId,
							>(config, polkadot_config, collator_options, id, hwbench)
							.await
							.map(|r| r.0),
						chain_spec::bridge_hubs::BridgeHubRuntimeType::Westend =>
							crate::service::start_generic_aura_node::<
								chain_spec::bridge_hubs::westend::RuntimeApi,
								AuraId,
							>(config, polkadot_config, collator_options, id, hwbench)
							.await
							.map(|r| r.0),
						chain_spec::bridge_hubs::BridgeHubRuntimeType::Rococo |
						chain_spec::bridge_hubs::BridgeHubRuntimeType::RococoLocal |
						chain_spec::bridge_hubs::BridgeHubRuntimeType::RococoDevelopment =>
							crate::service::start_generic_aura_node::<
								chain_spec::bridge_hubs::rococo::RuntimeApi,
								AuraId,
							>(config, polkadot_config, collator_options, id, hwbench)
							.await
							.map(|r| r.0),
						chain_spec::bridge_hubs::BridgeHubRuntimeType::Wococo |
						chain_spec::bridge_hubs::BridgeHubRuntimeType::WococoLocal =>
							crate::service::start_generic_aura_node::<
								chain_spec::bridge_hubs::wococo::RuntimeApi,
								AuraId,
							>(config, polkadot_config, collator_options, id, hwbench)
							.await
							.map(|r| r.0),
					}
					.map_err(Into::into),
					Runtime::Penpal(_) | Runtime::Default =>
						crate::service::start_rococo_parachain_node(
							config,
							polkadot_config,
							collator_options,
							id,
							hwbench,
						)
						.await
						.map(|r| r.0)
						.map_err(Into::into),
					Runtime::Glutton =>
						crate::service::start_shell_node::<glutton_runtime::RuntimeApi>(
							config,
							polkadot_config,
							collator_options,
							id,
							hwbench,
						)
						.await
						.map(|r| r.0)
						.map_err(Into::into),
				}
			})
		},
	}
}

impl DefaultConfigurationValues for RelayChainCli {
	fn p2p_listen_port() -> u16 {
		30334
	}

	fn rpc_listen_port() -> u16 {
		9945
	}

	fn prometheus_listen_port() -> u16 {
		9616
	}
}

impl CliConfiguration<Self> for RelayChainCli {
	fn shared_params(&self) -> &SharedParams {
		self.base.base.shared_params()
	}

	fn import_params(&self) -> Option<&ImportParams> {
		self.base.base.import_params()
	}

	fn network_params(&self) -> Option<&NetworkParams> {
		self.base.base.network_params()
	}

	fn keystore_params(&self) -> Option<&KeystoreParams> {
		self.base.base.keystore_params()
	}

	fn base_path(&self) -> Result<Option<BasePath>> {
		Ok(self
			.shared_params()
			.base_path()?
			.or_else(|| self.base_path.clone().map(Into::into)))
	}

	fn rpc_addr(&self, default_listen_port: u16) -> Result<Option<SocketAddr>> {
		self.base.base.rpc_addr(default_listen_port)
	}

	fn prometheus_config(
		&self,
		default_listen_port: u16,
		chain_spec: &Box<dyn ChainSpec>,
	) -> Result<Option<PrometheusConfig>> {
		self.base.base.prometheus_config(default_listen_port, chain_spec)
	}

	fn init<F>(
		&self,
		_support_url: &String,
		_impl_version: &String,
		_logger_hook: F,
		_config: &sc_service::Configuration,
	) -> Result<()>
	where
		F: FnOnce(&mut sc_cli::LoggerBuilder, &sc_service::Configuration),
	{
		unreachable!("PolkadotCli is never initialized; qed");
	}

	fn chain_id(&self, is_dev: bool) -> Result<String> {
		let chain_id = self.base.base.chain_id(is_dev)?;

		Ok(if chain_id.is_empty() { self.chain_id.clone().unwrap_or_default() } else { chain_id })
	}

	fn role(&self, is_dev: bool) -> Result<sc_service::Role> {
		self.base.base.role(is_dev)
	}

	fn transaction_pool(&self, is_dev: bool) -> Result<sc_service::config::TransactionPoolOptions> {
		self.base.base.transaction_pool(is_dev)
	}

	fn trie_cache_maximum_size(&self) -> Result<Option<usize>> {
		self.base.base.trie_cache_maximum_size()
	}

	fn rpc_methods(&self) -> Result<sc_service::config::RpcMethods> {
		self.base.base.rpc_methods()
	}

	fn rpc_max_connections(&self) -> Result<u32> {
		self.base.base.rpc_max_connections()
	}

	fn rpc_cors(&self, is_dev: bool) -> Result<Option<Vec<String>>> {
		self.base.base.rpc_cors(is_dev)
	}

	fn default_heap_pages(&self) -> Result<Option<u64>> {
		self.base.base.default_heap_pages()
	}

	fn force_authoring(&self) -> Result<bool> {
		self.base.base.force_authoring()
	}

	fn disable_grandpa(&self) -> Result<bool> {
		self.base.base.disable_grandpa()
	}

	fn max_runtime_instances(&self) -> Result<Option<usize>> {
		self.base.base.max_runtime_instances()
	}

	fn announce_block(&self) -> Result<bool> {
		self.base.base.announce_block()
	}

	fn telemetry_endpoints(
		&self,
		chain_spec: &Box<dyn ChainSpec>,
	) -> Result<Option<sc_telemetry::TelemetryEndpoints>> {
		self.base.base.telemetry_endpoints(chain_spec)
	}

	fn node_name(&self) -> Result<String> {
		self.base.base.node_name()
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		chain_spec::{get_account_id_from_seed, get_from_seed},
		command::{Runtime, RuntimeResolver},
	};
	use sc_chain_spec::{ChainSpec, ChainSpecExtension, ChainSpecGroup, ChainType, Extension};
	use serde::{Deserialize, Serialize};
	use sp_core::sr25519;
	use std::path::PathBuf;
	use tempfile::TempDir;

	#[derive(
		Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension, Default,
	)]
	#[serde(deny_unknown_fields)]
	pub struct Extensions1 {
		pub attribute1: String,
		pub attribute2: u32,
	}

	#[derive(
		Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension, Default,
	)]
	#[serde(deny_unknown_fields)]
	pub struct Extensions2 {
		pub attribute_x: String,
		pub attribute_y: String,
		pub attribute_z: u32,
	}

	fn store_configuration(dir: &TempDir, spec: Box<dyn ChainSpec>) -> PathBuf {
		let raw_output = true;
		let json = sc_service::chain_ops::build_spec(&*spec, raw_output)
			.expect("Failed to build json string");
		let mut cfg_file_path = dir.path().to_path_buf();
		cfg_file_path.push(spec.id());
		cfg_file_path.set_extension("json");
		std::fs::write(&cfg_file_path, json).expect("Failed to write to json file");
		cfg_file_path
	}

	pub type DummyChainSpec<E> =
		sc_service::GenericChainSpec<rococo_parachain_runtime::RuntimeGenesisConfig, E>;

	pub fn create_default_with_extensions<E: Extension>(
		id: &str,
		extension: E,
	) -> DummyChainSpec<E> {
		DummyChainSpec::from_genesis(
			"Dummy local testnet",
			id,
			ChainType::Local,
			move || {
				crate::chain_spec::rococo_parachain::testnet_genesis(
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					vec![
						get_from_seed::<rococo_parachain_runtime::AuraId>("Alice"),
						get_from_seed::<rococo_parachain_runtime::AuraId>("Bob"),
					],
					vec![get_account_id_from_seed::<sr25519::Public>("Alice")],
					1000.into(),
				)
			},
			Vec::new(),
			None,
			None,
			None,
			None,
			extension,
		)
	}

	#[test]
	fn test_resolve_runtime_for_different_configuration_files() {
		let temp_dir = tempfile::tempdir().expect("Failed to access tempdir");

		let path = store_configuration(
			&temp_dir,
			Box::new(create_default_with_extensions("shell-1", Extensions1::default())),
		);
		assert_eq!(Runtime::Shell, path.runtime());

		let path = store_configuration(
			&temp_dir,
			Box::new(create_default_with_extensions("shell-2", Extensions2::default())),
		);
		assert_eq!(Runtime::Shell, path.runtime());

		let path = store_configuration(
			&temp_dir,
			Box::new(create_default_with_extensions("seedling", Extensions2::default())),
		);
		assert_eq!(Runtime::Seedling, path.runtime());

		let path = store_configuration(
			&temp_dir,
			Box::new(crate::chain_spec::rococo_parachain::rococo_parachain_local_config()),
		);
		assert_eq!(Runtime::Default, path.runtime());

		let path = store_configuration(
			&temp_dir,
			Box::new(crate::chain_spec::asset_hubs::asset_hub_kusama_local_config()),
		);
		assert_eq!(Runtime::AssetHubKusama, path.runtime());

		let path = store_configuration(
			&temp_dir,
			Box::new(crate::chain_spec::contracts::contracts_rococo_local_config()),
		);
		assert_eq!(Runtime::ContractsRococo, path.runtime());
	}
}
