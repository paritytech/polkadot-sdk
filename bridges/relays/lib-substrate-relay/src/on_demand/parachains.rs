// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! On-demand Substrate -> Substrate parachain finality relay.

use crate::{
	messages_source::best_finalized_peer_header_at_self,
	on_demand::OnDemandRelay,
	parachains::{
		source::ParachainsSource, target::ParachainsTarget, ParachainsPipelineAdapter,
		SubmitParachainHeadsCallBuilder, SubstrateParachainsPipeline,
	},
	TransactionParams,
};

use async_std::{
	channel::{unbounded, Receiver, Sender},
	sync::{Arc, Mutex},
};
use async_trait::async_trait;
use bp_polkadot_core::parachains::{ParaHash, ParaId};
use bp_runtime::HeaderIdProvider;
use futures::{select, FutureExt};
use num_traits::Zero;
use pallet_bridge_parachains::{RelayBlockHash, RelayBlockHasher, RelayBlockNumber};
use parachains_relay::parachains_loop::{AvailableHeader, SourceClient, TargetClient};
use relay_substrate_client::{
	is_ancient_block, AccountIdOf, AccountKeyPairOf, BlockNumberOf, CallOf, Chain, Client,
	Error as SubstrateError, HashOf, HeaderIdOf, ParachainBase,
};
use relay_utils::{
	metrics::MetricsParams, relay_loop::Client as RelayClient, BlockNumberBase, FailedClient,
	HeaderId, UniqueSaturatedInto,
};
use std::fmt::Debug;

/// On-demand Substrate <-> Substrate parachain finality relay.
///
/// This relay may be requested to sync more parachain headers, whenever some other relay
/// (e.g. messages relay) needs it to continue its regular work. When enough parachain headers
/// are relayed, on-demand stops syncing headers.
#[derive(Clone)]
pub struct OnDemandParachainsRelay<P: SubstrateParachainsPipeline> {
	/// Relay task name.
	relay_task_name: String,
	/// Channel used to communicate with background task and ask for relay of parachain heads.
	required_header_number_sender: Sender<BlockNumberOf<P::SourceParachain>>,
	/// Source relay chain client.
	source_relay_client: Client<P::SourceRelayChain>,
	/// Target chain client.
	target_client: Client<P::TargetChain>,
	/// On-demand relay chain relay.
	on_demand_source_relay_to_target_headers:
		Arc<dyn OnDemandRelay<P::SourceRelayChain, P::TargetChain>>,
}

impl<P: SubstrateParachainsPipeline> OnDemandParachainsRelay<P> {
	/// Create new on-demand parachains relay.
	///
	/// Note that the argument is the source relay chain client, not the parachain client.
	/// That's because parachain finality is determined by the relay chain and we don't
	/// need to connect to the parachain itself here.
	pub fn new(
		source_relay_client: Client<P::SourceRelayChain>,
		target_client: Client<P::TargetChain>,
		target_transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
		on_demand_source_relay_to_target_headers: Arc<
			dyn OnDemandRelay<P::SourceRelayChain, P::TargetChain>,
		>,
	) -> Self
	where
		P::SourceParachain: Chain<Hash = ParaHash>,
		P::SourceRelayChain:
			Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash, Hasher = RelayBlockHasher>,
		AccountIdOf<P::TargetChain>:
			From<<AccountKeyPairOf<P::TargetChain> as sp_core::Pair>::Public>,
	{
		let (required_header_number_sender, required_header_number_receiver) = unbounded();
		let this = OnDemandParachainsRelay {
			relay_task_name: on_demand_parachains_relay_name::<P::SourceParachain, P::TargetChain>(
			),
			required_header_number_sender,
			source_relay_client: source_relay_client.clone(),
			target_client: target_client.clone(),
			on_demand_source_relay_to_target_headers: on_demand_source_relay_to_target_headers
				.clone(),
		};
		async_std::task::spawn(async move {
			background_task::<P>(
				source_relay_client,
				target_client,
				target_transaction_params,
				on_demand_source_relay_to_target_headers,
				required_header_number_receiver,
			)
			.await;
		});

		this
	}
}

#[async_trait]
impl<P: SubstrateParachainsPipeline> OnDemandRelay<P::SourceParachain, P::TargetChain>
	for OnDemandParachainsRelay<P>
