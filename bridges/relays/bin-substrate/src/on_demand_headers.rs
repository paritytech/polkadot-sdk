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

//! On-demand Substrate -> Substrate headers relay.

use crate::finality_pipeline::{
	SubstrateFinalitySyncPipeline, SubstrateFinalityToSubstrate, RECENT_FINALITY_PROOFS_LIMIT, STALL_TIMEOUT,
};
use crate::finality_target::SubstrateFinalityTarget;

use async_std::sync::{Arc, Mutex};
use bp_header_chain::justification::GrandpaJustification;
use finality_relay::{
	FinalitySyncParams, FinalitySyncPipeline, SourceClient as FinalitySourceClient,
	TargetClient as FinalityTargetClient,
};
use futures::{select, FutureExt};
use num_traits::{CheckedSub, Zero};
use relay_substrate_client::{
	finality_source::{FinalitySource as SubstrateFinalitySource, RequiredHeaderNumberRef},
	BlockNumberOf, Chain, Client, HashOf, HeaderIdOf, SyncHeader,
};
use relay_utils::{
	metrics::MetricsParams, relay_loop::Client as RelayClient, BlockNumberBase, FailedClient, MaybeConnectionError,
};
use std::fmt::Debug;

/// On-demand Substrate <-> Substrate headers relay.
///
/// This relay may be requested to sync more headers, whenever some other relay (e.g. messages relay) needs
/// it to continue its regular work. When enough headers are relayed, on-demand stops syncing headers.
#[derive(Clone)]
pub struct OnDemandHeadersRelay<SourceChain: Chain> {
	/// Relay task name.
	relay_task_name: String,
	/// Shared reference to maximal required finalized header number.
	required_header_number: RequiredHeaderNumberRef<SourceChain>,
}

impl<SourceChain: Chain> OnDemandHeadersRelay<SourceChain> {
	/// Create new on-demand headers relay.
	pub fn new<TargetChain: Chain, TargetSign>(
		source_client: Client<SourceChain>,
		target_client: Client<TargetChain>,
		pipeline: SubstrateFinalityToSubstrate<SourceChain, TargetChain, TargetSign>,
		maximal_headers_difference: SourceChain::BlockNumber,
	) -> Self
	where
		SourceChain: Chain + Debug,
		SourceChain::BlockNumber: BlockNumberBase,
		TargetChain: Chain + Debug,
		TargetChain::BlockNumber: BlockNumberBase,
		TargetSign: Clone + Send + Sync + 'static,
		SubstrateFinalityToSubstrate<SourceChain, TargetChain, TargetSign>: SubstrateFinalitySyncPipeline<
			Hash = HashOf<SourceChain>,
			Number = BlockNumberOf<SourceChain>,
			Header = SyncHeader<SourceChain::Header>,
			FinalityProof = GrandpaJustification<SourceChain::Header>,
			TargetChain = TargetChain,
		>,
		SubstrateFinalityTarget<TargetChain, SubstrateFinalityToSubstrate<SourceChain, TargetChain, TargetSign>>:
			FinalityTargetClient<SubstrateFinalityToSubstrate<SourceChain, TargetChain, TargetSign>>,
	{
		let required_header_number = Arc::new(Mutex::new(Zero::zero()));
		let this = OnDemandHeadersRelay {
			relay_task_name: on_demand_headers_relay_name::<SourceChain, TargetChain>(),
			required_header_number: required_header_number.clone(),
		};
		async_std::task::spawn(async move {
			background_task(
				source_client,
				target_client,
				pipeline,
				maximal_headers_difference,
				required_header_number,
			)
			.await;
		});

		this
	}

	/// Someone is asking us to relay given finalized header.
	pub async fn require_finalized_header(&self, header_id: HeaderIdOf<SourceChain>) {
		let mut required_header_number = self.required_header_number.lock().await;
		if header_id.0 > *required_header_number {
			log::trace!(
				target: "bridge",
				"More {} headers required in {} relay. Going to sync up to the {}",
				SourceChain::NAME,
				self.relay_task_name,
				header_id.0,
			);

			*required_header_number = header_id.0;
		}
	}
}

