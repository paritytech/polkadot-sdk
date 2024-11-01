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
	cli::{Cli, RelayChainCli, Subcommand},
	common::{
		chain_spec::{Extensions, LoadSpec},
		runtime::{
			AuraConsensusId, Consensus, Runtime, RuntimeResolver as RuntimeResolverT,
			RuntimeResolver,
		},
		types::Block,
		NodeBlock, NodeExtraArgs,
	},
	fake_runtime_api,
	nodes::DynNodeSpecExt,
	runtime::BlockNumber,
};
#[cfg(feature = "runtime-benchmarks")]
use cumulus_client_service::storage_proof_size::HostFunctions as ReclaimHostFunctions;
use cumulus_primitives_core::ParaId;
use frame_benchmarking_cli::{BenchmarkCmd, SUBSTRATE_REFERENCE_HARDWARE};
use log::info;
use sc_cli::{Result, SubstrateCli};
use sp_runtime::traits::AccountIdConversion;
#[cfg(feature = "runtime-benchmarks")]
use sp_runtime::traits::HashingFor;

/// Structure that can be used in order to provide customizers for different functionalities of the
/// node binary that is being built using this library.
pub struct RunConfig {
	/// A custom chain spec loader.
	pub chain_spec_loader: Box<dyn LoadSpec>,
	/// A custom runtime resolver.
	pub runtime_resolver: Box<dyn RuntimeResolver>,
}

pub fn new_aura_node_spec<Block>(
	aura_id: AuraConsensusId,
	extra_args: &NodeExtraArgs,
) -> Box<dyn DynNodeSpecExt>
where
	Block: NodeBlock,
{
	match aura_id {
		AuraConsensusId::Sr25519 => crate::nodes::aura::new_aura_node_spec::<
			Block,
			fake_runtime_api::aura_sr25519::RuntimeApi,
			sp_consensus_aura::sr25519::AuthorityId,
		>(extra_args),
		AuraConsensusId::Ed25519 => crate::nodes::aura::new_aura_node_spec::<
			Block,
			fake_runtime_api::aura_ed25519::RuntimeApi,
			sp_consensus_aura::ed25519::AuthorityId,
		>(extra_args),
	}
}

fn new_node_spec(
	config: &sc_service::Configuration,
	runtime_resolver: &Box<dyn RuntimeResolverT>,
	extra_args: &NodeExtraArgs,
) -> std::result::Result<Box<dyn DynNodeSpecExt>, sc_cli::Error> {
	let runtime = runtime_resolver.runtime(config.chain_spec.as_ref())?;

	Ok(match runtime {
		Runtime::Omni(block_number, consensus) => match (block_number, consensus) {
			(BlockNumber::U32, Consensus::Aura(aura_id)) =>
				new_aura_node_spec::<Block<u32>>(aura_id, extra_args),
			(BlockNumber::U64, Consensus::Aura(aura_id)) =>
				new_aura_node_spec::<Block<u64>>(aura_id, extra_args),
		},
	})
}