where
	P::SourceParachain: Chain<Hash = ParaHash>,
{
	async fn reconnect(&self) -> Result<(), SubstrateError> {
		// using clone is fine here (to avoid mut requirement), because clone on Client clones
		// internal references
		self.source_relay_client.clone().reconnect().await?;
		self.target_client.clone().reconnect().await?;
		// we'll probably need to reconnect relay chain relayer clients also
		self.on_demand_source_relay_to_target_headers.reconnect().await
	}

	async fn require_more_headers(&self, required_header: BlockNumberOf<P::SourceParachain>) {
		if let Err(e) = self.required_header_number_sender.send(required_header).await {
			log::trace!(
				target: "bridge",
				"[{}] Failed to request {} header {:?}: {:?}",
				self.relay_task_name,
				P::SourceParachain::NAME,
				required_header,
				e,
			);
		}
	}

	/// Ask relay to prove source `required_header` to the `TargetChain`.
	async fn prove_header(
		&self,
		required_parachain_header: BlockNumberOf<P::SourceParachain>,
	) -> Result<(HeaderIdOf<P::SourceParachain>, Vec<CallOf<P::TargetChain>>), SubstrateError> {
		// select headers to prove
		let parachains_source = ParachainsSource::<P>::new(
			self.source_relay_client.clone(),
			Arc::new(Mutex::new(AvailableHeader::Missing)),
		);
		let env = (self, &parachains_source);
		let (need_to_prove_relay_block, selected_relay_block, selected_parachain_block) =
			select_headers_to_prove(env, required_parachain_header).await?;

		log::debug!(
			target: "bridge",
			"[{}] Requested to prove {} head {:?}. Selected to prove {} head {:?} and {} head {:?}",
			self.relay_task_name,
			P::SourceParachain::NAME,
			required_parachain_header,
			P::SourceParachain::NAME,
			selected_parachain_block,
			P::SourceRelayChain::NAME,
			if need_to_prove_relay_block {
				Some(selected_relay_block)
			} else {
				None
			},
		);

		// now let's prove relay chain block (if needed)
		let mut calls = Vec::new();
		let mut proved_relay_block = selected_relay_block;
		if need_to_prove_relay_block {
			let (relay_block, relay_prove_call) = self
				.on_demand_source_relay_to_target_headers
				.prove_header(selected_relay_block.number())
				.await?;
			proved_relay_block = relay_block;
			calls.extend(relay_prove_call);
		}

		// despite what we've selected before (in `select_headers_to_prove` call), if headers relay
		// have chose the different header (e.g. because there's no GRANDPA jusstification for it),
		// we need to prove parachain head available at this header
		let para_id = ParaId(P::SourceParachain::PARACHAIN_ID);
		let mut proved_parachain_block = selected_parachain_block;
		if proved_relay_block != selected_relay_block {
			proved_parachain_block = parachains_source
				.on_chain_para_head_id(proved_relay_block)
				.await?
				// this could happen e.g. if parachain has been offboarded?
				.ok_or_else(|| {
					SubstrateError::MissingRequiredParachainHead(
						para_id,
						proved_relay_block.number().unique_saturated_into(),
					)
				})?;

			log::debug!(
				target: "bridge",
				"[{}] Selected to prove {} head {:?} and {} head {:?}. Instead proved {} head {:?} and {} head {:?}",
				self.relay_task_name,
				P::SourceParachain::NAME,
				selected_parachain_block,
				P::SourceRelayChain::NAME,
				selected_relay_block,
				P::SourceParachain::NAME,
				proved_parachain_block,
				P::SourceRelayChain::NAME,
				proved_relay_block,
			);
		}

		// and finally - prove parachain head
		let (para_proof, para_hash) =
			parachains_source.prove_parachain_head(proved_relay_block).await?;
		calls.push(P::SubmitParachainHeadsCallBuilder::build_submit_parachain_heads_call(
			proved_relay_block,
			vec![(para_id, para_hash)],
			para_proof,
			false,
		));

		Ok((proved_parachain_block, calls))
	}
}

