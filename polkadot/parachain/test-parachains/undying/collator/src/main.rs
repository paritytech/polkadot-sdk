// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Collator for the `Undying` test parachain.

use polkadot_cli::{Error, ProvideRuntimeApi, Result};
use polkadot_node_primitives::{CollationGenerationConfig, SubmitCollationParams};
use polkadot_node_subsystem::messages::{CollationGenerationMessage, CollatorProtocolMessage};
use polkadot_primitives::{
	vstaging::{ClaimQueueOffset, DEFAULT_CLAIM_QUEUE_OFFSET},
	CoreIndex, Id as ParaId, OccupiedCoreAssumption,
};
use polkadot_service::{Backend, HeaderBackend, ParachainHost};
use sc_cli::{Error as SubstrateCliError, SubstrateCli};
use sp_core::hexdisplay::HexDisplay;
use std::{
	fs,
	io::{self, Write},
	thread::sleep,
	time::Duration,
};
use test_parachain_undying_collator::{Collator, LOG_TARGET};

mod cli;
use cli::Cli;

fn main() -> Result<()> {
	let cli = Cli::from_args();

	match cli.subcommand {
		Some(cli::Subcommand::ExportGenesisState(params)) => {
			// `pov_size` and `pvf_complexity` need to match the ones that we start the collator
			// with.
			let collator = Collator::new(params.pov_size, params.pvf_complexity);

			let output_buf =
				format!("0x{:?}", HexDisplay::from(&collator.genesis_head())).into_bytes();

			if let Some(output) = params.output {
				std::fs::write(output, output_buf)?;
			} else {
				std::io::stdout().write_all(&output_buf)?;
			}

			Ok::<_, Error>(())
		},
		Some(cli::Subcommand::ExportGenesisWasm(params)) => {
			// We pass some dummy values for `pov_size` and `pvf_complexity` as these don't
			// matter for `wasm` export.
			let collator = Collator::default();
			let output_buf =
				format!("0x{:?}", HexDisplay::from(&collator.validation_code())).into_bytes();

			if let Some(output) = params.output {
				fs::write(output, output_buf)?;
			} else {
				io::stdout().write_all(&output_buf)?;
			}

			Ok(())
		},
		None => {
			let runner = cli.create_runner(&cli.run.base).map_err(|e| {
				SubstrateCliError::Application(
					Box::new(e) as Box<(dyn 'static + Send + Sync + std::error::Error)>
				)
			})?;

			runner.run_node_until_exit(|config| async move {
				let collator = Collator::new(cli.run.pov_size, cli.run.pvf_complexity);

				let full_node = polkadot_service::build_full(
					config,
					polkadot_service::NewFullParams {
						is_parachain_node: polkadot_service::IsParachainNode::Collator(
							collator.collator_key(),
						),
						enable_beefy: false,
						force_authoring_backoff: false,
						telemetry_worker_handle: None,

						// Collators don't spawn PVF workers, so we can disable version checks.
						node_version: None,
						secure_validator_mode: false,
						workers_path: None,
						workers_names: None,

						overseer_gen: polkadot_service::CollatorOverseerGen,
						overseer_message_channel_capacity_override: None,
						malus_finality_delay: None,
						hwbench: None,
						execute_workers_max_num: None,
						prepare_workers_hard_max_num: None,
						prepare_workers_soft_max_num: None,
						enable_approval_voting_parallel: false,
					},
				)
				.map_err(|e| e.to_string())?;
				let mut overseer_handle = full_node
					.overseer_handle
					.expect("Overseer handle should be initialized for collators");

				let genesis_head_hex =
					format!("0x{:?}", HexDisplay::from(&collator.genesis_head()));
				let validation_code_hex =
					format!("0x{:?}", HexDisplay::from(&collator.validation_code()));

				let para_id = ParaId::from(cli.run.parachain_id);

				log::info!("Running `Undying` collator for parachain id: {}", para_id);
				log::info!("Genesis state: {}", genesis_head_hex);
				log::info!("Validation code: {}", validation_code_hex);

				let config = CollationGenerationConfig {
					key: collator.collator_key(),
					// Set collation function to None if it is a malicious collator
					// and submit collations manually later.
					collator: if cli.run.malus {
						None
					} else {
						Some(
							collator
								.create_collation_function(full_node.task_manager.spawn_handle()),
						)
					},
					para_id,
				};
				overseer_handle
					.send_msg(CollationGenerationMessage::Initialize(config), "Collator")
					.await;

				overseer_handle
					.send_msg(CollatorProtocolMessage::CollateOn(para_id), "Collator")
					.await;

				// Check if it is a malicious collator.
				if cli.run.malus {
					let client = full_node.client.clone();
					let backend = full_node.backend.clone();

					let collation_function =
						collator.create_collation_function(full_node.task_manager.spawn_handle());

					full_node.task_manager.spawn_handle().spawn(
						"malus-undying-collator",
						None,
						async move {
							loop {
								let relay_parent = backend.blockchain().info().best_hash;

								// Get all assigned cores for the given parachain.
								let claim_queue =
									match client.runtime_api().claim_queue(relay_parent) {
										Ok(claim_queue) =>
											if claim_queue.is_empty() {
												log::info!(target: LOG_TARGET, "Claim queue is empty.");
												continue;
											} else {
												claim_queue
											},
										Err(error) => {
											log::error!(
												target: LOG_TARGET,
												"Failed to query claim queue runtime API: {error:?}"
											);
											continue;
										},
									};

								let claim_queue_offset =
									ClaimQueueOffset(DEFAULT_CLAIM_QUEUE_OFFSET);

								let scheduled_cores: Vec<CoreIndex> = claim_queue
									.iter()
									.filter_map(move |(core_index, paras)| {
										Some((
											*core_index,
											*paras.get(claim_queue_offset.0 as usize)?,
										))
									})
									.filter_map(|(core_index, core_para_id)| {
										(core_para_id == para_id).then_some(core_index)
									})
									.collect();

								if scheduled_cores.is_empty() {
									println!("Scheduled cores is empty.");
									continue;
								}

								// Get the collation.
								let validation_data =
									match client.runtime_api().persisted_validation_data(
										relay_parent,
										para_id,
										OccupiedCoreAssumption::Included,
									) {
										Ok(Some(validation_data)) => validation_data,
										Ok(None) => {
											log::warn!(
												target: LOG_TARGET,
												"Persisted validation data is None."
											);
											continue;
										},
										Err(error) => {
											log::error!(
												target: LOG_TARGET,
												"Failed to query persisted validation data runtime API: {error:?}"
											);
											continue;
										},
									};

								let collation = match collation_function(
									relay_parent,
									&validation_data,
								)
								.await
								{
									Some(collation) => collation,
									None => {
										log::warn!(
											target: LOG_TARGET,
											"Collation result is None."
										);
										continue;
									},
								}
								.collation;

								// Get validation code hash.
								let validation_code_hash =
									match client.runtime_api().validation_code_hash(
										relay_parent,
										para_id,
										OccupiedCoreAssumption::Included,
									) {
										Ok(Some(validation_code_hash)) => validation_code_hash,
										Ok(None) => {
											log::warn!(
												target: LOG_TARGET,
												"Validation code hash is None."
											);
											continue;
										},
										Err(error) => {
											log::error!(
												target: LOG_TARGET,
												"Failed to query validation code hash runtime API: {error:?}"
											);
											continue;
										},
									};

								// Submit the same collation for each assigned core.
								for core_index in &scheduled_cores {
									let submit_collation_params = SubmitCollationParams {
										relay_parent,
										collation: collation.clone(),
										parent_head: validation_data.parent_head.clone(),
										validation_code_hash,
										result_sender: None,
										core_index: *core_index,
									};

									overseer_handle
										.send_msg(
											CollationGenerationMessage::SubmitCollation(
												submit_collation_params,
											),
											"Collator",
										)
										.await;
								}

								// Wait before submitting the next collation.
								sleep(Duration::from_secs(6 as u64));
							}
						},
					);
				}

				Ok(full_node.task_manager)
			})
		},
	}?;
	Ok(())
}
