// This file is part of Substrate.

// Copyright (C) 2018-2020 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![warn(unused_extern_crates)]

//! Service implementation. Specialized wrapper over substrate service.

use std::sync::Arc;
use sc_consensus_babe;
use grandpa::{
	self, FinalityProofProvider as GrandpaFinalityProofProvider, StorageAndProofProvider,
};
use node_executor;
use node_primitives::Block;
use node_runtime::RuntimeApi;
use sc_service::{
	ServiceBuilder, config::{Role, Configuration}, error::{Error as ServiceError},
	RpcHandlers, ServiceComponents, TaskManager,
};
use sp_inherents::InherentDataProviders;
use sc_consensus::LongestChain;
use sc_network::{Event, NetworkService};
use sp_runtime::traits::Block as BlockT;
use futures::prelude::*;
use sc_client_api::ExecutorProvider;
use sp_core::traits::BareCryptoStorePtr;

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
macro_rules! new_full_start {
	($config:expr) => {{
		use std::sync::Arc;

		let mut import_setup = None;
		let mut rpc_setup = None;
		let inherent_data_providers = sp_inherents::InherentDataProviders::new();

		let builder = sc_service::ServiceBuilder::new_full::<
			node_primitives::Block, node_runtime::RuntimeApi, node_executor::Executor
		>($config)?
			.with_select_chain(|_config, backend| {
				Ok(sc_consensus::LongestChain::new(backend.clone()))
			})?
			.with_transaction_pool(|builder| {
				let pool_api = sc_transaction_pool::FullChainApi::new(
					builder.client().clone(),
				);
				let config = builder.config();

				Ok(sc_transaction_pool::BasicPool::new(
					config.transaction_pool.clone(),
					std::sync::Arc::new(pool_api),
					builder.prometheus_registry(),
				))
			})?
			.with_import_queue(|
				_config,
				client,
				mut select_chain,
				_transaction_pool,
				spawn_task_handle,
				prometheus_registry,
			| {
				let select_chain = select_chain.take()
					.ok_or_else(|| sc_service::Error::SelectChainRequired)?;
				let (grandpa_block_import, grandpa_link) = grandpa::block_import(
					client.clone(),
					&(client.clone() as Arc<_>),
					select_chain,
				)?;
				let justification_import = grandpa_block_import.clone();

				let (block_import, babe_link) = sc_consensus_babe::block_import(
					sc_consensus_babe::Config::get_or_compute(&*client)?,
					grandpa_block_import,
					client.clone(),
				)?;

				let import_queue = sc_consensus_babe::import_queue(
					babe_link.clone(),
					block_import.clone(),
					Some(Box::new(justification_import)),
					None,
					client,
					inherent_data_providers.clone(),
					spawn_task_handle,
					prometheus_registry,
				)?;

				import_setup = Some((block_import, grandpa_link, babe_link));
				Ok(import_queue)
			})?
			.with_rpc_extensions_builder(|builder| {
				let grandpa_link = import_setup.as_ref().map(|s| &s.1)
					.expect("GRANDPA LinkHalf is present for full services or set up failed; qed.");

				let shared_authority_set = grandpa_link.shared_authority_set().clone();
				let shared_voter_state = grandpa::SharedVoterState::empty();

				rpc_setup = Some((shared_voter_state.clone()));

				let babe_link = import_setup.as_ref().map(|s| &s.2)
					.expect("BabeLink is present for full services or set up failed; qed.");

				let babe_config = babe_link.config().clone();
				let shared_epoch_changes = babe_link.epoch_changes().clone();

				let client = builder.client().clone();
				let pool = builder.pool().clone();
				let select_chain = builder.select_chain().cloned()
					.expect("SelectChain is present for full services or set up failed; qed.");
				let keystore = builder.keystore().clone();

				Ok(move |deny_unsafe| {
					let deps = node_rpc::FullDeps {
						client: client.clone(),
						pool: pool.clone(),
						select_chain: select_chain.clone(),
						deny_unsafe,
						babe: node_rpc::BabeDeps {
							babe_config: babe_config.clone(),
							shared_epoch_changes: shared_epoch_changes.clone(),
							keystore: keystore.clone(),
						},
						grandpa: node_rpc::GrandpaDeps {
							shared_voter_state: shared_voter_state.clone(),
							shared_authority_set: shared_authority_set.clone(),
						},
					};

					node_rpc::create_full(deps)
				})
			})?;

		(builder, import_setup, inherent_data_providers, rpc_setup)
	}}
}

