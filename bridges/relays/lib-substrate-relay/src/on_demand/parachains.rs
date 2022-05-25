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
		SubstrateParachainsPipeline,
	},
	TransactionParams,
};

use async_std::{
	channel::{unbounded, Receiver, Sender},
	sync::{Arc, Mutex},
};
use async_trait::async_trait;
use bp_polkadot_core::parachains::ParaHash;
use futures::{select, FutureExt};
use num_traits::Zero;
use pallet_bridge_parachains::{RelayBlockHash, RelayBlockHasher, RelayBlockNumber};
use parachains_relay::parachains_loop::{ParachainSyncParams, TargetClient};
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, BlockNumberOf, Chain, Client, Error as SubstrateError,
	TransactionSignScheme,
};
use relay_utils::{
	metrics::MetricsParams, relay_loop::Client as RelayClient, FailedClient, HeaderId,
};
use sp_runtime::traits::Header as HeaderT;
use std::{cmp::Ordering, collections::BTreeMap};

/// On-demand Substrate <-> Substrate parachain finality relay.
///
/// This relay may be requested to sync more parachain headers, whenever some other relay
/// (e.g. messages relay) needs it to continue its regular work. When enough parachain headers
/// are relayed, on-demand stops syncing headers.
#[derive(Clone)]
pub struct OnDemandParachainsRelay<SourceParachain: Chain> {
	/// Relay task name.
	relay_task_name: String,
	/// Channel used to communicate with background task and ask for relay of parachain heads.
	required_header_number_sender: Sender<BlockNumberOf<SourceParachain>>,
}