/// Background task that is responsible for starting headers relay.
async fn background_task<SourceChain, TargetChain, TargetSign>(
	source_client: Client<SourceChain>,
	target_client: Client<TargetChain>,
	pipeline: SubstrateFinalityToSubstrate<SourceChain, TargetChain, TargetSign>,
	maximal_headers_difference: SourceChain::BlockNumber,
	required_header_number: RequiredHeaderNumberRef<SourceChain>,
) where
	SourceChain: Chain + Debug,
	SourceChain::BlockNumber: BlockNumberBase,
	TargetChain: Chain + Debug,
	TargetChain::BlockNumber: BlockNumberBase,
	TargetSign: Clone + Send + Sync + 'static,
	SubstrateFinalityToSubstrate<SourceChain, TargetChain, TargetSign>: SubstrateFinalitySyncPipeline<
		Hash = HashOf<SourceChain>,
		Number = BlockNumberOf<SourceChain>,
		Header = SyncHeader<SourceChain::Header>,
		FinalityProof = GrandpaJustification<SourceChain::Header>,
		TargetChain = TargetChain,
	>,
	SubstrateFinalityTarget<TargetChain, SubstrateFinalityToSubstrate<SourceChain, TargetChain, TargetSign>>:
		FinalityTargetClient<SubstrateFinalityToSubstrate<SourceChain, TargetChain, TargetSign>>,
{
	let relay_task_name = on_demand_headers_relay_name::<SourceChain, TargetChain>();
	let mut finality_source = SubstrateFinalitySource::<
		_,
		SubstrateFinalityToSubstrate<SourceChain, TargetChain, TargetSign>,
	>::new(source_client.clone(), Some(required_header_number.clone()));
	let mut finality_target = SubstrateFinalityTarget::new(target_client.clone(), pipeline.clone());

	let mut restart_relay = true;
	let finality_relay_task = futures::future::Fuse::terminated();
	futures::pin_mut!(finality_relay_task);

	loop {
		select! {
			_ = async_std::task::sleep(TargetChain::AVERAGE_BLOCK_INTERVAL).fuse() => {},
			_ = finality_relay_task => {
				// this should never happen in practice given the current code
				restart_relay = true;
			},
		}

		// read best finalized source header number from source
		let best_finalized_source_header_at_source =
			best_finalized_source_header_at_source(&finality_source, &relay_task_name).await;
		if matches!(best_finalized_source_header_at_source, Err(ref e) if e.is_connection_error()) {
			relay_utils::relay_loop::reconnect_failed_client(
				FailedClient::Source,
				relay_utils::relay_loop::RECONNECT_DELAY,
				&mut finality_source,
				&mut finality_target,
			)
			.await;
			continue;
		}

		// read best finalized source header number from target
		let best_finalized_source_header_at_target =
			best_finalized_source_header_at_target::<SourceChain, _, _>(&finality_target, &relay_task_name).await;
		if matches!(best_finalized_source_header_at_target, Err(ref e) if e.is_connection_error()) {
			relay_utils::relay_loop::reconnect_failed_client(
				FailedClient::Target,
				relay_utils::relay_loop::RECONNECT_DELAY,
				&mut finality_source,
				&mut finality_target,
			)
			.await;
			continue;
		}

		// update required header
		update_required_header_number_if_too_many_are_missing::<SourceChain>(
			best_finalized_source_header_at_source.ok(),
			best_finalized_source_header_at_target.ok(),
			maximal_headers_difference,
			&required_header_number,
			&relay_task_name,
		)
		.await;

		// start/restart relay
		if restart_relay {
			finality_relay_task.set(
				finality_relay::run(
					finality_source.clone(),
					finality_target.clone(),
					FinalitySyncParams {
						tick: std::cmp::max(SourceChain::AVERAGE_BLOCK_INTERVAL, TargetChain::AVERAGE_BLOCK_INTERVAL),
						recent_finality_proofs_limit: RECENT_FINALITY_PROOFS_LIMIT,
						stall_timeout: STALL_TIMEOUT,
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

/// If there are too many source headers missing at target, we ask for syncing more headers.
async fn update_required_header_number_if_too_many_are_missing<C: Chain>(
	best_finalized_source_header_at_source: Option<C::BlockNumber>,
	best_finalized_source_header_at_target: Option<C::BlockNumber>,
	maximal_headers_difference: C::BlockNumber,
	required_header_number: &RequiredHeaderNumberRef<C>,
	relay_task_name: &str,
) {
	let mut required_header_number = required_header_number.lock().await;

	// if we have been unable to read header number from the target, then let's assume
	// that it is the same as required header number. Otherwise we risk submitting
	// unneeded transactions
	let best_finalized_source_header_at_target =
		best_finalized_source_header_at_target.unwrap_or(*required_header_number);

	// if we have been unable to read header number from the source, then let's assume
	// that it is the same as at the target
	let best_finalized_source_header_at_source =
		best_finalized_source_header_at_source.unwrap_or(best_finalized_source_header_at_target);

	// if there are too many source headers missing from the target node, require some
	// new headers at target
	//
	// why do we need that? When complex headers+messages relay is used, it'll normally only relay
	// headers when there are undelivered messages/confirmations. But security model of the
	// `pallet-bridge-grandpa` module relies on the fact that headers are synced in real-time and
	// that it'll see authorities-change header before unbonding period will end for previous
	// authorities set.
	let current_headers_difference = best_finalized_source_header_at_source
		.checked_sub(&best_finalized_source_header_at_target)
		.unwrap_or_else(Zero::zero);
	if current_headers_difference > maximal_headers_difference {
		// if relay is already asked to sync headers, don't log anything
		if *required_header_number <= best_finalized_source_header_at_target {
			log::trace!(
				target: "bridge",
				"Too many {} headers missing at target in {} relay ({} vs {}). Going to sync up to the {}",
				C::NAME,
				relay_task_name,
				best_finalized_source_header_at_source,
				best_finalized_source_header_at_target,
				best_finalized_source_header_at_source,
			);

			*required_header_number = best_finalized_source_header_at_source;
		}
	}
}

/// Read best finalized source block number from source client.
///
/// Returns `None` if we have failed to read the number.
async fn best_finalized_source_header_at_source<SourceChain: Chain, P>(
	finality_source: &SubstrateFinalitySource<SourceChain, P>,
	relay_task_name: &str,
) -> Result<SourceChain::BlockNumber, relay_substrate_client::Error>
where
	SubstrateFinalitySource<SourceChain, P>: FinalitySourceClient<P>,
	P: FinalitySyncPipeline<Number = SourceChain::BlockNumber>,
{
	finality_source
		.on_chain_best_finalized_block_number()
		.await
		.map_err(|error| {
			log::error!(
				target: "bridge",
				"Failed to read best finalized source header from source in {} relay: {:?}",
				relay_task_name,
				error,
			);

			error
		})
}

/// Read best finalized source block number from target client.
///
/// Returns `None` if we have failed to read the number.
async fn best_finalized_source_header_at_target<SourceChain: Chain, TargetChain: Chain, P>(
	finality_target: &SubstrateFinalityTarget<TargetChain, P>,
	relay_task_name: &str,
) -> Result<SourceChain::BlockNumber, <SubstrateFinalityTarget<TargetChain, P> as RelayClient>::Error>
where
	SubstrateFinalityTarget<TargetChain, P>: FinalityTargetClient<P>,
	P: FinalitySyncPipeline<Number = SourceChain::BlockNumber>,
{
	finality_target
		.best_finalized_source_block_number()
		.await
		.map_err(|error| {
			log::error!(
				target: "bridge",
				"Failed to read best finalized source header from target in {} relay: {:?}",
				relay_task_name,
				error,
			);

			error
		})
}

/// On-demand headers relay task name.
fn on_demand_headers_relay_name<SourceChain: Chain, TargetChain: Chain>() -> String {
	format!("on-demand-{}-to-{}", SourceChain::NAME, TargetChain::NAME)
}

#[cfg(test)]
mod tests {
	use super::*;

	type TestChain = relay_millau_client::Millau;

	const AT_SOURCE: Option<bp_millau::BlockNumber> = Some(10);
	const AT_TARGET: Option<bp_millau::BlockNumber> = Some(1);

	#[async_std::test]
	async fn updates_required_header_when_too_many_headers_missing() {
		let required_header_number = Arc::new(Mutex::new(0));
		update_required_header_number_if_too_many_are_missing::<TestChain>(
			AT_SOURCE,
			AT_TARGET,
			5,
			&required_header_number,
			"test",
		)
		.await;
		assert_eq!(*required_header_number.lock().await, AT_SOURCE.unwrap());
	}
}
