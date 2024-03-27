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

use crate::{parachains_loop_metrics::ParachainsLoopMetrics, ParachainsPipeline};

use async_trait::async_trait;
use bp_polkadot_core::{
	parachains::{ParaHash, ParaHeadsProof, ParaId},
	BlockNumber as RelayBlockNumber,
};
use futures::{
	future::{FutureExt, Shared},
	poll, select_biased,
};
use relay_substrate_client::{Chain, HeaderIdOf, ParachainBase};
use relay_utils::{
	metrics::MetricsParams, relay_loop::Client as RelayClient, FailedClient,
	TrackedTransactionStatus, TransactionTracker,
};
use std::{future::Future, pin::Pin, task::Poll};

/// Parachain header availability at a certain chain.
#[derive(Clone, Copy, Debug)]
pub enum AvailableHeader<T> {
	/// The client can not report actual parachain head at this moment.
	///
	/// It is a "mild" error, which may appear when e.g. on-demand parachains relay is used.
	/// This variant must be treated as "we don't want to update parachain head value at the
	/// target chain at this moment".
	Unavailable,
	/// There's no parachain header at the relay chain.
	///
	/// Normally it means that the parachain is not registered there.
	Missing,
	/// Parachain head with given hash is available at the source chain.
	Available(T),
}

impl<T> AvailableHeader<T> {
	/// Return available header.
	pub fn as_available(&self) -> Option<&T> {
		match *self {
			AvailableHeader::Available(ref header) => Some(header),
			_ => None,
		}
	}
}

impl<T> From<Option<T>> for AvailableHeader<T> {
	fn from(maybe_header: Option<T>) -> AvailableHeader<T> {
		match maybe_header {
			Some(header) => AvailableHeader::Available(header),
			None => AvailableHeader::Missing,
		}
	}
}

/// Source client used in parachain heads synchronization loop.
#[async_trait]
pub trait SourceClient<P: ParachainsPipeline>: RelayClient {
	/// Returns `Ok(true)` if client is in synced state.
	async fn ensure_synced(&self) -> Result<bool, Self::Error>;

	/// Get parachain head id at given block.
	async fn parachain_head(
		&self,
		at_block: HeaderIdOf<P::SourceRelayChain>,
	) -> Result<AvailableHeader<HeaderIdOf<P::SourceParachain>>, Self::Error>;

	/// Get parachain head proof at given block.
	async fn prove_parachain_head(
		&self,
		at_block: HeaderIdOf<P::SourceRelayChain>,
	) -> Result<(ParaHeadsProof, ParaHash), Self::Error>;
}

/// Target client used in parachain heads synchronization loop.
#[async_trait]
pub trait TargetClient<P: ParachainsPipeline>: RelayClient {
	/// Transaction tracker to track submitted transactions.
	type TransactionTracker: TransactionTracker<HeaderId = HeaderIdOf<P::TargetChain>>;

	/// Get best block id.
	async fn best_block(&self) -> Result<HeaderIdOf<P::TargetChain>, Self::Error>;

	/// Get best finalized source relay chain block id.
	async fn best_finalized_source_relay_chain_block(
		&self,
		at_block: &HeaderIdOf<P::TargetChain>,
	) -> Result<HeaderIdOf<P::SourceRelayChain>, Self::Error>;

	/// Get parachain head id at given block.
	async fn parachain_head(
		&self,
		at_block: HeaderIdOf<P::TargetChain>,
	) -> Result<Option<HeaderIdOf<P::SourceParachain>>, Self::Error>;

	/// Submit parachain heads proof.
	async fn submit_parachain_head_proof(
		&self,
		at_source_block: HeaderIdOf<P::SourceRelayChain>,
		para_head_hash: ParaHash,
		proof: ParaHeadsProof,
	) -> Result<Self::TransactionTracker, Self::Error>;
}

/// Return prefix that will be used by default to expose Prometheus metrics of the parachains
/// sync loop.
pub fn metrics_prefix<P: ParachainsPipeline>() -> String {
	format!(
		"{}_to_{}_Parachains_{}",
		P::SourceRelayChain::NAME,
		P::TargetChain::NAME,
		P::SourceParachain::PARACHAIN_ID
	)
}