/// Background task that is responsible for starting parachain headers relay.
async fn background_task<P: SubstrateParachainsPipeline>(
	source_relay_client: Client<P::SourceRelayChain>,
	target_client: Client<P::TargetChain>,
	target_transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
	on_demand_source_relay_to_target_headers: Arc<
		dyn OnDemandRelay<P::SourceRelayChain, P::TargetChain>,
	>,
	required_parachain_header_number_receiver: Receiver<BlockNumberOf<P::SourceParachain>>,
) where
	P::SourceParachain: Chain<Hash = ParaHash>,
	P::SourceRelayChain:
		Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash, Hasher = RelayBlockHasher>,
	AccountIdOf<P::TargetChain>: From<<AccountKeyPairOf<P::TargetChain> as sp_core::Pair>::Public>,
{
	let relay_task_name = on_demand_parachains_relay_name::<P::SourceParachain, P::TargetChain>();
	let target_transactions_mortality = target_transaction_params.mortality;

	let mut relay_state = RelayState::Idle;
	let mut required_parachain_header_number = Zero::zero();
	let required_para_header_ref = Arc::new(Mutex::new(AvailableHeader::Unavailable));

	let mut restart_relay = true;
	let parachains_relay_task = futures::future::Fuse::terminated();
	futures::pin_mut!(parachains_relay_task);

	let mut parachains_source =
		ParachainsSource::<P>::new(source_relay_client.clone(), required_para_header_ref.clone());
	let mut parachains_target = ParachainsTarget::<P>::new(
		source_relay_client.clone(),
		target_client.clone(),
		target_transaction_params.clone(),
	);

	loop {
		select! {
			new_required_parachain_header_number = required_parachain_header_number_receiver.recv().fuse() => {
				let new_required_parachain_header_number = match new_required_parachain_header_number {
					Ok(new_required_parachain_header_number) => new_required_parachain_header_number,
					Err(e) => {
						log::error!(
							target: "bridge",
							"[{}] Background task has exited with error: {:?}",
							relay_task_name,
							e,
						);

						return;
					},
				};

				// keep in mind that we are not updating `required_para_header_ref` here, because
				// then we'll be submitting all previous headers as well (while required relay headers are
				// delivered) and we want to avoid that (to reduce cost)
				if new_required_parachain_header_number > required_parachain_header_number {
					log::trace!(
						target: "bridge",
						"[{}] More {} headers required. Going to sync up to the {}",
						relay_task_name,
						P::SourceParachain::NAME,
						new_required_parachain_header_number,
					);

					required_parachain_header_number = new_required_parachain_header_number;
				}
			},
			_ = async_std::task::sleep(P::TargetChain::AVERAGE_BLOCK_INTERVAL).fuse() => {},
			_ = parachains_relay_task => {
				// this should never happen in practice given the current code
				restart_relay = true;
			},
		}

		// the workflow of the on-demand parachains relay is:
		//
		// 1) message relay (or any other dependent relay) sees new message at parachain header
		// `PH`;
		//
		// 2) it sees that the target chain does not know `PH`;
		//
		// 3) it asks on-demand parachains relay to relay `PH` to the target chain;
		//
		// Phase#1: relaying relay chain header
		//
		// 4) on-demand parachains relay waits for GRANDPA-finalized block of the source relay chain
		//    `RH` that is storing `PH` or its descendant. Let it be `PH'`;
		// 5) it asks on-demand headers relay to relay `RH` to the target chain;
		// 6) it waits until `RH` (or its descendant) is relayed to the target chain;
		//
		// Phase#2: relaying parachain header
		//
		// 7) on-demand parachains relay sets `ParachainsSource::maximal_header_number` to the
		//    `PH'.number()`.
		// 8) parachains finality relay sees that the parachain head has been updated and relays
		//    `PH'` to    the target chain.

		// select headers to relay
		let relay_data = read_relay_data(
			&parachains_source,
			&parachains_target,
			required_parachain_header_number,
		)
		.await;
		match relay_data {
			Ok(relay_data) => {
				let prev_relay_state = relay_state;
				relay_state = select_headers_to_relay(&relay_data, relay_state);
				log::trace!(
					target: "bridge",
					"[{}] Selected new relay state: {:?} using old state {:?} and data {:?}",
					relay_task_name,
					relay_state,
					prev_relay_state,
					relay_data,
				);
			},
			Err(failed_client) => {
				relay_utils::relay_loop::reconnect_failed_client(
					failed_client,
					relay_utils::relay_loop::RECONNECT_DELAY,
					&mut parachains_source,
					&mut parachains_target,
				)
				.await;
				continue
			},
		}

		// we have selected our new 'state' => let's notify our source clients about our new
		// requirements
		match relay_state {
			RelayState::Idle => (),
			RelayState::RelayingRelayHeader(required_relay_header) => {
				on_demand_source_relay_to_target_headers
					.require_more_headers(required_relay_header)
					.await;
			},
			RelayState::RelayingParaHeader(required_para_header) => {
				*required_para_header_ref.lock().await =
					AvailableHeader::Available(required_para_header);
			},
		}

		// start/restart relay
		if restart_relay {
			let stall_timeout = relay_substrate_client::transaction_stall_timeout(
				target_transactions_mortality,
				P::TargetChain::AVERAGE_BLOCK_INTERVAL,
				relay_utils::STALL_TIMEOUT,
			);

			log::info!(
				target: "bridge",
				"[{}] Starting on-demand-parachains relay task\n\t\
					Tx mortality: {:?} (~{}m)\n\t\
					Stall timeout: {:?}",
				relay_task_name,
				target_transactions_mortality,
				stall_timeout.as_secs_f64() / 60.0f64,
				stall_timeout,
			);

			parachains_relay_task.set(
				parachains_relay::parachains_loop::run(
					parachains_source.clone(),
					parachains_target.clone(),
					MetricsParams::disabled(),
					// we do not support free parachain headers relay in on-demand relays
					false,
					futures::future::pending(),
				)
				.fuse(),
			);

			restart_relay = false;
		}
	}
}