type FullClient = sc_service::TFullClient<Block, RuntimeApi, node_executor::Executor>;
type FullBackend = sc_service::TFullBackend<Block>;
type GrandpaBlockImport = grandpa::GrandpaBlockImport<
	FullBackend, Block, FullClient, sc_consensus::LongestChain<FullBackend, Block>
>;
type BabeBlockImport = sc_consensus_babe::BabeBlockImport<Block, FullClient, GrandpaBlockImport>;

/// Creates a full service from the configuration.
pub fn new_full_base(
	config: Configuration,
	with_startup_data: impl FnOnce(&BabeBlockImport, &sc_consensus_babe::BabeLink<Block>)
) -> Result<(
	TaskManager,
	InherentDataProviders,
	Arc<FullClient>, Arc<NetworkService<Block, <Block as BlockT>::Hash>>,
	Arc<sc_transaction_pool::BasicPool<sc_transaction_pool::FullChainApi<FullClient, Block>, Block>>
), ServiceError> {
	let (
		role,
		force_authoring,
		name,
		disable_grandpa,
	) = (
		config.role.clone(),
		config.force_authoring,
		config.network.node_name.clone(),
		config.disable_grandpa,
	);

	let (builder, mut import_setup, inherent_data_providers, mut rpc_setup) =
		new_full_start!(config);

	let ServiceComponents {
		client, transaction_pool, task_manager, keystore, network, select_chain,
		prometheus_registry, telemetry_on_connect_sinks, ..
	} = builder
		.with_finality_proof_provider(|client, backend| {
			// GenesisAuthoritySetProvider is implemented for StorageAndProofProvider
			let provider = client as Arc<dyn grandpa::StorageAndProofProvider<_, _>>;
			Ok(Arc::new(grandpa::FinalityProofProvider::new(backend, provider)) as _)
		})?
		.build_full()?;

	let (block_import, grandpa_link, babe_link) = import_setup.take()
		.expect("Link Half and Block Import are present for Full Services or setup failed before. qed");

	let shared_voter_state = rpc_setup.take()
		.expect("The SharedVoterState is present for Full Services or setup failed before. qed");

	(with_startup_data)(&block_import, &babe_link);

	if let sc_service::config::Role::Authority { .. } = &role {
		let proposer = sc_basic_authorship::ProposerFactory::new(
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
		);

		let select_chain = select_chain
			.ok_or(sc_service::Error::SelectChainRequired)?;

		let can_author_with =
			sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());

		let babe_config = sc_consensus_babe::BabeParams {
			keystore: keystore.clone(),
			client: client.clone(),
			select_chain,
			env: proposer,
			block_import,
			sync_oracle: network.clone(),
			inherent_data_providers: inherent_data_providers.clone(),
			force_authoring,
			babe_link,
			can_author_with,
		};

		let babe = sc_consensus_babe::start_babe(babe_config)?;
		task_manager.spawn_essential_handle().spawn_blocking("babe-proposer", babe);
	}

	// Spawn authority discovery module.
	if matches!(role, Role::Authority{..} | Role::Sentry {..}) {
		let (sentries, authority_discovery_role) = match role {
			sc_service::config::Role::Authority { ref sentry_nodes } => (
				sentry_nodes.clone(),
				sc_authority_discovery::Role::Authority (
					keystore.clone(),
				),
			),
			sc_service::config::Role::Sentry {..} => (
				vec![],
				sc_authority_discovery::Role::Sentry,
			),
			_ => unreachable!("Due to outer matches! constraint; qed.")
		};

		let dht_event_stream = network.event_stream("authority-discovery")
			.filter_map(|e| async move { match e {
				Event::Dht(e) => Some(e),
				_ => None,
			}}).boxed();
		let authority_discovery = sc_authority_discovery::AuthorityDiscovery::new(
			client.clone(),
			network.clone(),
			sentries,
			dht_event_stream,
			authority_discovery_role,
			prometheus_registry.clone(),
		);

		task_manager.spawn_handle().spawn("authority-discovery", authority_discovery);
	}

	// if the node isn't actively participating in consensus then it doesn't
	// need a keystore, regardless of which protocol we use below.
	let keystore = if role.is_authority() {
		Some(keystore.clone() as BareCryptoStorePtr)
	} else {
		None
	};

	let config = grandpa::Config {
		// FIXME #1578 make this available through chainspec
		gossip_duration: std::time::Duration::from_millis(333),
		justification_period: 512,
		name: Some(name),
		observer_enabled: false,
		keystore,
		is_authority: role.is_network_authority(),
	};

	let enable_grandpa = !disable_grandpa;
	if enable_grandpa {
		// start the full GRANDPA voter
		// NOTE: non-authorities could run the GRANDPA observer protocol, but at
		// this point the full voter should provide better guarantees of block
		// and vote data availability than the observer. The observer has not
		// been tested extensively yet and having most nodes in a network run it
		// could lead to finality stalls.
		let grandpa_config = grandpa::GrandpaParams {
			config,
			link: grandpa_link,
			network: network.clone(),
			inherent_data_providers: inherent_data_providers.clone(),
			telemetry_on_connect: Some(telemetry_on_connect_sinks.on_connect_stream()),
			voting_rule: grandpa::VotingRulesBuilder::default().build(),
			prometheus_registry: prometheus_registry.clone(),
			shared_voter_state,
		};

		// the GRANDPA voter task is considered infallible, i.e.
		// if it fails we take down the service with it.
		task_manager.spawn_essential_handle().spawn_blocking(
			"grandpa-voter",
			grandpa::run_grandpa_voter(grandpa_config)?
		);
	} else {
		grandpa::setup_disabled_grandpa(
			client.clone(),
			&inherent_data_providers,
			network.clone(),
		)?;
	}

	Ok((task_manager, inherent_data_providers, client, network, transaction_pool))
}