/// Run parachain heads synchronization.
pub async fn run<P: ParachainsPipeline>(
	source_client: impl SourceClient<P>,
	target_client: impl TargetClient<P>,
	metrics_params: MetricsParams,
	exit_signal: impl Future<Output = ()> + 'static + Send,
) -> Result<(), relay_utils::Error>
where
	P::SourceRelayChain: Chain<BlockNumber = RelayBlockNumber>,
{
	let exit_signal = exit_signal.shared();
	relay_utils::relay_loop(source_client, target_client)
		.with_metrics(metrics_params)
		.loop_metric(ParachainsLoopMetrics::new(Some(&metrics_prefix::<P>()))?)?
		.expose()
		.await?
		.run(metrics_prefix::<P>(), move |source_client, target_client, metrics| {
			run_until_connection_lost(source_client, target_client, metrics, exit_signal.clone())
		})
		.await
}

/// Run parachain heads synchronization.
async fn run_until_connection_lost<P: ParachainsPipeline>(
	source_client: impl SourceClient<P>,
	target_client: impl TargetClient<P>,
	metrics: Option<ParachainsLoopMetrics>,
	exit_signal: impl Future<Output = ()> + Send,
) -> Result<(), FailedClient>
where
	P::SourceRelayChain: Chain<BlockNumber = RelayBlockNumber>,
{
	let exit_signal = exit_signal.fuse();
	let min_block_interval = std::cmp::min(
		P::SourceRelayChain::AVERAGE_BLOCK_INTERVAL,
		P::TargetChain::AVERAGE_BLOCK_INTERVAL,
	);

	let mut submitted_heads_tracker: Option<SubmittedHeadsTracker<P>> = None;

	futures::pin_mut!(exit_signal);

	// Note that the internal loop breaks with `FailedClient` error even if error is non-connection.
	// It is Ok for now, but it may need to be fixed in the future to use exponential backoff for
	// regular errors.

	loop {
		// Either wait for new block, or exit signal.
		// Please note that we are prioritizing the exit signal since if both events happen at once
		// it doesn't make sense to perform one more loop iteration.
		select_biased! {
			_ = exit_signal => return Ok(()),
			_ = async_std::task::sleep(min_block_interval).fuse() => {},
		}

		// if source client is not yet synced, we'll need to sleep. Otherwise we risk submitting too
		// much redundant transactions
		match source_client.ensure_synced().await {
			Ok(true) => (),
			Ok(false) => {
				log::warn!(
					target: "bridge",
					"{} client is syncing. Won't do anything until it is synced",
					P::SourceRelayChain::NAME,
				);
				continue
			},
			Err(e) => {
				log::warn!(
					target: "bridge",
					"{} client has failed to return its sync status: {:?}",
					P::SourceRelayChain::NAME,
					e,
				);
				return Err(FailedClient::Source)
			},
		}

		// if we have active transaction, we'll need to wait until it is mined or dropped
		let best_target_block = target_client.best_block().await.map_err(|e| {
			log::warn!(target: "bridge", "Failed to read best {} block: {:?}", P::SourceRelayChain::NAME, e);
			FailedClient::Target
		})?;
		let head_at_target =
			read_head_at_target(&target_client, metrics.as_ref(), &best_target_block).await?;

		// check if our transaction has been mined
		if let Some(tracker) = submitted_heads_tracker.take() {
			match tracker.update(&best_target_block, &head_at_target).await {
				SubmittedHeadStatus::Waiting(tracker) => {
					// no news about our transaction and we shall keep waiting
					submitted_heads_tracker = Some(tracker);
					continue
				},
				SubmittedHeadStatus::Final(TrackedTransactionStatus::Finalized(_)) => {
					// all heads have been updated, we don't need this tracker anymore
				},
				SubmittedHeadStatus::Final(TrackedTransactionStatus::Lost) => {
					log::warn!(
						target: "bridge",
						"Parachains synchronization from {} to {} has stalled. Going to restart",
						P::SourceRelayChain::NAME,
						P::TargetChain::NAME,
					);

					return Err(FailedClient::Both)
				},
			}
		}

		// we have no active transaction and may need to update heads, but do we have something for
		// update?
		let best_finalized_relay_block = target_client
			.best_finalized_source_relay_chain_block(&best_target_block)
			.await
			.map_err(|e| {
				log::warn!(
					target: "bridge",
					"Failed to read best finalized {} block from {}: {:?}",
					P::SourceRelayChain::NAME,
					P::TargetChain::NAME,
					e,
				);
				FailedClient::Target
			})?;
		let head_at_source =
			read_head_at_source(&source_client, metrics.as_ref(), &best_finalized_relay_block)
				.await?;
		let is_update_required = is_update_required::<P>(
			head_at_source,
			head_at_target,
			best_finalized_relay_block,
			best_target_block,
		);

		if is_update_required {
			let (head_proof, head_hash) = source_client
				.prove_parachain_head(best_finalized_relay_block)
				.await
				.map_err(|e| {
					log::warn!(
						target: "bridge",
						"Failed to prove {} parachain ParaId({}) heads: {:?}",
						P::SourceRelayChain::NAME,
						P::SourceParachain::PARACHAIN_ID,
						e,
					);
					FailedClient::Source
				})?;
			log::info!(
				target: "bridge",
				"Submitting {} parachain ParaId({}) head update transaction to {}. Para hash at source relay {:?}: {:?}",
				P::SourceRelayChain::NAME,
				P::SourceParachain::PARACHAIN_ID,
				P::TargetChain::NAME,
				best_finalized_relay_block,
				head_hash,
			);

			let transaction_tracker = target_client
				.submit_parachain_head_proof(best_finalized_relay_block, head_hash, head_proof)
				.await
				.map_err(|e| {
					log::warn!(
						target: "bridge",
						"Failed to submit {} parachain ParaId({}) heads proof to {}: {:?}",
						P::SourceRelayChain::NAME,
						P::SourceParachain::PARACHAIN_ID,
						P::TargetChain::NAME,
						e,
					);
					FailedClient::Target
				})?;
			submitted_heads_tracker =
				Some(SubmittedHeadsTracker::<P>::new(head_at_source, transaction_tracker));
		}
	}
}