/// On-demand parachains relay task name.
fn on_demand_parachains_relay_name<SourceChain: Chain, TargetChain: Chain>() -> String {
	format!("{}-to-{}-on-demand-parachain", SourceChain::NAME, TargetChain::NAME)
}

/// On-demand relay state.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RelayState<ParaHash, ParaNumber, RelayNumber> {
	/// On-demand relay is not doing anything.
	Idle,
	/// Relaying given relay header to relay given parachain header later.
	RelayingRelayHeader(RelayNumber),
	/// Relaying given parachain header.
	RelayingParaHeader(HeaderId<ParaHash, ParaNumber>),
}

/// Data gathered from source and target clients, used by on-demand relay.
#[derive(Debug)]
struct RelayData<ParaHash, ParaNumber, RelayNumber> {
	/// Parachain header number that is required at the target chain.
	pub required_para_header: ParaNumber,
	/// Parachain header number, known to the target chain.
	pub para_header_at_target: Option<ParaNumber>,
	/// Parachain header id, known to the source (relay) chain.
	pub para_header_at_source: Option<HeaderId<ParaHash, ParaNumber>>,
	/// Parachain header, that is available at the source relay chain at `relay_header_at_target`
	/// block.
	///
	/// May be `None` if there's no `relay_header_at_target` yet, or if the
	/// `relay_header_at_target` is too old and we think its state has been pruned.
	pub para_header_at_relay_header_at_target: Option<HeaderId<ParaHash, ParaNumber>>,
	/// Relay header number at the source chain.
	pub relay_header_at_source: RelayNumber,
	/// Relay header number at the target chain.
	pub relay_header_at_target: Option<RelayNumber>,
}

/// Read required data from source and target clients.
async fn read_relay_data<P: SubstrateParachainsPipeline>(
	source: &ParachainsSource<P>,
	target: &ParachainsTarget<P>,
	required_header_number: BlockNumberOf<P::SourceParachain>,
) -> Result<
	RelayData<
		HashOf<P::SourceParachain>,
		BlockNumberOf<P::SourceParachain>,
		BlockNumberOf<P::SourceRelayChain>,
	>,
	FailedClient,
>
where
	ParachainsTarget<P>:
		TargetClient<ParachainsPipelineAdapter<P>> + RelayClient<Error = SubstrateError>,
{
	let map_target_err = |e| {
		log::error!(
			target: "bridge",
			"[{}] Failed to read relay data from {} client: {:?}",
			on_demand_parachains_relay_name::<P::SourceParachain, P::TargetChain>(),
			P::TargetChain::NAME,
			e,
		);
		FailedClient::Target
	};
	let map_source_err = |e| {
		log::error!(
			target: "bridge",
			"[{}] Failed to read relay data from {} client: {:?}",
			on_demand_parachains_relay_name::<P::SourceParachain, P::TargetChain>(),
			P::SourceRelayChain::NAME,
			e,
		);
		FailedClient::Source
	};

	let best_target_block_hash = target.best_block().await.map_err(map_target_err)?.1;
	let para_header_at_target = best_finalized_peer_header_at_self::<
		P::TargetChain,
		P::SourceParachain,
	>(target.target_client(), best_target_block_hash)
	.await;
	// if there are no parachain heads at the target (`NoParachainHeadAtTarget`), we'll need to
	// submit at least one. Otherwise the pallet will be treated as uninitialized and messages
	// sync will stall.
	let para_header_at_target = match para_header_at_target {
		Ok(Some(para_header_at_target)) => Some(para_header_at_target.0),
		Ok(None) => None,
		Err(e) => return Err(map_target_err(e)),
	};

	let best_finalized_relay_header =
		source.client().best_finalized_header().await.map_err(map_source_err)?;
	let best_finalized_relay_block_id = best_finalized_relay_header.id();
	let para_header_at_source = source
		.on_chain_para_head_id(best_finalized_relay_block_id)
		.await
		.map_err(map_source_err)?;

	let relay_header_at_source = best_finalized_relay_block_id.0;
	let relay_header_at_target = best_finalized_peer_header_at_self::<
		P::TargetChain,
		P::SourceRelayChain,
	>(target.target_client(), best_target_block_hash)
	.await
	.map_err(map_target_err)?;

	// if relay header at target is too old then its state may already be discarded at the source
	// => just use `None` in this case
	//
	// the same is for case when there's no relay header at target at all
	let available_relay_header_at_target =
		relay_header_at_target.filter(|relay_header_at_target| {
			!is_ancient_block(relay_header_at_target.number(), relay_header_at_source)
		});
	let para_header_at_relay_header_at_target =
		if let Some(available_relay_header_at_target) = available_relay_header_at_target {
			source
				.on_chain_para_head_id(available_relay_header_at_target)
				.await
				.map_err(map_source_err)?
		} else {
			None
		};

	Ok(RelayData {
		required_para_header: required_header_number,
		para_header_at_target,
		para_header_at_source,
		relay_header_at_source,
		relay_header_at_target: relay_header_at_target
			.map(|relay_header_at_target| relay_header_at_target.0),
		para_header_at_relay_header_at_target,
	})
}

