// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The dev (manual/instant-seal) node on a JAM runtime blob.
//!
//! A JAM block carries no parachain inherent; its context is the `JamParent` digest, the JAM state
//! it reads and the timestamp inherent. A dev node has no JAM chain behind it, so it stands in for
//! all three with a fixed, self-consistent JAM clock: block `n` is anchored at JAM slot
//! `first_slot + n`, carries that slot's timestamp ([`timestamp_for_anchor`]) and reads a
//! `ParaInfo` that names its parent as included.

use super::{
	jam_slot_at,
	service::{rpc_extensions_builder, spawn_storage_monitor, JamNode},
	LOG_TARGET,
};
use crate::{
	cli::{AuthoringPolicy, DevSealMode},
	common::{
		aura::{AuraIdT, AuraRuntimeApi},
		spec::BaseNodeSpec,
		types::{AccountId, Balance, JamClient, Nonce},
		ConstructNodeRuntimeApi, NodeBlock, NodeExtraArgs,
	},
};
use codec::Encode;
use cumulus_jam_aura_primitives::timestamp_for_anchor;
use cumulus_jam_state_reader::{JamStateExt, JamStateReader, StateKey};
use cumulus_primitives_core::{BlockBundleInfo, CumulusDigestItem, ParaId};
use futures::{future, FutureExt};
use parachain_service_core::{para_info_key, service_value_state_key, ParaInfo};
use sc_client_api::Backend;
use sc_consensus::{BlockImportParams, LongestChain};
use sc_consensus_manual_seal::{
	consensus::{aura::AuraConsensusDataProvider, ConsensusDataProvider},
	Error,
};
use sc_network::{config::FullNetworkConfiguration, NotificationMetrics};
use sc_service::{Configuration, PartialComponents, TaskManager};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sp_api::StorageProof;
use sp_consensus::ProposeArgs;
use sp_inherents::InherentData;
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT, UniqueSaturatedInto},
	Digest,
};
use sp_timestamp::{InherentDataProvider as TimestampProvider, Timestamp, TimestampInherentData};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

/// The JAM slot the dev chain's block 0 is anchored at: two hours before now, so the 6 s anchor
/// steps of a chain sealing faster than that stay behind the wall clock for a long run.
pub(crate) fn first_slot() -> u32 {
	let now = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.expect("Current time is always after UNIX_EPOCH; qed")
		.saturating_sub(Duration::from_secs(2 * 60 * 60));
	jam_slot_at(Timestamp::new(now.as_millis() as u64))
}

/// The timestamp of a dev block built on `parent`: its anchor slot's.
pub(crate) fn timestamp_for_block<Header: HeaderT>(first_slot: u32, parent: &Header) -> Timestamp {
	let number = UniqueSaturatedInto::<u32>::unique_saturated_into(*parent.number()) + 1;
	Timestamp::new(timestamp_for_anchor(first_slot.saturating_add(number)))
}

/// The inherent data providers of a dev block: the timestamp, nothing else.
pub(crate) fn inherent_data_providers<Header: HeaderT>(
	first_slot: u32,
	parent: &Header,
) -> TimestampProvider {
	TimestampProvider::new(timestamp_for_block(first_slot, parent))
}

/// Appends the JAM digests to `inner`'s (the Aura pre-digest): a `JamParent` anchored at the slot
/// of the block's timestamp, and a `BlockBundleInfo` for a block that has its core to itself.
pub(crate) struct JamDevDigests<P> {
	inner: P,
}

impl<P> JamDevDigests<P> {
	pub(crate) fn new(inner: P) -> Self {
		Self { inner }
	}
}

impl<B: BlockT, P: ConsensusDataProvider<B>> ConsensusDataProvider<B> for JamDevDigests<P> {
	fn create_digest(&self, parent: &B::Header, inherents: &InherentData) -> Result<Digest, Error> {
		let mut digest = self.inner.create_digest(parent, inherents)?;
		let timestamp = inherents
			.timestamp_inherent_data()?
			.ok_or_else(|| Error::StringError("a dev block has a timestamp inherent".into()))?;
		let anchor_slot = jam_slot_at(timestamp);
		digest.push(
			CumulusDigestItem::JamParent {
				anchor: Default::default(),
				anchor_slot,
				lookup_anchor: Default::default(),
				lookup_anchor_slot: anchor_slot,
			}
			.to_digest_item(),
		);
		digest.push(BlockBundleInfo { index: 0, is_last: true }.to_digest_item());
		Ok(digest)
	}