impl<SourceParachain: Chain> OnDemandParachainsRelay<SourceParachain> {
	/// Create new on-demand parachains relay.
	///
	/// Note that the argument is the source relay chain client, not the parachain client.
	/// That's because parachain finality is determined by the relay chain and we don't
	/// need to connect to the parachain itself here.
	pub fn new<P: SubstrateParachainsPipeline<SourceParachain = SourceParachain>>(
		source_relay_client: Client<P::SourceRelayChain>,
		target_client: Client<P::TargetChain>,
		target_transaction_params: TransactionParams<AccountKeyPairOf<P::TransactionSignScheme>>,
		on_demand_source_relay_to_target_headers: Arc<
			dyn OnDemandRelay<BlockNumberOf<P::SourceRelayChain>>,
		>,
	) -> Self
	where
		P::SourceParachain: Chain<Hash = ParaHash>,
		P::SourceRelayChain:
			Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash, Hasher = RelayBlockHasher>,
		AccountIdOf<P::TargetChain>:
			From<<AccountKeyPairOf<P::TransactionSignScheme> as sp_core::Pair>::Public>,
		P::TransactionSignScheme: TransactionSignScheme<Chain = P::TargetChain>,
	{
		let (required_header_number_sender, required_header_number_receiver) = unbounded();
		let this = OnDemandParachainsRelay {
			relay_task_name: on_demand_parachains_relay_name::<SourceParachain, P::TargetChain>(),
			required_header_number_sender,
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
impl<SourceParachain> OnDemandRelay<BlockNumberOf<SourceParachain>>
	for OnDemandParachainsRelay<SourceParachain>
where
	SourceParachain: Chain,
{
	async fn require_more_headers(&self, required_header: BlockNumberOf<SourceParachain>) {
		if let Err(e) = self.required_header_number_sender.send(required_header).await {
			log::trace!(
				target: "bridge",
				"Failed to request {} header {:?} in {:?}: {:?}",
				SourceParachain::NAME,
				required_header,
				self.relay_task_name,
				e,
			);
		}
	}
}

/// Background task that is responsible for starting parachain headers relay.
async fn background_task<P: SubstrateParachainsPipeline>(
	source_relay_client: Client<P::SourceRelayChain>,
	target_client: Client<P::TargetChain>,
	target_transaction_params: TransactionParams<AccountKeyPairOf<P::TransactionSignScheme>>,
	on_demand_source_relay_to_target_headers: Arc<
		dyn OnDemandRelay<BlockNumberOf<P::SourceRelayChain>>,
	>,
	required_parachain_header_number_receiver: Receiver<BlockNumberOf<P::SourceParachain>>,
) where
	P::SourceParachain: Chain<Hash = ParaHash>,
	P::SourceRelayChain:
		Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash, Hasher = RelayBlockHasher>,
	AccountIdOf<P::TargetChain>:
		From<<AccountKeyPairOf<P::TransactionSignScheme> as sp_core::Pair>::Public>,
	P::TransactionSignScheme: TransactionSignScheme<Chain = P::TargetChain>,
{
	let relay_task_name = on_demand_parachains_relay_name::<P::SourceParachain, P::TargetChain>();
	let target_transactions_mortality = target_transaction_params.mortality;

	let mut relay_state = RelayState::Idle;
	let mut headers_map_cache = BTreeMap::new();
	let mut required_parachain_header_number = Zero::zero();
	let required_para_header_number_ref = Arc::new(Mutex::new(required_parachain_header_number));

	let mut restart_relay = true;
	let parachains_relay_task = futures::future::Fuse::terminated();
	futures::pin_mut!(parachains_relay_task);

	let mut parachains_source = ParachainsSource::<P>::new(
		source_relay_client.clone(),
		Some(required_para_header_number_ref.clone()),
	);
	let mut parachains_target =
		ParachainsTarget::<P>::new(target_client.clone(), target_transaction_params.clone());

	loop {
		select! {
			new_required_parachain_header_number = required_parachain_header_number_receiver.recv().fuse() => {
				let new_required_parachain_header_number = match new_required_parachain_header_number {
					Ok(new_required_parachain_header_number) => new_required_parachain_header_number,
					Err(e) => {
						log::error!(
							target: "bridge",
							"Background task of {} has exited with error: {:?}",
							relay_task_name,
							e,
						);

						return;
					},
				};

				// keep in mind that we are not updating `required_para_header_number_ref` here, because
				// then we'll be submitting all previous headers as well (while required relay headers are
				// delivered) and we want to avoid that (to reduce cost)
				required_parachain_header_number = std::cmp::max(
					required_parachain_header_number,
					new_required_parachain_header_number,
				);
			},
			_ = parachains_relay_task => {
				// this should never happen in practice given the current code
				restart_relay = true;
			},
		}

		// the workflow of the on-demand parachains relay is:
		//
		// 1) message relay (or any other dependent relay) sees new message at parachain header
		// `PH`; 2) it sees that the target chain does not know `PH`;
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
		// `PH'.number()`. 8) parachains finality relay sees that the parachain head has been
		// updated and relays `PH'` to    the target chain.

		// select headers to relay
		let relay_data = read_relay_data(
			&parachains_source,
			&parachains_target,
			required_parachain_header_number,
			&mut headers_map_cache,
		)
		.await;
		match relay_data {
			Ok(mut relay_data) => {
				let prev_relay_state = relay_state;
				relay_state = select_headers_to_relay(&mut relay_data, relay_state);
				log::trace!(
					target: "bridge",
					"Selected new relay state in {}: {:?} using old state {:?} and data {:?}",
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
			RelayState::RelayingRelayHeader(required_relay_header, _) => {
				on_demand_source_relay_to_target_headers
					.require_more_headers(required_relay_header)
					.await;
			},
			RelayState::RelayingParaHeader(required_para_header) => {
				*required_para_header_number_ref.lock().await = required_para_header;
			},
		}

		// start/restart relay
		if restart_relay {
			let stall_timeout = relay_substrate_client::transaction_stall_timeout(
				target_transactions_mortality,
				P::TargetChain::AVERAGE_BLOCK_INTERVAL,
				crate::STALL_TIMEOUT,
			);

			log::info!(
				target: "bridge",
				"Starting {} relay\n\t\
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
					ParachainSyncParams {
						parachains: vec![P::SOURCE_PARACHAIN_PARA_ID.into()],
						stall_timeout: std::time::Duration::from_secs(60),
						strategy: parachains_relay::parachains_loop::ParachainSyncStrategy::Any,
					},
					MetricsParams::disabled(),
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
	format!("on-demand-{}-to-{}", SourceChain::NAME, TargetChain::NAME)
}

/// On-demand relay state.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RelayState<SourceParaBlock, SourceRelayBlock> {
	/// On-demand relay is not doing anything.
	Idle,
	/// Relaying given relay header to relay given parachain header later.
	RelayingRelayHeader(SourceRelayBlock, SourceParaBlock),
	/// Relaying given parachain header.
	RelayingParaHeader(SourceParaBlock),
}

/// Data gathered from source and target clients, used by on-demand relay.
#[derive(Debug)]
struct RelayData<'a, SourceParaBlock, SourceRelayBlock> {
	/// Parachain header number that is required at the target chain.
	pub required_para_header: SourceParaBlock,
	/// Parachain header number, known to the target chain.
	pub para_header_at_target: SourceParaBlock,
	/// Parachain header number, known to the source (relay) chain.
	pub para_header_at_source: Option<SourceParaBlock>,
	/// Relay header number at the source chain.
	pub relay_header_at_source: SourceRelayBlock,
	/// Relay header number at the target chain.
	pub relay_header_at_target: SourceRelayBlock,
	/// Map of relay to para header block numbers for recent relay headers.
	///
	/// Even if we have been trying to relay relay header #100 to relay parachain header #50
	/// afterwards, it may happen that the relay header #200 may be relayed instead - either
	/// by us (e.g. if GRANDPA justification is generated for #200, or if we are only syncing
	/// mandatory headers), or by other relayer. Then, instead of parachain header #50 we may
	/// relay parachain header #70.
	///
	/// This cache is especially important, given that we assume that the nodes we're connected
	/// to are not necessarily archive nodes. Then, if current relay chain block is #210 and #200
	/// has been delivered to the target chain, we have more chances to generate storage proof
	/// at relay block #200 than on relay block #100, which is most likely has pruned state
	/// already.
	pub headers_map_cache: &'a mut BTreeMap<SourceRelayBlock, SourceParaBlock>,
}

/// Read required data from source and target clients.
async fn read_relay_data<'a, P: SubstrateParachainsPipeline>(
	source: &ParachainsSource<P>,
	target: &ParachainsTarget<P>,
	required_header_number: BlockNumberOf<P::SourceParachain>,
	headers_map_cache: &'a mut BTreeMap<
		BlockNumberOf<P::SourceRelayChain>,
		BlockNumberOf<P::SourceParachain>,
	>,
) -> Result<
	RelayData<'a, BlockNumberOf<P::SourceParachain>, BlockNumberOf<P::SourceRelayChain>>,
	FailedClient,
>
where
	ParachainsTarget<P>:
		TargetClient<ParachainsPipelineAdapter<P>> + RelayClient<Error = SubstrateError>,
{
	let map_target_err = |e| {
		log::error!(
			target: "bridge",
			"Failed to read {} relay data from {} client: {:?}",
			on_demand_parachains_relay_name::<P::SourceParachain, P::TargetChain>(),
			P::TargetChain::NAME,
			e,
		);
		FailedClient::Target
	};
	let map_source_err = |e| {
		log::error!(
			target: "bridge",
			"Failed to read {} relay data from {} client: {:?}",
			on_demand_parachains_relay_name::<P::SourceParachain, P::TargetChain>(),
			P::SourceRelayChain::NAME,
			e,
		);
		FailedClient::Source
	};

	let best_target_block_hash = target.best_block().await.map_err(map_target_err)?.1;
	let para_header_at_target =
		best_finalized_peer_header_at_self::<P::TargetChain, P::SourceParachain>(
			target.client(),
			best_target_block_hash,
			P::SourceParachain::BEST_FINALIZED_HEADER_ID_METHOD,
		)
		.await
		.map_err(map_target_err)?
		.0;

	let best_finalized_relay_header =
		source.client().best_finalized_header().await.map_err(map_source_err)?;
	let best_finalized_relay_block_id =
		HeaderId(*best_finalized_relay_header.number(), best_finalized_relay_header.hash());
	let para_header_at_source = source
		.on_chain_parachain_header(
			best_finalized_relay_block_id,
			P::SOURCE_PARACHAIN_PARA_ID.into(),
		)
		.await
		.map_err(map_source_err)?
		.map(|h| *h.number());

	let relay_header_at_source = best_finalized_relay_block_id.0;
	let relay_header_at_target =
		best_finalized_peer_header_at_self::<P::TargetChain, P::SourceRelayChain>(
			target.client(),
			best_target_block_hash,
			P::SourceRelayChain::BEST_FINALIZED_HEADER_ID_METHOD,
		)
		.await
		.map_err(map_target_err)?
		.0;

	Ok(RelayData {
		required_para_header: required_header_number,
		para_header_at_target,
		para_header_at_source,
		relay_header_at_source,
		relay_header_at_target,
		headers_map_cache,
	})
}

// This number is bigger than the session length of any well-known Substrate-based relay
// chain. We expect that the underlying on-demand relay will submit at least 1 header per
// session.
const MAX_HEADERS_MAP_CACHE_ENTRIES: usize = 4096;

/// Select relay and parachain headers that need to be relayed.
fn select_headers_to_relay<'a, SourceParaBlock, SourceRelayBlock>(
	data: &mut RelayData<'a, SourceParaBlock, SourceRelayBlock>,
	mut state: RelayState<SourceParaBlock, SourceRelayBlock>,
) -> RelayState<SourceParaBlock, SourceRelayBlock>
where
	RelayData<'a, SourceParaBlock, SourceRelayBlock>: std::fmt::Debug, // TODO: remove
	SourceParaBlock: Copy + PartialOrd,
	SourceRelayBlock: Copy + Ord,
{
	// despite of our current state, we want to update the headers map cache
	if let Some(para_header_at_source) = data.para_header_at_source {
		data.headers_map_cache
			.insert(data.relay_header_at_source, para_header_at_source);
		if data.headers_map_cache.len() > MAX_HEADERS_MAP_CACHE_ENTRIES {
			let first_key = *data.headers_map_cache.keys().next().expect("map is not empty; qed");
			data.headers_map_cache.remove(&first_key);
		}
	}

	// this switch is responsible for processing `RelayingRelayHeader` state
	match state {
		RelayState::Idle | RelayState::RelayingParaHeader(_) => (),
		RelayState::RelayingRelayHeader(relay_header_number, para_header_number) => {
			match data.relay_header_at_target.cmp(&relay_header_number) {
				Ordering::Less => {
					// relay header hasn't yet been relayed
					return RelayState::RelayingRelayHeader(relay_header_number, para_header_number)
				},
				Ordering::Equal => {
					// relay header has been realyed and we may continue with parachain header
					state = RelayState::RelayingParaHeader(para_header_number);
				},
				Ordering::Greater => {
					// relay header descendant has been relayed and we may need to change parachain
					// header that we want to relay
					let next_para_header_number = data
						.headers_map_cache
						.range(..=data.relay_header_at_target)
						.next_back()
						.map(|(_, next_para_header_number)| *next_para_header_number)
						.unwrap_or_else(|| para_header_number);
					state = RelayState::RelayingParaHeader(next_para_header_number);
				},
			}
		},
	}

	// this switch is responsible for processing `RelayingParaHeader` state
	match state {
		RelayState::Idle => (),
		RelayState::RelayingRelayHeader(_, _) => unreachable!("processed by previous match; qed"),
		RelayState::RelayingParaHeader(para_header_number) => {
			if data.para_header_at_target < para_header_number {
				// parachain header hasn't yet been relayed
				return RelayState::RelayingParaHeader(para_header_number)
			}
		},
	}

	// if we have already satisfied our "customer", do nothing
	if data.required_para_header <= data.para_header_at_target {
		return RelayState::Idle
	}

	// if required header is not available even at the source chain, let's wait
	if Some(data.required_para_header) > data.para_header_at_source {
		return RelayState::Idle
	}

	// we will always try to sync latest parachain/relay header, even if we've been asked for some
	// its ancestor

	// we need relay chain header first
	if data.relay_header_at_target < data.relay_header_at_source {
		return RelayState::RelayingRelayHeader(
			data.relay_header_at_source,
			data.required_para_header,
		)
	}

	// if all relay headers synced, we may start directly with parachain header
	RelayState::RelayingParaHeader(data.required_para_header)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn relay_waits_for_relay_header_to_be_delivered() {
		assert_eq!(
			select_headers_to_relay(
				&mut RelayData {
					required_para_header: 100,
					para_header_at_target: 50,
					para_header_at_source: Some(110),
					relay_header_at_source: 800,
					relay_header_at_target: 700,
					headers_map_cache: &mut BTreeMap::new(),
				},
				RelayState::RelayingRelayHeader(750, 100),
			),
			RelayState::RelayingRelayHeader(750, 100),
		);
	}

	#[test]
	fn relay_starts_relaying_requested_para_header_after_relay_header_is_delivered() {
		assert_eq!(
			select_headers_to_relay(
				&mut RelayData {
					required_para_header: 100,
					para_header_at_target: 50,
					para_header_at_source: Some(110),
					relay_header_at_source: 800,
					relay_header_at_target: 750,
					headers_map_cache: &mut BTreeMap::new(),
				},
				RelayState::RelayingRelayHeader(750, 100),
			),
			RelayState::RelayingParaHeader(100),
		);
	}

	#[test]
	fn relay_selects_same_para_header_after_better_relay_header_is_delivered_1() {
		assert_eq!(
			select_headers_to_relay(
				&mut RelayData {
					required_para_header: 100,
					para_header_at_target: 50,
					para_header_at_source: Some(110),
					relay_header_at_source: 800,
					relay_header_at_target: 780,
					headers_map_cache: &mut vec![(700, 90), (750, 100)].into_iter().collect(),
				},
				RelayState::RelayingRelayHeader(750, 100),
			),
			RelayState::RelayingParaHeader(100),
		);
	}

	#[test]
	fn relay_selects_same_para_header_after_better_relay_header_is_delivered_2() {
		assert_eq!(
			select_headers_to_relay(
				&mut RelayData {
					required_para_header: 100,
					para_header_at_target: 50,
					para_header_at_source: Some(110),
					relay_header_at_source: 800,
					relay_header_at_target: 780,
					headers_map_cache: &mut BTreeMap::new(),
				},
				RelayState::RelayingRelayHeader(750, 100),
			),
			RelayState::RelayingParaHeader(100),
		);
	}

	#[test]
	fn relay_selects_better_para_header_after_better_relay_header_is_delivered() {
		assert_eq!(
			select_headers_to_relay(
				&mut RelayData {
					required_para_header: 100,
					para_header_at_target: 50,
					para_header_at_source: Some(120),
					relay_header_at_source: 800,
					relay_header_at_target: 780,
					headers_map_cache: &mut vec![(700, 90), (750, 100), (780, 110), (790, 120)]
						.into_iter()
						.collect(),
				},
				RelayState::RelayingRelayHeader(750, 100),
			),
			RelayState::RelayingParaHeader(110),
		);
	}

	#[test]
	fn relay_waits_for_para_header_to_be_delivered() {
		assert_eq!(
			select_headers_to_relay(
				&mut RelayData {
					required_para_header: 100,
					para_header_at_target: 50,
					para_header_at_source: Some(110),
					relay_header_at_source: 800,
					relay_header_at_target: 700,
					headers_map_cache: &mut BTreeMap::new(),
				},
				RelayState::RelayingParaHeader(100),
			),
			RelayState::RelayingParaHeader(100),
		);
	}

	#[test]
	fn relay_stays_idle_if_required_para_header_is_already_delivered() {
		assert_eq!(
			select_headers_to_relay(
				&mut RelayData {
					required_para_header: 100,
					para_header_at_target: 100,
					para_header_at_source: Some(110),
					relay_header_at_source: 800,
					relay_header_at_target: 700,
					headers_map_cache: &mut BTreeMap::new(),
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
				&mut RelayData {
					required_para_header: 110,
					para_header_at_target: 100,
					para_header_at_source: None,
					relay_header_at_source: 800,
					relay_header_at_target: 700,
					headers_map_cache: &mut BTreeMap::new(),
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
				&mut RelayData {
					required_para_header: 110,
					para_header_at_target: 100,
					para_header_at_source: Some(100),
					relay_header_at_source: 800,
					relay_header_at_target: 700,
					headers_map_cache: &mut BTreeMap::new(),
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
				&mut RelayData {
					required_para_header: 110,
					para_header_at_target: 100,
					para_header_at_source: Some(110),
					relay_header_at_source: 800,
					relay_header_at_target: 700,
					headers_map_cache: &mut BTreeMap::new(),
				},
				RelayState::Idle,
			),
			RelayState::RelayingRelayHeader(800, 110),
		);
	}

	#[test]
	fn relay_starts_relaying_para_header_when_new_para_header_is_requested() {
		assert_eq!(
			select_headers_to_relay(
				&mut RelayData {
					required_para_header: 110,
					para_header_at_target: 100,
					para_header_at_source: Some(110),
					relay_header_at_source: 800,
					relay_header_at_target: 800,
					headers_map_cache: &mut BTreeMap::new(),
				},
				RelayState::Idle,
			),
			RelayState::RelayingParaHeader(110),
		);
	}

	#[test]
	fn headers_map_cache_is_updated() {
		let mut headers_map_cache = BTreeMap::new();

		// when parachain header is known, map is updated
		select_headers_to_relay(
			&mut RelayData {
				required_para_header: 0,
				para_header_at_target: 50,
				para_header_at_source: Some(110),
				relay_header_at_source: 800,
				relay_header_at_target: 700,
				headers_map_cache: &mut headers_map_cache,
			},
			RelayState::RelayingRelayHeader(750, 100),
		);
		assert_eq!(headers_map_cache.clone().into_iter().collect::<Vec<_>>(), vec![(800, 110)],);

		// when parachain header is not known, map is NOT updated
		select_headers_to_relay(
			&mut RelayData {
				required_para_header: 0,
				para_header_at_target: 50,
				para_header_at_source: None,
				relay_header_at_source: 800,
				relay_header_at_target: 700,
				headers_map_cache: &mut headers_map_cache,
			},
			RelayState::RelayingRelayHeader(750, 100),
		);
		assert_eq!(headers_map_cache.clone().into_iter().collect::<Vec<_>>(), vec![(800, 110)],);

		// map auto-deduplicates equal entries
		select_headers_to_relay(
			&mut RelayData {
				required_para_header: 0,
				para_header_at_target: 50,
				para_header_at_source: Some(110),
				relay_header_at_source: 800,
				relay_header_at_target: 700,
				headers_map_cache: &mut headers_map_cache,
			},
			RelayState::RelayingRelayHeader(750, 100),
		);
		assert_eq!(headers_map_cache.clone().into_iter().collect::<Vec<_>>(), vec![(800, 110)],);

		// nothing is pruned if number of map entries is < MAX_HEADERS_MAP_CACHE_ENTRIES
		for i in 1..MAX_HEADERS_MAP_CACHE_ENTRIES {
			select_headers_to_relay(
				&mut RelayData {
					required_para_header: 0,
					para_header_at_target: 50,
					para_header_at_source: Some(110 + i),
					relay_header_at_source: 800 + i,
					relay_header_at_target: 700,
					headers_map_cache: &mut headers_map_cache,
				},
				RelayState::RelayingRelayHeader(750, 100),
			);
			assert_eq!(headers_map_cache.len(), i + 1);
		}

		// when we add next entry, the oldest one is pruned
		assert!(headers_map_cache.contains_key(&800));
		assert_eq!(headers_map_cache.len(), MAX_HEADERS_MAP_CACHE_ENTRIES);
		select_headers_to_relay(
			&mut RelayData {
				required_para_header: 0,
				para_header_at_target: 50,
				para_header_at_source: Some(110 + MAX_HEADERS_MAP_CACHE_ENTRIES),
				relay_header_at_source: 800 + MAX_HEADERS_MAP_CACHE_ENTRIES,
				relay_header_at_target: 700,
				headers_map_cache: &mut headers_map_cache,
			},
			RelayState::RelayingRelayHeader(750, 100),
		);
		assert!(!headers_map_cache.contains_key(&800));
		assert_eq!(headers_map_cache.len(), MAX_HEADERS_MAP_CACHE_ENTRIES);
	}
}