/// Select relay and parachain headers that need to be relayed.
fn select_headers_to_relay<ParaHash, ParaNumber, RelayNumber>(
	data: &RelayData<ParaHash, ParaNumber, RelayNumber>,
	state: RelayState<ParaHash, ParaNumber, RelayNumber>,
) -> RelayState<ParaHash, ParaNumber, RelayNumber>
where
	ParaHash: Clone,
	ParaNumber: Copy + PartialOrd + Zero,
	RelayNumber: Copy + Debug + Ord,
{
	// we can't do anything until **relay chain** bridge GRANDPA pallet is not initialized at the
	// target chain
	let relay_header_at_target = match data.relay_header_at_target {
		Some(relay_header_at_target) => relay_header_at_target,
		None => return RelayState::Idle,
	};

	// Process the `RelayingRelayHeader` state.
	if let &RelayState::RelayingRelayHeader(relay_header_number) = &state {
		if relay_header_at_target < relay_header_number {
			// The required relay header hasn't yet been relayed. Ask / wait for it.
			return state
		}

		// We may switch to `RelayingParaHeader` if parachain head is available.
		if let Some(para_header_at_relay_header_at_target) =
			data.para_header_at_relay_header_at_target.as_ref()
		{
			return RelayState::RelayingParaHeader(para_header_at_relay_header_at_target.clone())
		}

		// else use the regular process - e.g. we may require to deliver new relay header first
	}

	// Process the `RelayingParaHeader` state.
	if let RelayState::RelayingParaHeader(para_header_id) = &state {
		let para_header_at_target_or_zero = data.para_header_at_target.unwrap_or_else(Zero::zero);
		if para_header_at_target_or_zero < para_header_id.0 {
			// The required parachain header hasn't yet been relayed. Ask / wait for it.
			return state
		}
	}

	// if we haven't read para head from the source, we can't yet do anything
	let para_header_at_source = match data.para_header_at_source {
		Some(ref para_header_at_source) => para_header_at_source.clone(),
		None => return RelayState::Idle,
	};

	// if we have parachain head at the source, but no parachain heads at the target, we'll need
	// to deliver at least one parachain head
	let (required_para_header, para_header_at_target) = match data.para_header_at_target {
		Some(para_header_at_target) => (data.required_para_header, para_header_at_target),
		None => (para_header_at_source.0, Zero::zero()),
	};

	// if we have already satisfied our "customer", do nothing
	if required_para_header <= para_header_at_target {
		return RelayState::Idle
	}

	// if required header is not available even at the source chain, let's wait
	if required_para_header > para_header_at_source.0 {
		return RelayState::Idle
	}

	// we will always try to sync latest parachain/relay header, even if we've been asked for some
	// its ancestor

	// we need relay chain header first
	if relay_header_at_target < data.relay_header_at_source {
		return RelayState::RelayingRelayHeader(data.relay_header_at_source)
	}

	// if all relay headers synced, we may start directly with parachain header
	RelayState::RelayingParaHeader(para_header_at_source)
}

/// Environment for the `select_headers_to_prove` call.
#[async_trait]
trait SelectHeadersToProveEnvironment<RBN, RBH, PBN, PBH> {
	/// Returns associated parachain id.
	fn parachain_id(&self) -> ParaId;
	/// Returns best finalized relay block.
	async fn best_finalized_relay_block_at_source(
		&self,
	) -> Result<HeaderId<RBH, RBN>, SubstrateError>;
	/// Returns best finalized relay block that is known at `P::TargetChain`.
	async fn best_finalized_relay_block_at_target(
		&self,
	) -> Result<HeaderId<RBH, RBN>, SubstrateError>;
	/// Returns best finalized parachain block at given source relay chain block.
	async fn best_finalized_para_block_at_source(
		&self,
		at_relay_block: HeaderId<RBH, RBN>,
	) -> Result<Option<HeaderId<PBH, PBN>>, SubstrateError>;
}