	fn append_block_import(
		&self,
		parent: &B::Header,
		params: &mut BlockImportParams<B>,
		inherents: &InherentData,
		proof: StorageProof,
	) -> Result<(), Error> {
		self.inner.append_block_import(parent, params, inherents, proof)
	}
}

/// The JAM state a dev block reads: the para's `ParaInfo`, naming the parent as included, and
/// nothing else.
///
/// With the parent included the runtime's unincluded segment never grows, as on a chain whose
/// every block is included right away.
pub(crate) struct DevJamState {
	para_info_key: StateKey,
	para_info: Vec<u8>,
}

impl DevJamState {
	pub(crate) fn new(para_id: ParaId, parent_hash: &[u8]) -> Self {
		let para_id = parachain_service_core::types::ParaId::from(u32::from(para_id));
		let head_data = parachain_service_core::types::HeadData::try_from(parent_hash.to_vec())
			.expect("a block hash is shorter than the head data bound; qed");
		let para_info = ParaInfo {
			head_data,
			validation_code: None,
			announced_upgrade: None,
			total_state_balance: 0,
			used_state_balance: 0,
			is_deregistering: false,
		};
		Self {
			para_info_key: service_value_state_key(
				parachain_service_core::PARACHAIN_SERVICE_ID,
				&para_info_key(para_id),
			),
			para_info: para_info.encode(),
		}
	}
}

impl JamStateReader for DevJamState {
	fn read(&self, key: &StateKey) -> Option<Vec<u8>> {
		(*key == self.para_info_key).then(|| self.para_info.clone())
	}

	fn proof_size(&self) -> usize {
		0
	}
}

/// Wraps a block-authoring [`sp_consensus::Environment`] so every proposal runs with a
/// [`DevJamState`] for its parent registered as the [`JamStateExt`].
pub(crate) struct DevJamEnv<E> {
	inner: E,
	para_id: ParaId,
}

impl<E> DevJamEnv<E> {
	pub(crate) fn new(inner: E, para_id: ParaId) -> Self {
		Self { inner, para_id }
	}
}

impl<B, E> sp_consensus::Environment<B> for DevJamEnv<E>
where
	B: BlockT,
	E: sp_consensus::Environment<B>,
{
	type CreateProposer = Pin<Box<dyn Future<Output = Result<Self::Proposer, Self::Error>> + Send>>;
	type Proposer = DevJamProposer<E::Proposer>;
	type Error = E::Error;

	fn init(&mut self, parent_header: &B::Header) -> Self::CreateProposer {
		let init = self.inner.init(parent_header);
		let state = DevJamState::new(self.para_id, parent_header.hash().as_ref());
		Box::pin(async move { Ok(DevJamProposer { inner: init.await?, state }) })
	}
}

/// The proposer [`DevJamEnv`] yields.
pub(crate) struct DevJamProposer<P> {
	inner: P,
	state: DevJamState,
}

impl<B, P> sp_consensus::Proposer<B> for DevJamProposer<P>
where
	B: BlockT,
	P: sp_consensus::Proposer<B>,
{
	type Error = P::Error;
	type Proposal = P::Proposal;

	fn propose(self, mut args: ProposeArgs<B>) -> Self::Proposal {
		tracing::trace!(target: LOG_TARGET, "Proposing a dev block on the dev JAM state.");
		args.extra_extensions.register(JamStateExt(Box::new(self.state)));
		self.inner.propose(args)
	}
}