/// Builds a new service for a full client.
pub fn new_full(config: Configuration)
-> Result<TaskManager, ServiceError> {
	new_full_base(config, |_, _| ()).map(|(task_manager, _, _, _, _)| {
		task_manager
	})
}

type LightClient = sc_service::TLightClient<Block, RuntimeApi, node_executor::Executor>;
type LightFetcher = sc_network::config::OnDemand<Block>;

pub fn new_light_base(config: Configuration) -> Result<(
	TaskManager, Arc<RpcHandlers>, Arc<LightClient>,
	Arc<NetworkService<Block, <Block as BlockT>::Hash>>,
	Arc<sc_transaction_pool::BasicPool<
		sc_transaction_pool::LightChainApi<LightClient, LightFetcher, Block>, Block
	>>
), ServiceError> {
	let inherent_data_providers = InherentDataProviders::new();

	let ServiceComponents {
		task_manager, rpc_handlers, client, network, transaction_pool, ..
	} = ServiceBuilder::new_light::<Block, RuntimeApi, node_executor::Executor>(config)?
		.with_select_chain(|_config, backend| {
			Ok(LongestChain::new(backend.clone()))
		})?
		.with_transaction_pool(|builder| {
			let fetcher = builder.fetcher()
				.ok_or_else(|| "Trying to start light transaction pool without active fetcher")?;
			let pool_api = sc_transaction_pool::LightChainApi::new(
				builder.client().clone(),
				fetcher,
			);
			let pool = sc_transaction_pool::BasicPool::with_revalidation_type(
				builder.config().transaction_pool.clone(),
				Arc::new(pool_api),
				builder.prometheus_registry(),
				sc_transaction_pool::RevalidationType::Light,
			);
			Ok(pool)
		})?
		.with_import_queue_and_fprb(|
			_config,
			client,
			backend,
			fetcher,
			_select_chain,
			_tx_pool,
			spawn_task_handle,
			registry,
		| {
			let fetch_checker = fetcher
				.map(|fetcher| fetcher.checker().clone())
				.ok_or_else(|| "Trying to start light import queue without active fetch checker")?;
			let grandpa_block_import = grandpa::light_block_import(
				client.clone(),
				backend,
				&(client.clone() as Arc<_>),
				Arc::new(fetch_checker),
			)?;

			let finality_proof_import = grandpa_block_import.clone();
			let finality_proof_request_builder =
				finality_proof_import.create_finality_proof_request_builder();

			let (babe_block_import, babe_link) = sc_consensus_babe::block_import(
				sc_consensus_babe::Config::get_or_compute(&*client)?,
				grandpa_block_import,
				client.clone(),
			)?;

			let import_queue = sc_consensus_babe::import_queue(
				babe_link,
				babe_block_import,
				None,
				Some(Box::new(finality_proof_import)),
				client.clone(),
				inherent_data_providers.clone(),
				spawn_task_handle,
				registry,
			)?;

			Ok((import_queue, finality_proof_request_builder))
		})?
		.with_finality_proof_provider(|client, backend| {
			// GenesisAuthoritySetProvider is implemented for StorageAndProofProvider
			let provider = client as Arc<dyn StorageAndProofProvider<_, _>>;
			Ok(Arc::new(GrandpaFinalityProofProvider::new(backend, provider)) as _)
		})?
		.with_rpc_extensions(|builder| {
			let fetcher = builder.fetcher()
				.ok_or_else(|| "Trying to start node RPC without active fetcher")?;
			let remote_blockchain = builder.remote_backend()
				.ok_or_else(|| "Trying to start node RPC without active remote blockchain")?;

			let light_deps = node_rpc::LightDeps {
				remote_blockchain,
				fetcher,
				client: builder.client().clone(),
				pool: builder.pool(),
			};

			Ok(node_rpc::create_light(light_deps))
		})?
		.build_light()?;
	
	Ok((task_manager, rpc_handlers, client, network, transaction_pool))
}