/// Returns `true` if we need to submit parachain-head-update transaction.
fn is_update_required<P: ParachainsPipeline>(
	head_at_source: AvailableHeader<HeaderIdOf<P::SourceParachain>>,
	head_at_target: Option<HeaderIdOf<P::SourceParachain>>,
	best_finalized_relay_block_at_source: HeaderIdOf<P::SourceRelayChain>,
	best_target_block: HeaderIdOf<P::TargetChain>,
) -> bool
where
	P::SourceRelayChain: Chain<BlockNumber = RelayBlockNumber>,
{
	log::trace!(
		target: "bridge",
		"Checking if {} parachain ParaId({}) needs update at {}:\n\t\
			At {} ({:?}): {:?}\n\t\
			At {} ({:?}): {:?}",
		P::SourceRelayChain::NAME,
		P::SourceParachain::PARACHAIN_ID,
		P::TargetChain::NAME,
		P::SourceRelayChain::NAME,
		best_finalized_relay_block_at_source,
		head_at_source,
		P::TargetChain::NAME,
		best_target_block,
		head_at_target,
	);

	let needs_update = match (head_at_source, head_at_target) {
		(AvailableHeader::Unavailable, _) => {
			// source client has politely asked us not to update current parachain head
			// at the target chain
			false
		},
		(AvailableHeader::Available(head_at_source), Some(head_at_target))
			if head_at_source.number() > head_at_target.number() =>
		{
			// source client knows head that is better than the head known to the target
			// client
			true
		},
		(AvailableHeader::Available(_), Some(_)) => {
			// this is normal case when relay has recently updated heads, when parachain is
			// not progressing, or when our source client is still syncing
			false
		},
		(AvailableHeader::Available(_), None) => {
			// parachain is not yet known to the target client. This is true when parachain
			// or bridge has been just onboarded/started
			true
		},
		(AvailableHeader::Missing, Some(_)) => {
			// parachain/parathread has been offboarded removed from the system. It needs to
			// be propageted to the target client
			true
		},
		(AvailableHeader::Missing, None) => {
			// all's good - parachain is unknown to both clients
			false
		},
	};

	if needs_update {
		log::trace!(
			target: "bridge",
			"{} parachain ParaId({}) needs update at {}: {:?} vs {:?}",
			P::SourceRelayChain::NAME,
			P::SourceParachain::PARACHAIN_ID,
			P::TargetChain::NAME,
			head_at_source,
			head_at_target,
		);
	}

	needs_update
}