/// Start the dev (manual/instant-seal) node on a JAM runtime blob.
pub(crate) fn start_dev_node<Block, RuntimeApi, AuraId>(
	mut config: Configuration,
	mode: DevSealMode,
	node_extra_args: NodeExtraArgs,
) -> sc_service::error::Result<TaskManager>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, JamClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	AuraId: AuraIdT + Sync,
{
	// Destructure all fields so the compiler enforces handling new args.
	let NodeExtraArgs {
		authoring_policy,
		ref export_pov,
		max_pov_percentage,
		ref statement_store_config,
		ref storage_monitor,
		ref hop,
		collator_reserved_slots: _,
	} = node_extra_args;

	// Warn about args that have no effect in dev mode or are not supported in JAM mode.
	if authoring_policy != AuthoringPolicy::Lookahead {
		log::warn!(
			"Authoring policy `{}` has no effect in dev mode (manual/instant seal is used).",
			authoring_policy,
		);
	}
	if export_pov.is_some() {
		log::warn!("`--export-pov` has no effect in dev mode (no PoVs are produced).");
	}
	if max_pov_percentage.is_some() {
		log::warn!("`--max-pov-percentage` has no effect in dev mode (no PoVs are produced).");
	}
	if statement_store_config.is_some() {
		log::warn!("The statement store is not supported in JAM mode yet.");
	}
	if hop.is_some() {
		log::warn!("HOP is not supported in JAM mode yet.");
	}

	let PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		select_chain: _,
		transaction_pool,
		other: (_, mut telemetry, _, _, _),
	} = JamNode::<Block, RuntimeApi, AuraId>::new_partial(&config)?;

	// Since this is a dev node, prevent it from connecting to peers.
	config.network.default_peers_set.in_peers = 0;
	config.network.default_peers_set.out_peers = 0;
	let net_config = FullNetworkConfiguration::<_, _, sc_network::Litep2pNetworkBackend>::new(
		&config.network,
		None,
	);

	let (network, system_rpc_tx, tx_handler_controller, sync_service, _bitswap_handle) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			spawn_essential_handle: task_manager.spawn_essential_handle(),
			import_queue,
			net_config,
			block_announce_validator_builder: None,
			warp_sync_config: None,
			block_relay: None,
			metrics: NotificationMetrics::new(None),
		})?;

	if config.offchain_worker.enabled {
		let offchain_workers =
			sc_offchain::OffchainWorkers::new(sc_offchain::OffchainWorkerOptions {
				runtime_api_provider: client.clone(),
				keystore: Some(keystore_container.keystore()),
				offchain_db: backend.offchain_storage(),
				transaction_pool: Some(OffchainTransactionPoolFactory::new(
					transaction_pool.clone(),
				)),
				network_provider: Arc::new(network.clone()),
				is_validator: config.role.is_authority(),
				enable_http_requests: true,
				custom_extensions: |_| vec![],
			})?;
		task_manager.spawn_handle().spawn(
			"offchain-workers-runner",
			"offchain-work",
			offchain_workers.run(client.clone(), task_manager.spawn_handle()).boxed(),
		);
	}

	let proposer = sc_basic_authorship::ProposerFactory::new(
		task_manager.spawn_handle(),
		client.clone(),
		transaction_pool.clone(),
		None,
		None,
	);

	// Note: Changing slot durations are currently not supported
	let slot_duration =
		sc_consensus_aura::slot_duration(&*client).expect("slot_duration is always present; qed.");
	let para_id = JamNode::<Block, RuntimeApi, AuraId>::parachain_id(&client, &config)
		.ok_or("Failed to retrieve the parachain id")?;

	// No parachain inherent: the JAM context comes from the digests and the JAM state reads
	// `DevJamEnv` stands in for.
	let env = DevJamEnv::new(proposer, para_id);
	let consensus_data_provider = Box::new(JamDevDigests::new(
		AuraConsensusDataProvider::<Block>::new_with_slot_duration(slot_duration),
	));
	let first_slot = first_slot();
	let header_client = client.clone();
	let create_inherent_data_providers = move |parent: Block::Hash, ()| {
		let header = header_client
			.header(parent)
			.ok()
			.flatten()
			.ok_or_else(|| format!("the parent header {parent:?} is not in the backend"));
		future::ready(
			header
				.map(|header| inherent_data_providers(first_slot, &header))
				.map_err(Into::into),
		)
	};

	match mode {
		DevSealMode::InstantSeal => {
			let params = sc_consensus_manual_seal::InstantSealParams {
				block_import: client.clone(),
				env,
				client: client.clone(),
				pool: transaction_pool.clone(),
				select_chain: LongestChain::new(backend.clone()),
				consensus_data_provider: Some(consensus_data_provider),
				create_inherent_data_providers,
			};

			let authorship_future = sc_consensus_manual_seal::run_instant_seal(params);
			task_manager.spawn_essential_handle().spawn_blocking(
				"instant-seal",
				None,
				authorship_future,
			);
		},
		DevSealMode::ManualSeal(block_time) => {
			let (mut manual_seal_sink, manual_seal_stream) = futures::channel::mpsc::channel(1024);
			task_manager
				.spawn_essential_handle()
				.spawn("block_authoring", None, async move {
					loop {
						futures_timer::Delay::new(Duration::from_millis(block_time)).await;
						manual_seal_sink
							.try_send(sc_consensus_manual_seal::EngineCommand::SealNewBlock {
								create_empty: true,
								finalize: true,
								parent_hash: None,
								sender: None,
							})
							.expect("the manual-seal stream lives as long as the node; qed");
					}
				});

			let params = sc_consensus_manual_seal::ManualSealParams {
				block_import: client.clone(),
				env,
				client: client.clone(),
				pool: transaction_pool.clone(),
				select_chain: LongestChain::new(backend.clone()),
				commands_stream: Box::pin(manual_seal_stream),
				consensus_data_provider: Some(consensus_data_provider),
				create_inherent_data_providers,
			};

			let authorship_future = sc_consensus_manual_seal::run_manual_seal(params);
			task_manager.spawn_essential_handle().spawn_blocking(
				"manual-seal",
				None,
				authorship_future,
			);
		},
	}

	let rpc_builder = rpc_extensions_builder(
		client.clone(),
		backend.clone(),
		transaction_pool.clone(),
		Arc::new(task_manager.spawn_handle()),
	);
	let database_path = config.database.path().map(|p| p.to_path_buf());

	let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		network,
		client,
		keystore: keystore_container.keystore(),
		task_manager: &mut task_manager,
		transaction_pool,
		rpc_builder,
		backend,
		system_rpc_tx,
		tx_handler_controller,
		sync_service,
		config,
		telemetry: telemetry.as_mut(),
		tracing_execute_block: None,
	})?;

	spawn_storage_monitor(storage_monitor, database_path, &task_manager)?;

	Ok(task_manager)
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_consensus_aura::SlotDuration;
	use sp_core::H256;
	use sp_inherents::InherentDataProvider;
	use sp_runtime::{generic, traits::BlakeTwo256, OpaqueExtrinsic};

	type Header = generic::Header<u32, BlakeTwo256>;
	type Block = generic::Block<Header, OpaqueExtrinsic>;

	fn header(number: u32) -> Header {
		Header::new(
			number,
			Default::default(),
			Default::default(),
			H256::zero(),
			Default::default(),
		)
	}

	async fn digest_on(parent: &Header, first_slot: u32) -> Digest {
		let mut inherents = InherentData::new();
		inherent_data_providers(first_slot, parent)
			.provide_inherent_data(&mut inherents)
			.await
			.expect("the timestamp provider cannot fail");
		JamDevDigests::new(AuraConsensusDataProvider::<Block>::new_with_slot_duration(
			SlotDuration::from_millis(6000),
		))
		.create_digest(parent, &inherents)
		.expect("a dev digest is always created")
	}

	/// `JamSystem::on_initialize` panics on a block without `JamParent`, so every dev block has
	/// to carry one, anchored at the slot `JamFixedVelocityConsensusHook` expects: the one its
	/// timestamp, and therefore its Aura slot, names.
	#[tokio::test]
	async fn every_dev_block_carries_a_jam_parent_anchored_at_its_timestamp() {
		let first = 1_000;
		for number in [0, 1, 7] {
			let parent = header(number);
			let digest = digest_on(&parent, first).await;

			let jam_parent = CumulusDigestItem::find_jam_parent_info(&digest)
				.expect("every dev block carries a JamParent digest");
			assert_eq!(jam_parent.anchor_slot, first + number + 1);
			assert_eq!(jam_parent.lookup_anchor_slot, jam_parent.anchor_slot);
			assert_eq!(
				jam_slot_at(timestamp_for_block(first, &parent)),
				jam_parent.anchor_slot,
				"the anchor is the slot of the block's timestamp",
			);
			assert!(
				digest.logs().iter().any(|item| item
					.as_pre_runtime()
					.is_some_and(|(id, _)| id == sp_consensus_aura::AURA_ENGINE_ID)),
				"the Aura pre-digest is kept",
			);
		}
	}

	#[test]
	fn the_dev_state_names_the_parent_as_included() {
		let parent = H256::repeat_byte(0xab);
		let state = DevJamState::new(ParaId::from(1000), parent.as_ref());
		let key = service_value_state_key(
			parachain_service_core::PARACHAIN_SERVICE_ID,
			&para_info_key(parachain_service_core::types::ParaId::from(1000)),
		);

		let info = <ParaInfo as codec::Decode>::decode(
			&mut &state.read(&key).expect("the para's ParaInfo is present")[..],
		)
		.expect("it decodes");
		assert_eq!(&info.head_data[..], parent.as_bytes());
		assert_eq!(state.read(&[0u8; 31]), None, "every other key is absent");
	}
}