/// Builds a new service for a light client.
pub fn new_light(config: Configuration) -> Result<TaskManager, ServiceError> {	
	new_light_base(config).map(|(task_manager, _, _, _, _)| {
		task_manager
	})
}

#[cfg(test)]
mod tests {
	use std::{sync::Arc, borrow::Cow, any::Any};
	use sc_consensus_babe::{CompatibleDigestItem, BabeIntermediate, INTERMEDIATE_KEY};
	use sc_consensus_epochs::descendent_query;
	use sp_consensus::{
		Environment, Proposer, BlockImportParams, BlockOrigin, ForkChoiceStrategy, BlockImport,
		RecordProof,
	};
	use node_primitives::{Block, DigestItem, Signature};
	use node_runtime::{BalancesCall, Call, UncheckedExtrinsic, Address};
	use node_runtime::constants::{currency::CENTS, time::SLOT_DURATION};
	use codec::Encode;
	use sp_core::{crypto::Pair as CryptoPair, H256};
	use sp_runtime::{
		generic::{BlockId, Era, Digest, SignedPayload},
		traits::{Block as BlockT, Header as HeaderT},
		traits::Verify,
	};
	use sp_timestamp;
	use sp_finality_tracker;
	use sp_keyring::AccountKeyring;
	use sc_service_test::TestNetNode;
	use crate::service::{new_full_base, new_light_base};
	use sp_runtime::traits::IdentifyAccount;
	use sp_transaction_pool::{MaintainedTransactionPool, ChainEvent};
	use sc_client_api::BlockBackend;