/// Reads parachain head from the source client.
async fn read_head_at_source<P: ParachainsPipeline>(
	source_client: &impl SourceClient<P>,
	metrics: Option<&ParachainsLoopMetrics>,
	at_relay_block: &HeaderIdOf<P::SourceRelayChain>,
) -> Result<AvailableHeader<HeaderIdOf<P::SourceParachain>>, FailedClient> {
	let para_head = source_client.parachain_head(*at_relay_block).await;
	match para_head {
		Ok(AvailableHeader::Available(para_head)) => {
			if let Some(metrics) = metrics {
				metrics.update_best_parachain_block_at_source(
					ParaId(P::SourceParachain::PARACHAIN_ID),
					para_head.number(),
				);
			}
			Ok(AvailableHeader::Available(para_head))
		},
		Ok(r) => Ok(r),
		Err(e) => {
			log::warn!(
				target: "bridge",
				"Failed to read head of {} parachain ParaId({:?}): {:?}",
				P::SourceRelayChain::NAME,
				P::SourceParachain::PARACHAIN_ID,
				e,
			);
			Err(FailedClient::Source)
		},
	}
}

/// Reads parachain head from the target client.
async fn read_head_at_target<P: ParachainsPipeline>(
	target_client: &impl TargetClient<P>,
	metrics: Option<&ParachainsLoopMetrics>,
	at_block: &HeaderIdOf<P::TargetChain>,
) -> Result<Option<HeaderIdOf<P::SourceParachain>>, FailedClient> {
	let para_head_id = target_client.parachain_head(*at_block).await;
	match para_head_id {
		Ok(Some(para_head_id)) => {
			if let Some(metrics) = metrics {
				metrics.update_best_parachain_block_at_target(
					ParaId(P::SourceParachain::PARACHAIN_ID),
					para_head_id.number(),
				);
			}
			Ok(Some(para_head_id))
		},
		Ok(None) => Ok(None),
		Err(e) => {
			log::warn!(
				target: "bridge",
				"Failed to read head of {} parachain ParaId({}) at {}: {:?}",
				P::SourceRelayChain::NAME,
				P::SourceParachain::PARACHAIN_ID,
				P::TargetChain::NAME,
				e,
			);
			Err(FailedClient::Target)
		},
	}
}

/// Submitted heads status.
enum SubmittedHeadStatus<P: ParachainsPipeline> {
	/// Heads are not yet updated.
	Waiting(SubmittedHeadsTracker<P>),
	/// Heads transaction has either been finalized or lost (i.e. received its "final" status).
	Final(TrackedTransactionStatus<HeaderIdOf<P::TargetChain>>),
}

/// Type of the transaction tracker that the `SubmittedHeadsTracker` is using.
///
/// It needs to be shared because of `poll` macro and our consuming `update` method.
type SharedTransactionTracker<P> = Shared<
	Pin<
		Box<
			dyn Future<
					Output = TrackedTransactionStatus<
						HeaderIdOf<<P as ParachainsPipeline>::TargetChain>,
					>,
				> + Send,
		>,
	>,
>;

/// Submitted parachain heads transaction.
struct SubmittedHeadsTracker<P: ParachainsPipeline> {
	/// Parachain header id that we have submitted.
	submitted_head: AvailableHeader<HeaderIdOf<P::SourceParachain>>,
	/// Future that waits for submitted transaction finality or loss.
	///
	/// It needs to be shared because of `poll` macro and our consuming `update` method.
	transaction_tracker: SharedTransactionTracker<P>,
}