#[async_trait]
impl<'a, P: SubstrateParachainsPipeline>
	SelectHeadersToProveEnvironment<
		BlockNumberOf<P::SourceRelayChain>,
		HashOf<P::SourceRelayChain>,
		BlockNumberOf<P::SourceParachain>,
		HashOf<P::SourceParachain>,
	> for (&'a OnDemandParachainsRelay<P>, &'a ParachainsSource<P>)
{
	fn parachain_id(&self) -> ParaId {
		ParaId(P::SourceParachain::PARACHAIN_ID)
	}

	async fn best_finalized_relay_block_at_source(
		&self,
	) -> Result<HeaderIdOf<P::SourceRelayChain>, SubstrateError> {
		Ok(self.0.source_relay_client.best_finalized_header().await?.id())
	}

	async fn best_finalized_relay_block_at_target(
		&self,
	) -> Result<HeaderIdOf<P::SourceRelayChain>, SubstrateError> {
		Ok(crate::messages_source::read_client_state::<P::TargetChain, P::SourceRelayChain>(
			&self.0.target_client,
			None,
		)
		.await?
		.best_finalized_peer_at_best_self
		.ok_or(SubstrateError::BridgePalletIsNotInitialized)?)
	}

	async fn best_finalized_para_block_at_source(
		&self,
		at_relay_block: HeaderIdOf<P::SourceRelayChain>,
	) -> Result<Option<HeaderIdOf<P::SourceParachain>>, SubstrateError> {
		self.1.on_chain_para_head_id(at_relay_block).await
	}
}