	type AccountPublic = <Signature as Verify>::Signer;

	#[test]
	// It is "ignored", but the node-cli ignored tests are running on the CI.
	// This can be run locally with `cargo test --release -p node-cli test_sync -- --ignored`.
	#[ignore]
	fn test_sync() {
		let keystore_path = tempfile::tempdir().expect("Creates keystore path");
		let keystore = sc_keystore::Store::open(keystore_path.path(), None)
			.expect("Creates keystore");
		let alice = keystore.write().insert_ephemeral_from_seed::<sc_consensus_babe::AuthorityPair>("//Alice")
			.expect("Creates authority pair");

		let chain_spec = crate::chain_spec::tests::integration_test_config_with_single_authority();

		// For the block factory
		let mut slot_num = 1u64;

		// For the extrinsics factory
		let bob = Arc::new(AccountKeyring::Bob.pair());
		let charlie = Arc::new(AccountKeyring::Charlie.pair());
		let mut index = 0;

		sc_service_test::sync(
			chain_spec,
			|config| {
				let mut setup_handles = None;
				let (keep_alive, inherent_data_providers, client, network, transaction_pool) =
					new_full_base(config,
						|
							block_import: &sc_consensus_babe::BabeBlockImport<Block, _, _>,
							babe_link: &sc_consensus_babe::BabeLink<Block>,
						| {
							setup_handles = Some((block_import.clone(), babe_link.clone()));
						}
					)?;
				
				let node = sc_service_test::TestNetComponents::new(
					keep_alive, client, network, transaction_pool
				);
				Ok((node, (inherent_data_providers, setup_handles.unwrap())))
			},
			|config| {
				let (keep_alive, _, client, network, transaction_pool) = new_light_base(config)?;
				Ok(sc_service_test::TestNetComponents::new(keep_alive, client, network, transaction_pool))
			},
			|service, &mut (ref inherent_data_providers, (ref mut block_import, ref babe_link))| {
				let mut inherent_data = inherent_data_providers
					.create_inherent_data()
					.expect("Creates inherent data.");
				inherent_data.replace_data(sp_finality_tracker::INHERENT_IDENTIFIER, &1u64);

				let parent_id = BlockId::number(service.client().chain_info().best_number);
				let parent_header = service.client().header(&parent_id).unwrap().unwrap();
				let parent_hash = parent_header.hash();
				let parent_number = *parent_header.number();

				futures::executor::block_on(
					service.transaction_pool().maintain(
						ChainEvent::NewBlock {
							is_new_best: true,
							hash: parent_header.hash(),
							tree_route: None,
							header: parent_header.clone(),
						},
					)
				);

				let mut proposer_factory = sc_basic_authorship::ProposerFactory::new(
					service.client(),
					service.transaction_pool(),
					None,
				);

				let epoch_descriptor = babe_link.epoch_changes().lock().epoch_descriptor_for_child_of(
					descendent_query(&*service.client()),
					&parent_hash,
					parent_number,
					slot_num,
				).unwrap().unwrap();

				let mut digest = Digest::<H256>::default();

				// even though there's only one authority some slots might be empty,
				// so we must keep trying the next slots until we can claim one.
				let babe_pre_digest = loop {
					inherent_data.replace_data(sp_timestamp::INHERENT_IDENTIFIER, &(slot_num * SLOT_DURATION));
					if let Some(babe_pre_digest) = sc_consensus_babe::test_helpers::claim_slot(
						slot_num,
						&parent_header,
						&*service.client(),
						&keystore,
						&babe_link,
					) {
						break babe_pre_digest;
					}

					slot_num += 1;
				};

				digest.push(<DigestItem as CompatibleDigestItem>::babe_pre_digest(babe_pre_digest));

				let new_block = futures::executor::block_on(async move {
					let proposer = proposer_factory.init(&parent_header).await;
					proposer.unwrap().propose(
						inherent_data,
						digest,
						std::time::Duration::from_secs(1),
						RecordProof::Yes,
					).await
				}).expect("Error making test block").block;

				let (new_header, new_body) = new_block.deconstruct();
				let pre_hash = new_header.hash();
				// sign the pre-sealed hash of the block and then
				// add it to a digest item.
				let to_sign = pre_hash.encode();
				let signature = alice.sign(&to_sign[..]);
				let item = <DigestItem as CompatibleDigestItem>::babe_seal(
					signature.into(),
				);
				slot_num += 1;

				let mut params = BlockImportParams::new(BlockOrigin::File, new_header);
				params.post_digests.push(item);
				params.body = Some(new_body);
				params.intermediates.insert(
					Cow::from(INTERMEDIATE_KEY),
					Box::new(BabeIntermediate::<Block> { epoch_descriptor }) as Box<dyn Any>,
				);
				params.fork_choice = Some(ForkChoiceStrategy::LongestChain);

				block_import.import_block(params, Default::default())
					.expect("error importing test block");
			},
			|service, _| {
				let amount = 5 * CENTS;
				let to: Address = AccountPublic::from(bob.public()).into_account().into();
				let from: Address = AccountPublic::from(charlie.public()).into_account().into();
				let genesis_hash = service.client().block_hash(0).unwrap().unwrap();
				let best_block_id = BlockId::number(service.client().chain_info().best_number);
				let (spec_version, transaction_version) = {
					let version = service.client().runtime_version_at(&best_block_id).unwrap();
					(version.spec_version, version.transaction_version)
				};
				let signer = charlie.clone();

				let function = Call::Balances(BalancesCall::transfer(to.into(), amount));

				let check_spec_version = frame_system::CheckSpecVersion::new();
				let check_tx_version = frame_system::CheckTxVersion::new();
				let check_genesis = frame_system::CheckGenesis::new();
				let check_era = frame_system::CheckEra::from(Era::Immortal);
				let check_nonce = frame_system::CheckNonce::from(index);
				let check_weight = frame_system::CheckWeight::new();
				let payment = pallet_transaction_payment::ChargeTransactionPayment::from(0);
				let validate_grandpa_equivocation = pallet_grandpa::ValidateEquivocationReport::new();
				let extra = (
					check_spec_version,
					check_tx_version,
					check_genesis,
					check_era,
					check_nonce,
					check_weight,
					payment,
					validate_grandpa_equivocation,
				);
				let raw_payload = SignedPayload::from_raw(
					function,
					extra,
					(spec_version, transaction_version, genesis_hash, genesis_hash, (), (), (), ())
				);
				let signature = raw_payload.using_encoded(|payload|	{
					signer.sign(payload)
				});
				let (function, extra, _) = raw_payload.deconstruct();
				index += 1;
				UncheckedExtrinsic::new_signed(
					function,
					from.into(),
					signature.into(),
					extra,
				).into()
			},
		);
	}

	#[test]
	#[ignore]
	fn test_consensus() {
		sc_service_test::consensus(
			crate::chain_spec::tests::integration_test_config_with_two_authorities(),
			|config| {
				let (keep_alive, _, client, network, transaction_pool) = new_full_base(config, |_, _| ())?;
				Ok(sc_service_test::TestNetComponents::new(keep_alive, client, network, transaction_pool))
			},
			|config| {
				let (keep_alive, _, client, network, transaction_pool) = new_light_base(config)?;
				Ok(sc_service_test::TestNetComponents::new(keep_alive, client, network, transaction_pool))
			},
			vec![
				"//Alice".into(),
				"//Bob".into(),
			],
		)
	}
}