/// Parse command line arguments into service configuration.
pub fn run<CliConfig: crate::cli::CliConfig>(cmd_config: RunConfig) -> Result<()> {
	let mut cli = Cli::<CliConfig>::from_args();
	cli.chain_spec_loader = Some(cmd_config.chain_spec_loader);

	match &cli.subcommand {
		Some(Subcommand::BuildSpec(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Some(Subcommand::CheckBlock(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let node =
					new_node_spec(&config, &cmd_config.runtime_resolver, &cli.node_extra_args())?;
				node.prepare_check_block_cmd(config, cmd)
			})
		},
		Some(Subcommand::ExportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let node =
					new_node_spec(&config, &cmd_config.runtime_resolver, &cli.node_extra_args())?;
				node.prepare_export_blocks_cmd(config, cmd)
			})
		},
		Some(Subcommand::ExportState(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let node =
					new_node_spec(&config, &cmd_config.runtime_resolver, &cli.node_extra_args())?;
				node.prepare_export_state_cmd(config, cmd)
			})
		},
		Some(Subcommand::ImportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let node =
					new_node_spec(&config, &cmd_config.runtime_resolver, &cli.node_extra_args())?;
				node.prepare_import_blocks_cmd(config, cmd)
			})
		},
		Some(Subcommand::Revert(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let node =
					new_node_spec(&config, &cmd_config.runtime_resolver, &cli.node_extra_args())?;
				node.prepare_revert_cmd(config, cmd)
			})
		},
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			let polkadot_cli =
				RelayChainCli::<CliConfig>::new(runner.config(), cli.relay_chain_args.iter());

			runner.sync_run(|config| {
				let polkadot_config = SubstrateCli::create_configuration(
					&polkadot_cli,
					&polkadot_cli,
					config.tokio_handle.clone(),
				)
				.map_err(|err| format!("Relay chain argument error: {}", err))?;

				cmd.run(config, polkadot_config)
			})
		},
		Some(Subcommand::ExportGenesisHead(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| {
				let node =
					new_node_spec(&config, &cmd_config.runtime_resolver, &cli.node_extra_args())?;
				node.run_export_genesis_head_cmd(config, cmd)
			})
		},
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
				#[cfg(feature = "runtime-benchmarks")]
				BenchmarkCmd::Pallet(cmd) => runner.sync_run(|config| {
					cmd.run_with_spec::<HashingFor<Block<u32>>, ReclaimHostFunctions>(Some(
						config.chain_spec,
					))
				}),
				BenchmarkCmd::Block(cmd) => runner.sync_run(|config| {
					let node = new_node_spec(
						&config,
						&cmd_config.runtime_resolver,
						&cli.node_extra_args(),
					)?;
					node.run_benchmark_block_cmd(config, cmd)
				}),
				#[cfg(feature = "runtime-benchmarks")]
				BenchmarkCmd::Storage(cmd) => runner.sync_run(|config| {
					let node = new_node_spec(
						&config,
						&cmd_config.runtime_resolver,
						&cli.node_extra_args(),
					)?;
					node.run_benchmark_storage_cmd(config, cmd)
				}),
				BenchmarkCmd::Machine(cmd) =>
					runner.sync_run(|config| cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone())),
				#[allow(unreachable_patterns)]
				_ => Err("Benchmarking sub-command unsupported or compilation feature missing. \
					Make sure to compile with --features=runtime-benchmarks \
					to enable all supported benchmarks."
					.into()),
			}
		},
		Some(Subcommand::Key(cmd)) => Ok(cmd.run(&cli)?),
		None => {
			let runner = cli.create_runner(&cli.run.normalize())?;
			let polkadot_cli =
				RelayChainCli::<CliConfig>::new(runner.config(), cli.relay_chain_args.iter());
			let collator_options = cli.run.collator_options();

			runner.run_node_until_exit(|config| async move {
				let node_spec =
					new_node_spec(&config, &cmd_config.runtime_resolver, &cli.node_extra_args())?;
				let para_id = ParaId::from(
					Extensions::try_get(&*config.chain_spec)
						.map(|e| e.para_id)
						.ok_or("Could not find parachain extension in chain-spec.")?,
				);

				if let Some(dev_block_time) = cli.dev_block_time {
					return node_spec
						.start_manual_seal_node(config, para_id, dev_block_time)
						.map_err(Into::into)
				}

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
							"Found legacy {} path {} and new Asset Hub path {}. \
							Delete one path such that only one exists.",
							old_name,
							old_path.display(),
							new_path.display()
						)
						.into());
					}

					if old_path.exists() {
						std::fs::rename(old_path.clone(), new_path.clone())?;
						info!(
							"{} was renamed to Asset Hub. The filepath with associated data on disk \
							has been renamed from {} to {}.",
							old_name,
							old_path.display(),
							new_path.display()
						);
					}
				}

				let hwbench = (!cli.no_hardware_benchmarks)
					.then(|| {
						config.database.path().map(|database_path| {
							let _ = std::fs::create_dir_all(database_path);
							sc_sysinfo::gather_hwbench(
								Some(database_path),
								&SUBSTRATE_REFERENCE_HARDWARE,
							)
						})
					})
					.flatten();

				let parachain_account =
					AccountIdConversion::<polkadot_primitives::AccountId>::into_account_truncating(
						&para_id,
					);

				let tokio_handle = config.tokio_handle.clone();
				let polkadot_config =
					SubstrateCli::create_configuration(&polkadot_cli, &polkadot_cli, tokio_handle)
						.map_err(|err| format!("Relay chain argument error: {}", err))?;

				info!("🪪 Parachain id: {:?}", para_id);
				info!("🧾 Parachain Account: {}", parachain_account);
				info!("✍️ Is collating: {}", if config.role.is_authority() { "yes" } else { "no" });

				node_spec
					.start_node(
						config,
						polkadot_config,
						collator_options,
						para_id,
						hwbench,
						cli.node_extra_args(),
					)
					.await
					.map_err(Into::into)
			})
		},
	}
}