/// Given request to prove `required_parachain_header`, select actual headers that need to be
/// proved.
async fn select_headers_to_prove<RBN, RBH, PBN, PBH>(
	env: impl SelectHeadersToProveEnvironment<RBN, RBH, PBN, PBH>,
	required_parachain_header: PBN,
) -> Result<(bool, HeaderId<RBH, RBN>, HeaderId<PBH, PBN>), SubstrateError>
where
	RBH: Copy,
	RBN: BlockNumberBase,
	PBH: Copy,
	PBN: BlockNumberBase,
{
	// parachains proof also requires relay header proof. Let's first select relay block
	// number that we'll be dealing with
	let best_finalized_relay_block_at_source = env.best_finalized_relay_block_at_source().await?;
	let best_finalized_relay_block_at_target = env.best_finalized_relay_block_at_target().await?;

	// if we can't prove `required_header` even using `best_finalized_relay_block_at_source`, we
	// can't do anything here
	// (this shall not actually happen, given current code, because we only require finalized
	// headers)
	let best_possible_parachain_block = env
		.best_finalized_para_block_at_source(best_finalized_relay_block_at_source)
		.await?
		.filter(|best_possible_parachain_block| {
			best_possible_parachain_block.number() >= required_parachain_header
		})
		.ok_or(SubstrateError::MissingRequiredParachainHead(
			env.parachain_id(),
			required_parachain_header.unique_saturated_into(),
		))?;

	// we don't require source node to be archive, so we can't craft storage proofs using
	// ancient headers. So if the `best_finalized_relay_block_at_target` is too ancient, we
	// can't craft storage proofs using it
	let may_use_state_at_best_finalized_relay_block_at_target = !is_ancient_block(
		best_finalized_relay_block_at_target.number(),
		best_finalized_relay_block_at_source.number(),
	);

	// now let's check if `required_header` may be proved using
	// `best_finalized_relay_block_at_target`
	let selection = if may_use_state_at_best_finalized_relay_block_at_target {
		env.best_finalized_para_block_at_source(best_finalized_relay_block_at_target)
			.await?
			.filter(|best_finalized_para_block_at_target| {
				best_finalized_para_block_at_target.number() >= required_parachain_header
			})
			.map(|best_finalized_para_block_at_target| {
				(false, best_finalized_relay_block_at_target, best_finalized_para_block_at_target)
			})
	} else {
		None
	};

	Ok(selection.unwrap_or((
		true,
		best_finalized_relay_block_at_source,
		best_possible_parachain_block,
	)))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn relay_waits_for_relay_header_to_be_delivered() {
		assert_eq!(
			select_headers_to_relay(
				&RelayData {
					required_para_header: 90,
					para_header_at_target: Some(50),
					para_header_at_source: Some(HeaderId(110, 110)),
					relay_header_at_source: 800,
					relay_header_at_target: Some(700),
					para_header_at_relay_header_at_target: Some(HeaderId(100, 100)),
				},
				RelayState::RelayingRelayHeader(750),
			),
			RelayState::RelayingRelayHeader(750),
		);
	}

	#[test]
	fn relay_starts_relaying_requested_para_header_after_relay_header_is_delivered() {
		assert_eq!(
			select_headers_to_relay(
				&RelayData {
					required_para_header: 90,
					para_header_at_target: Some(50),
					para_header_at_source: Some(HeaderId(110, 110)),
					relay_header_at_source: 800,
					relay_header_at_target: Some(750),
					para_header_at_relay_header_at_target: Some(HeaderId(100, 100)),
				},
				RelayState::RelayingRelayHeader(750),
			),
			RelayState::RelayingParaHeader(HeaderId(100, 100)),
		);
	}

	#[test]
	fn relay_selects_better_para_header_after_better_relay_header_is_delivered() {
		assert_eq!(
			select_headers_to_relay(
				&RelayData {
					required_para_header: 90,
					para_header_at_target: Some(50),
					para_header_at_source: Some(HeaderId(110, 110)),
					relay_header_at_source: 800,
					relay_header_at_target: Some(780),
					para_header_at_relay_header_at_target: Some(HeaderId(105, 105)),
				},
				RelayState::RelayingRelayHeader(750),
			),
			RelayState::RelayingParaHeader(HeaderId(105, 105)),
		);
	}
	#[test]
	fn relay_waits_for_para_header_to_be_delivered() {
		assert_eq!(
			select_headers_to_relay(
				&RelayData {
					required_para_header: 90,
					para_header_at_target: Some(50),
					para_header_at_source: Some(HeaderId(110, 110)),
					relay_header_at_source: 800,
					relay_header_at_target: Some(780),
					para_header_at_relay_header_at_target: Some(HeaderId(105, 105)),
				},
				RelayState::RelayingParaHeader(HeaderId(105, 105)),
			),
			RelayState::RelayingParaHeader(HeaderId(105, 105)),
		);
	}

	#[test]
	fn relay_stays_idle_if_required_para_header_is_already_delivered() {
		assert_eq!(
			select_headers_to_relay(
				&RelayData {
					required_para_header: 90,
					para_header_at_target: Some(105),
					para_header_at_source: Some(HeaderId(110, 110)),
					relay_header_at_source: 800,
					relay_header_at_target: Some(780),
					para_header_at_relay_header_at_target: Some(HeaderId(105, 105)),
				},
				RelayState::Idle,
			),
			RelayState::Idle,
		);
	}

	#[test]
	fn relay_waits_for_required_para_header_to_appear_at_source_1() {
		assert_eq!(
			select_headers_to_relay(
				&RelayData {
					required_para_header: 120,
					para_header_at_target: Some(105),
					para_header_at_source: None,
					relay_header_at_source: 800,
					relay_header_at_target: Some(780),
					para_header_at_relay_header_at_target: Some(HeaderId(105, 105)),
				},
				RelayState::Idle,
			),
			RelayState::Idle,
		);
	}

	#[test]
	fn relay_waits_for_required_para_header_to_appear_at_source_2() {
		assert_eq!(
			select_headers_to_relay(
				&RelayData {
					required_para_header: 120,
					para_header_at_target: Some(105),
					para_header_at_source: Some(HeaderId(110, 110)),
					relay_header_at_source: 800,
					relay_header_at_target: Some(780),
					para_header_at_relay_header_at_target: Some(HeaderId(105, 105)),
				},
				RelayState::Idle,
			),
			RelayState::Idle,
		);
	}

	#[test]
	fn relay_starts_relaying_relay_header_when_new_para_header_is_requested() {
		assert_eq!(
			select_headers_to_relay(
				&RelayData {
					required_para_header: 120,
					para_header_at_target: Some(105),
					para_header_at_source: Some(HeaderId(125, 125)),
					relay_header_at_source: 800,
					relay_header_at_target: Some(780),
					para_header_at_relay_header_at_target: Some(HeaderId(105, 105)),
				},
				RelayState::Idle,
			),
			RelayState::RelayingRelayHeader(800),
		);
	}

	#[test]
	fn relay_starts_relaying_para_header_when_new_para_header_is_requested() {
		assert_eq!(
			select_headers_to_relay(
				&RelayData {
					required_para_header: 120,
					para_header_at_target: Some(105),
					para_header_at_source: Some(HeaderId(125, 125)),
					relay_header_at_source: 800,
					relay_header_at_target: Some(800),
					para_header_at_relay_header_at_target: Some(HeaderId(125, 125)),
				},
				RelayState::Idle,
			),
			RelayState::RelayingParaHeader(HeaderId(125, 125)),
		);
	}

	#[test]
	fn relay_goes_idle_when_parachain_is_deregistered() {
		assert_eq!(
			select_headers_to_relay::<i32, _, _>(
				&RelayData {
					required_para_header: 120,
					para_header_at_target: Some(105),
					para_header_at_source: None,
					relay_header_at_source: 800,
					relay_header_at_target: Some(800),
					para_header_at_relay_header_at_target: None,
				},
				RelayState::RelayingRelayHeader(800),
			),
			RelayState::Idle,
		);
	}

	#[test]
	fn relay_starts_relaying_first_parachain_header() {
		assert_eq!(
			select_headers_to_relay::<i32, _, _>(
				&RelayData {
					required_para_header: 0,
					para_header_at_target: None,
					para_header_at_source: Some(HeaderId(125, 125)),
					relay_header_at_source: 800,
					relay_header_at_target: Some(800),
					para_header_at_relay_header_at_target: Some(HeaderId(125, 125)),
				},
				RelayState::Idle,
			),
			RelayState::RelayingParaHeader(HeaderId(125, 125)),
		);
	}

	#[test]
	fn relay_starts_relaying_relay_header_to_relay_first_parachain_header() {
		assert_eq!(
			select_headers_to_relay::<i32, _, _>(
				&RelayData {
					required_para_header: 0,
					para_header_at_target: None,
					para_header_at_source: Some(HeaderId(125, 125)),
					relay_header_at_source: 800,
					relay_header_at_target: Some(700),
					para_header_at_relay_header_at_target: Some(HeaderId(125, 125)),
				},
				RelayState::Idle,
			),
			RelayState::RelayingRelayHeader(800),
		);
	}

	// tuple is:
	//
	// - best_finalized_relay_block_at_source
	// - best_finalized_relay_block_at_target
	// - best_finalized_para_block_at_source at best_finalized_relay_block_at_source
	// - best_finalized_para_block_at_source at best_finalized_relay_block_at_target
	#[async_trait]
	impl SelectHeadersToProveEnvironment<u32, u32, u32, u32> for (u32, u32, u32, u32) {
		fn parachain_id(&self) -> ParaId {
			ParaId(0)
		}

		async fn best_finalized_relay_block_at_source(
			&self,
		) -> Result<HeaderId<u32, u32>, SubstrateError> {
			Ok(HeaderId(self.0, self.0))
		}

		async fn best_finalized_relay_block_at_target(
			&self,
		) -> Result<HeaderId<u32, u32>, SubstrateError> {
			Ok(HeaderId(self.1, self.1))
		}

		async fn best_finalized_para_block_at_source(
			&self,
			at_relay_block: HeaderId<u32, u32>,
		) -> Result<Option<HeaderId<u32, u32>>, SubstrateError> {
			if at_relay_block.0 == self.0 {
				Ok(Some(HeaderId(self.2, self.2)))
			} else if at_relay_block.0 == self.1 {
				Ok(Some(HeaderId(self.3, self.3)))
			} else {
				Ok(None)
			}
		}
	}

	#[async_std::test]
	async fn select_headers_to_prove_returns_err_if_required_para_block_is_missing_at_source() {
		assert!(matches!(
			select_headers_to_prove((20_u32, 10_u32, 200_u32, 100_u32), 300_u32,).await,
			Err(SubstrateError::MissingRequiredParachainHead(ParaId(0), 300_u64)),
		));
	}

	#[async_std::test]
	async fn select_headers_to_prove_fails_to_use_existing_ancient_relay_block() {
		assert_eq!(
			select_headers_to_prove((220_u32, 10_u32, 200_u32, 100_u32), 100_u32,)
				.await
				.map_err(drop),
			Ok((true, HeaderId(220, 220), HeaderId(200, 200))),
		);
	}

	#[async_std::test]
	async fn select_headers_to_prove_is_able_to_use_existing_recent_relay_block() {
		assert_eq!(
			select_headers_to_prove((40_u32, 10_u32, 200_u32, 100_u32), 100_u32,)
				.await
				.map_err(drop),
			Ok((false, HeaderId(10, 10), HeaderId(100, 100))),
		);
	}

	#[async_std::test]
	async fn select_headers_to_prove_uses_new_relay_block() {
		assert_eq!(
			select_headers_to_prove((20_u32, 10_u32, 200_u32, 100_u32), 200_u32,)
				.await
				.map_err(drop),
			Ok((true, HeaderId(20, 20), HeaderId(200, 200))),
		);
	}
}