impl<P: ParachainsPipeline> SubmittedHeadsTracker<P> {
	/// Creates new parachain heads transaction tracker.
	pub fn new(
		submitted_head: AvailableHeader<HeaderIdOf<P::SourceParachain>>,
		transaction_tracker: impl TransactionTracker<HeaderId = HeaderIdOf<P::TargetChain>> + 'static,
	) -> Self {
		SubmittedHeadsTracker {
			submitted_head,
			transaction_tracker: transaction_tracker.wait().fuse().boxed().shared(),
		}
	}

	/// Returns `None` if all submitted parachain heads have been updated.
	pub async fn update(
		self,
		at_target_block: &HeaderIdOf<P::TargetChain>,
		head_at_target: &Option<HeaderIdOf<P::SourceParachain>>,
	) -> SubmittedHeadStatus<P> {
		// check if our head has been updated
		let is_head_updated = match (self.submitted_head, head_at_target) {
			(AvailableHeader::Available(submitted_head), Some(head_at_target))
				if head_at_target.number() >= submitted_head.number() =>
				true,
			(AvailableHeader::Missing, None) => true,
			_ => false,
		};
		if is_head_updated {
			log::trace!(
				target: "bridge",
				"Head of parachain ParaId({}) has been updated at {}: {:?}",
				P::SourceParachain::PARACHAIN_ID,
				P::TargetChain::NAME,
				head_at_target,
			);

			return SubmittedHeadStatus::Final(TrackedTransactionStatus::Finalized(*at_target_block))
		}

		// if underlying transaction tracker has reported that the transaction is lost, we may
		// then restart our sync
		let transaction_tracker = self.transaction_tracker.clone();
		match poll!(transaction_tracker) {
			Poll::Ready(TrackedTransactionStatus::Lost) =>
				return SubmittedHeadStatus::Final(TrackedTransactionStatus::Lost),
			Poll::Ready(TrackedTransactionStatus::Finalized(_)) => {
				// so we are here and our transaction is mined+finalized, but some of heads were not
				// updated => we're considering our loop as stalled
				return SubmittedHeadStatus::Final(TrackedTransactionStatus::Lost)
			},
			_ => (),
		}

		SubmittedHeadStatus::Waiting(self)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use async_std::sync::{Arc, Mutex};
	use codec::Encode;
	use futures::{SinkExt, StreamExt};
	use relay_substrate_client::test_chain::{TestChain, TestParachain};
	use relay_utils::{HeaderId, MaybeConnectionError};
	use sp_core::H256;

	const PARA_10_HASH: ParaHash = H256([10u8; 32]);
	const PARA_20_HASH: ParaHash = H256([20u8; 32]);

	#[derive(Clone, Debug)]
	enum TestError {
		Error,
	}

	impl MaybeConnectionError for TestError {
		fn is_connection_error(&self) -> bool {
			false
		}
	}

	#[derive(Clone, Debug, PartialEq, Eq)]
	struct TestParachainsPipeline;

	impl ParachainsPipeline for TestParachainsPipeline {
		type SourceRelayChain = TestChain;
		type SourceParachain = TestParachain;
		type TargetChain = TestChain;
	}

	#[derive(Clone, Debug)]
	struct TestClient {
		data: Arc<Mutex<TestClientData>>,
	}

	#[derive(Clone, Debug)]
	struct TestTransactionTracker(Option<TrackedTransactionStatus<HeaderIdOf<TestChain>>>);

	#[async_trait]
	impl TransactionTracker for TestTransactionTracker {
		type HeaderId = HeaderIdOf<TestChain>;

		async fn wait(self) -> TrackedTransactionStatus<HeaderIdOf<TestChain>> {
			match self.0 {
				Some(status) => status,
				None => futures::future::pending().await,
			}
		}
	}

	#[derive(Clone, Debug)]
	struct TestClientData {
		source_sync_status: Result<bool, TestError>,
		source_head: Result<AvailableHeader<HeaderIdOf<TestParachain>>, TestError>,
		source_proof: Result<(), TestError>,

		target_best_block: Result<HeaderIdOf<TestChain>, TestError>,
		target_best_finalized_source_block: Result<HeaderIdOf<TestChain>, TestError>,
		target_head: Result<Option<HeaderIdOf<TestParachain>>, TestError>,
		target_submit_result: Result<(), TestError>,

		exit_signal_sender: Option<Box<futures::channel::mpsc::UnboundedSender<()>>>,
	}

	impl TestClientData {
		pub fn minimal() -> Self {
			TestClientData {
				source_sync_status: Ok(true),
				source_head: Ok(AvailableHeader::Available(HeaderId(0, PARA_20_HASH))),
				source_proof: Ok(()),

				target_best_block: Ok(HeaderId(0, Default::default())),
				target_best_finalized_source_block: Ok(HeaderId(0, Default::default())),
				target_head: Ok(None),
				target_submit_result: Ok(()),

				exit_signal_sender: None,
			}
		}

		pub fn with_exit_signal_sender(
			sender: futures::channel::mpsc::UnboundedSender<()>,
		) -> Self {
			let mut client = Self::minimal();
			client.exit_signal_sender = Some(Box::new(sender));
			client
		}
	}

	impl From<TestClientData> for TestClient {
		fn from(data: TestClientData) -> TestClient {
			TestClient { data: Arc::new(Mutex::new(data)) }
		}
	}

	#[async_trait]
	impl RelayClient for TestClient {
		type Error = TestError;

		async fn reconnect(&mut self) -> Result<(), TestError> {
			unimplemented!()
		}
	}

	#[async_trait]
	impl SourceClient<TestParachainsPipeline> for TestClient {
		async fn ensure_synced(&self) -> Result<bool, TestError> {
			self.data.lock().await.source_sync_status.clone()
		}

		async fn parachain_head(
			&self,
			_at_block: HeaderIdOf<TestChain>,
		) -> Result<AvailableHeader<HeaderIdOf<TestParachain>>, TestError> {
			self.data.lock().await.source_head.clone()
		}

		async fn prove_parachain_head(
			&self,
			_at_block: HeaderIdOf<TestChain>,
		) -> Result<(ParaHeadsProof, ParaHash), TestError> {
			let head = *self.data.lock().await.source_head.clone()?.as_available().unwrap();
			let storage_proof = vec![head.hash().encode()];
			let proof = (ParaHeadsProof { storage_proof }, head.hash());
			self.data.lock().await.source_proof.clone().map(|_| proof)
		}
	}

	#[async_trait]
	impl TargetClient<TestParachainsPipeline> for TestClient {
		type TransactionTracker = TestTransactionTracker;

		async fn best_block(&self) -> Result<HeaderIdOf<TestChain>, TestError> {
			self.data.lock().await.target_best_block.clone()
		}

		async fn best_finalized_source_relay_chain_block(
			&self,
			_at_block: &HeaderIdOf<TestChain>,
		) -> Result<HeaderIdOf<TestChain>, TestError> {
			self.data.lock().await.target_best_finalized_source_block.clone()
		}

		async fn parachain_head(
			&self,
			_at_block: HeaderIdOf<TestChain>,
		) -> Result<Option<HeaderIdOf<TestParachain>>, TestError> {
			self.data.lock().await.target_head.clone()
		}

		async fn submit_parachain_head_proof(
			&self,
			_at_source_block: HeaderIdOf<TestChain>,
			_updated_parachain_head: ParaHash,
			_proof: ParaHeadsProof,
		) -> Result<TestTransactionTracker, Self::Error> {
			let mut data = self.data.lock().await;
			data.target_submit_result.clone()?;

			if let Some(mut exit_signal_sender) = data.exit_signal_sender.take() {
				exit_signal_sender.send(()).await.unwrap();
			}
			Ok(TestTransactionTracker(Some(
				TrackedTransactionStatus::Finalized(Default::default()),
			)))
		}
	}

	#[test]
	fn when_source_client_fails_to_return_sync_state() {
		let mut test_source_client = TestClientData::minimal();
		test_source_client.source_sync_status = Err(TestError::Error);

		assert_eq!(
			async_std::task::block_on(run_until_connection_lost(
				TestClient::from(test_source_client),
				TestClient::from(TestClientData::minimal()),
				None,
				futures::future::pending(),
			)),
			Err(FailedClient::Source),
		);
	}

	#[test]
	fn when_target_client_fails_to_return_best_block() {
		let mut test_target_client = TestClientData::minimal();
		test_target_client.target_best_block = Err(TestError::Error);

		assert_eq!(
			async_std::task::block_on(run_until_connection_lost(
				TestClient::from(TestClientData::minimal()),
				TestClient::from(test_target_client),
				None,
				futures::future::pending(),
			)),
			Err(FailedClient::Target),
		);
	}

	#[test]
	fn when_target_client_fails_to_read_heads() {
		let mut test_target_client = TestClientData::minimal();
		test_target_client.target_head = Err(TestError::Error);

		assert_eq!(
			async_std::task::block_on(run_until_connection_lost(
				TestClient::from(TestClientData::minimal()),
				TestClient::from(test_target_client),
				None,
				futures::future::pending(),
			)),
			Err(FailedClient::Target),
		);
	}

	#[test]
	fn when_target_client_fails_to_read_best_finalized_source_block() {
		let mut test_target_client = TestClientData::minimal();
		test_target_client.target_best_finalized_source_block = Err(TestError::Error);

		assert_eq!(
			async_std::task::block_on(run_until_connection_lost(
				TestClient::from(TestClientData::minimal()),
				TestClient::from(test_target_client),
				None,
				futures::future::pending(),
			)),
			Err(FailedClient::Target),
		);
	}

	#[test]
	fn when_source_client_fails_to_read_heads() {
		let mut test_source_client = TestClientData::minimal();
		test_source_client.source_head = Err(TestError::Error);

		assert_eq!(
			async_std::task::block_on(run_until_connection_lost(
				TestClient::from(test_source_client),
				TestClient::from(TestClientData::minimal()),
				None,
				futures::future::pending(),
			)),
			Err(FailedClient::Source),
		);
	}

	#[test]
	fn when_source_client_fails_to_prove_heads() {
		let mut test_source_client = TestClientData::minimal();
		test_source_client.source_proof = Err(TestError::Error);

		assert_eq!(
			async_std::task::block_on(run_until_connection_lost(
				TestClient::from(test_source_client),
				TestClient::from(TestClientData::minimal()),
				None,
				futures::future::pending(),
			)),
			Err(FailedClient::Source),
		);
	}

	#[test]
	fn when_target_client_rejects_update_transaction() {
		let mut test_target_client = TestClientData::minimal();
		test_target_client.target_submit_result = Err(TestError::Error);

		assert_eq!(
			async_std::task::block_on(run_until_connection_lost(
				TestClient::from(TestClientData::minimal()),
				TestClient::from(test_target_client),
				None,
				futures::future::pending(),
			)),
			Err(FailedClient::Target),
		);
	}

	#[test]
	fn minimal_working_case() {
		let (exit_signal_sender, exit_signal) = futures::channel::mpsc::unbounded();
		assert_eq!(
			async_std::task::block_on(run_until_connection_lost(
				TestClient::from(TestClientData::minimal()),
				TestClient::from(TestClientData::with_exit_signal_sender(exit_signal_sender)),
				None,
				exit_signal.into_future().map(|(_, _)| ()),
			)),
			Ok(()),
		);
	}

	fn test_tx_tracker() -> SubmittedHeadsTracker<TestParachainsPipeline> {
		SubmittedHeadsTracker::new(
			AvailableHeader::Available(HeaderId(20, PARA_20_HASH)),
			TestTransactionTracker(None),
		)
	}

	impl From<SubmittedHeadStatus<TestParachainsPipeline>> for Option<()> {
		fn from(status: SubmittedHeadStatus<TestParachainsPipeline>) -> Option<()> {
			match status {
				SubmittedHeadStatus::Waiting(_) => Some(()),
				_ => None,
			}
		}
	}

	#[async_std::test]
	async fn tx_tracker_update_when_head_at_target_has_none_value() {
		assert_eq!(
			Some(()),
			test_tx_tracker()
				.update(&HeaderId(0, Default::default()), &Some(HeaderId(10, PARA_10_HASH)))
				.await
				.into(),
		);
	}

	#[async_std::test]
	async fn tx_tracker_update_when_head_at_target_has_old_value() {
		assert_eq!(
			Some(()),
			test_tx_tracker()
				.update(&HeaderId(0, Default::default()), &Some(HeaderId(10, PARA_10_HASH)))
				.await
				.into(),
		);
	}

	#[async_std::test]
	async fn tx_tracker_update_when_head_at_target_has_same_value() {
		assert!(matches!(
			test_tx_tracker()
				.update(&HeaderId(0, Default::default()), &Some(HeaderId(20, PARA_20_HASH)))
				.await,
			SubmittedHeadStatus::Final(TrackedTransactionStatus::Finalized(_)),
		));
	}

	#[async_std::test]
	async fn tx_tracker_update_when_head_at_target_has_better_value() {
		assert!(matches!(
			test_tx_tracker()
				.update(&HeaderId(0, Default::default()), &Some(HeaderId(30, PARA_20_HASH)))
				.await,
			SubmittedHeadStatus::Final(TrackedTransactionStatus::Finalized(_)),
		));
	}

	#[async_std::test]
	async fn tx_tracker_update_when_tx_is_lost() {
		let mut tx_tracker = test_tx_tracker();
		tx_tracker.transaction_tracker =
			futures::future::ready(TrackedTransactionStatus::Lost).boxed().shared();
		assert!(matches!(
			tx_tracker
				.update(&HeaderId(0, Default::default()), &Some(HeaderId(10, PARA_10_HASH)))
				.await,
			SubmittedHeadStatus::Final(TrackedTransactionStatus::Lost),
		));
	}

	#[async_std::test]
	async fn tx_tracker_update_when_tx_is_finalized_but_heads_are_not_updated() {
		let mut tx_tracker = test_tx_tracker();
		tx_tracker.transaction_tracker =
			futures::future::ready(TrackedTransactionStatus::Finalized(Default::default()))
				.boxed()
				.shared();
		assert!(matches!(
			tx_tracker
				.update(&HeaderId(0, Default::default()), &Some(HeaderId(10, PARA_10_HASH)))
				.await,
			SubmittedHeadStatus::Final(TrackedTransactionStatus::Lost),
		));
	}

	#[test]
	fn parachain_is_not_updated_if_it_is_unavailable() {
		assert!(!is_update_required::<TestParachainsPipeline>(
			AvailableHeader::Unavailable,
			None,
			Default::default(),
			Default::default(),
		));
		assert!(!is_update_required::<TestParachainsPipeline>(
			AvailableHeader::Unavailable,
			Some(HeaderId(10, PARA_10_HASH)),
			Default::default(),
			Default::default(),
		));
	}

	#[test]
	fn parachain_is_not_updated_if_it_is_unknown_to_both_clients() {
		assert!(!is_update_required::<TestParachainsPipeline>(
			AvailableHeader::Missing,
			None,
			Default::default(),
			Default::default(),
		),);
	}

	#[test]
	fn parachain_is_not_updated_if_target_has_better_head() {
		assert!(!is_update_required::<TestParachainsPipeline>(
			AvailableHeader::Available(HeaderId(10, Default::default())),
			Some(HeaderId(20, Default::default())),
			Default::default(),
			Default::default(),
		),);
	}

	#[test]
	fn parachain_is_updated_after_offboarding() {
		assert!(is_update_required::<TestParachainsPipeline>(
			AvailableHeader::Missing,
			Some(HeaderId(20, Default::default())),
			Default::default(),
			Default::default(),
		),);
	}

	#[test]
	fn parachain_is_updated_after_onboarding() {
		assert!(is_update_required::<TestParachainsPipeline>(
			AvailableHeader::Available(HeaderId(30, Default::default())),
			None,
			Default::default(),
			Default::default(),
		),);
	}

	#[test]
	fn parachain_is_updated_if_newer_head_is_known() {
		assert!(is_update_required::<TestParachainsPipeline>(
			AvailableHeader::Available(HeaderId(40, Default::default())),
			Some(HeaderId(30, Default::default())),
			Default::default(),
			Default::default(),
		),);
	}
}
