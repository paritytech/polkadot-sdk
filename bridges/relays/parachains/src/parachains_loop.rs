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
use bp_parachains::BestParaHeadHash;
use bp_polkadot_core::{
	parachains::{ParaHash, ParaHeadsProof, ParaId},
	BlockNumber as RelayBlockNumber,
};
use futures::{future::FutureExt, select};
use relay_substrate_client::{BlockNumberOf, Chain, HeaderIdOf};
use relay_utils::{metrics::MetricsParams, relay_loop::Client as RelayClient, FailedClient};
use std::{
	collections::{BTreeMap, BTreeSet},
	future::Future,
	time::{Duration, Instant},
};

/// Parachain heads synchronization params.
#[derive(Clone, Debug)]
pub struct ParachainSyncParams {
	/// Parachains that we're relaying here.
	pub parachains: Vec<ParaId>,
	/// Parachain heads update strategy.
	pub strategy: ParachainSyncStrategy,
	/// Stall timeout. If we have submitted transaction and we see no state updates for this
	/// period, we consider our transaction lost.
	pub stall_timeout: Duration,
}

/// Parachain heads update strategy.
#[derive(Clone, Copy, Debug)]
pub enum ParachainSyncStrategy {
	/// Update whenever any parachain head is updated.
	Any,
	/// Wait till all parachain heads are updated.
	All,
}

/// Parachain head hash, available at the source (relay) chain.
#[derive(Clone, Copy, Debug)]
pub enum ParaHashAtSource {
	/// There's no parachain head at the source chain.
	///
	/// Normally it means that the parachain is not registered there.
	None,
	/// Parachain head with given hash is available at the source chain.
	Some(ParaHash),
	/// The source client refuses to report parachain head hash at this moment.
	///
	/// It is a "mild" error, which may appear when e.g. on-demand parachains relay is used.
	/// This variant must be treated as "we don't want to update parachain head value at the
	/// target chain at this moment".
	Unavailable,
}

impl ParaHashAtSource {
	/// Return parachain head hash, if available.
	pub fn hash(&self) -> Option<&ParaHash> {
		match *self {
			ParaHashAtSource::Some(ref para_hash) => Some(para_hash),
			_ => None,
		}
	}
}

/// Source client used in parachain heads synchronization loop.
#[async_trait]
pub trait SourceClient<P: ParachainsPipeline>: RelayClient {
	/// Returns `Ok(true)` if client is in synced state.
	async fn ensure_synced(&self) -> Result<bool, Self::Error>;

	/// Get parachain head hash at given block.
	///
	/// The implementation may call `ParachainsLoopMetrics::update_best_parachain_block_at_source`
	/// on provided `metrics` object to update corresponding metric value.
	async fn parachain_head(
		&self,
		at_block: HeaderIdOf<P::SourceChain>,
		metrics: Option<&ParachainsLoopMetrics>,
		para_id: ParaId,
	) -> Result<ParaHashAtSource, Self::Error>;

	/// Get parachain heads proof.
	///
	/// The number and order of entries in the resulting parachain head hashes vector must match the
	/// number and order of parachains in the `parachains` vector. The incorrect implementation will
	/// result in panic.
	async fn prove_parachain_heads(
		&self,
		at_block: HeaderIdOf<P::SourceChain>,
		parachains: &[ParaId],
	) -> Result<(ParaHeadsProof, Vec<ParaHash>), Self::Error>;
}

/// Target client used in parachain heads synchronization loop.
#[async_trait]
pub trait TargetClient<P: ParachainsPipeline>: RelayClient {
	/// Get best block id.
	async fn best_block(&self) -> Result<HeaderIdOf<P::TargetChain>, Self::Error>;

	/// Get best finalized source block id.
	async fn best_finalized_source_block(
		&self,
		at_block: &HeaderIdOf<P::TargetChain>,
	) -> Result<HeaderIdOf<P::SourceChain>, Self::Error>;

	/// Get parachain head hash at given block.
	///
	/// The implementation may call `ParachainsLoopMetrics::update_best_parachain_block_at_target`
	/// on provided `metrics` object to update corresponding metric value.
	async fn parachain_head(
		&self,
		at_block: HeaderIdOf<P::TargetChain>,
		metrics: Option<&ParachainsLoopMetrics>,
		para_id: ParaId,
	) -> Result<Option<BestParaHeadHash>, Self::Error>;

	/// Submit parachain heads proof.
	async fn submit_parachain_heads_proof(
		&self,
		at_source_block: HeaderIdOf<P::SourceChain>,
		updated_parachains: Vec<(ParaId, ParaHash)>,
		proof: ParaHeadsProof,
	) -> Result<(), Self::Error>;
}

/// Return prefix that will be used by default to expose Prometheus metrics of the parachains
/// sync loop.
pub fn metrics_prefix<P: ParachainsPipeline>() -> String {
	format!("{}_to_{}_Parachains", P::SourceChain::NAME, P::TargetChain::NAME)
}

/// Run parachain heads synchronization.
pub async fn run<P: ParachainsPipeline>(
	source_client: impl SourceClient<P>,
	target_client: impl TargetClient<P>,
	sync_params: ParachainSyncParams,
	metrics_params: MetricsParams,
	exit_signal: impl Future<Output = ()> + 'static + Send,
) -> Result<(), relay_utils::Error>
where
	P::SourceChain: Chain<BlockNumber = RelayBlockNumber>,
{
	let exit_signal = exit_signal.shared();
	relay_utils::relay_loop(source_client, target_client)
		.with_metrics(metrics_params)
		.loop_metric(ParachainsLoopMetrics::new(Some(&metrics_prefix::<P>()))?)?
		.expose()
		.await?
		.run(metrics_prefix::<P>(), move |source_client, target_client, metrics| {
			run_until_connection_lost(
				source_client,
				target_client,
				sync_params.clone(),
				metrics,
				exit_signal.clone(),
			)
		})
		.await
}

/// Run parachain heads synchronization.
async fn run_until_connection_lost<P: ParachainsPipeline>(
	source_client: impl SourceClient<P>,
	target_client: impl TargetClient<P>,
	sync_params: ParachainSyncParams,
	metrics: Option<ParachainsLoopMetrics>,
	exit_signal: impl Future<Output = ()> + Send,
) -> Result<(), FailedClient>
where
	P::SourceChain: Chain<BlockNumber = RelayBlockNumber>,
{
	let exit_signal = exit_signal.fuse();
	let min_block_interval = std::cmp::min(
		P::SourceChain::AVERAGE_BLOCK_INTERVAL,
		P::TargetChain::AVERAGE_BLOCK_INTERVAL,
	);

	let mut tx_tracker: Option<TransactionTracker<P>> = None;

	futures::pin_mut!(exit_signal);

	// Note that the internal loop breaks with `FailedClient` error even if error is non-connection.
	// It is Ok for now, but it may need to be fixed in the future to use exponential backoff for
	// regular errors.

	loop {
		// either wait for new block, or exit signal
		select! {
			_ = async_std::task::sleep(min_block_interval).fuse() => {},
			_ = exit_signal => return Ok(()),
		}

		// if source client is not yet synced, we'll need to sleep. Otherwise we risk submitting too
		// much redundant transactions
		match source_client.ensure_synced().await {
			Ok(true) => (),
			Ok(false) => {
				log::warn!(
					target: "bridge",
					"{} client is syncing. Won't do anything until it is synced",
					P::SourceChain::NAME,
				);
				continue
			},
			Err(e) => {
				log::warn!(
					target: "bridge",
					"{} client has failed to return its sync status: {:?}",
					P::SourceChain::NAME,
					e,
				);
				return Err(FailedClient::Target)
			},
		}

		// if we have active transaction, we'll need to wait until it is mined or dropped
		let best_target_block = target_client.best_block().await.map_err(|e| {
			log::warn!(target: "bridge", "Failed to read best {} block: {:?}", P::SourceChain::NAME, e);
			FailedClient::Target
		})?;
		let heads_at_target = read_heads_at_target(
			&target_client,
			metrics.as_ref(),
			&best_target_block,
			&sync_params.parachains,
		)
		.await?;
		tx_tracker = tx_tracker.take().and_then(|tx_tracker| tx_tracker.update(&heads_at_target));
		if tx_tracker.is_some() {
			continue
		}

		// we have no active transaction and may need to update heads, but do we have something for
		// update?
		let best_finalized_relay_block = target_client
			.best_finalized_source_block(&best_target_block)
			.await
			.map_err(|e| {
				log::warn!(
					target: "bridge",
					"Failed to read best finalized {} block from {}: {:?}",
					P::SourceChain::NAME,
					P::TargetChain::NAME,
					e,
				);
				FailedClient::Target
			})?;
		let heads_at_source = read_heads_at_source(
			&source_client,
			metrics.as_ref(),
			&best_finalized_relay_block,
			&sync_params.parachains,
		)
		.await?;
		let updated_ids = select_parachains_to_update::<P>(
			heads_at_source,
			heads_at_target,
			best_finalized_relay_block,
		);
		let is_update_required = is_update_required(&sync_params, &updated_ids);

		log::info!(
			target: "bridge",
			"Total {} parachains: {}. Up-to-date at {}: {}. Needs update at {}: {}.",
			P::SourceChain::NAME,
			sync_params.parachains.len(),
			P::TargetChain::NAME,
			sync_params.parachains.len() - updated_ids.len(),
			P::TargetChain::NAME,
			updated_ids.len(),
		);

		if is_update_required {
			let (heads_proofs, head_hashes) = source_client
				.prove_parachain_heads(best_finalized_relay_block, &updated_ids)
				.await
				.map_err(|e| {
					log::warn!(
						target: "bridge",
						"Failed to prove {} parachain heads: {:?}",
						P::SourceChain::NAME,
						e,
					);
					FailedClient::Source
				})?;
			log::info!(
				target: "bridge",
				"Submitting {} parachain heads update transaction to {}",
				P::SourceChain::NAME,
				P::TargetChain::NAME,
			);

			assert_eq!(
				head_hashes.len(),
				updated_ids.len(),
				"Incorrect parachains SourceClient implementation"
			);

			target_client
				.submit_parachain_heads_proof(
					best_finalized_relay_block,
					updated_ids.iter().cloned().zip(head_hashes).collect(),
					heads_proofs,
				)
				.await
				.map_err(|e| {
					log::warn!(
						target: "bridge",
						"Failed to submit {} parachain heads proof to {}: {:?}",
						P::SourceChain::NAME,
						P::TargetChain::NAME,
						e,
					);
					FailedClient::Target
				})?;

			tx_tracker = Some(TransactionTracker::<P>::new(
				updated_ids,
				best_finalized_relay_block.0,
				sync_params.stall_timeout,
			));
		}
	}
}

/// Given heads at source and target clients, returns set of heads that are out of sync.
fn select_parachains_to_update<P: ParachainsPipeline>(
	heads_at_source: BTreeMap<ParaId, ParaHashAtSource>,
	heads_at_target: BTreeMap<ParaId, Option<BestParaHeadHash>>,
	best_finalized_relay_block: HeaderIdOf<P::SourceChain>,
) -> Vec<ParaId>
where
	P::SourceChain: Chain<BlockNumber = RelayBlockNumber>,
{
	log::trace!(
		target: "bridge",
		"Selecting {} parachains to update at {} (relay block: {:?}):\n\t\
			At {}: {:?}\n\t\
			At {}: {:?}",
		P::SourceChain::NAME,
		P::TargetChain::NAME,
		best_finalized_relay_block,
		P::SourceChain::NAME,
		heads_at_source,
		P::TargetChain::NAME,
		heads_at_target,
	);

	heads_at_source
		.into_iter()
		.zip(heads_at_target.into_iter())
		.filter(|((para, head_at_source), (_, head_at_target))| {
			let needs_update = match (head_at_source, head_at_target) {
				(ParaHashAtSource::Unavailable, _) => {
					// source client has politely asked us not to update current parachain head
					// at the target chain
					false
				},
				(ParaHashAtSource::Some(head_at_source), Some(head_at_target))
					if head_at_target.at_relay_block_number < best_finalized_relay_block.0 &&
						head_at_target.head_hash != *head_at_source =>
				{
					// source client knows head that is better than the head known to the target
					// client
					true
				},
				(ParaHashAtSource::Some(_), Some(_)) => {
					// this is normal case when relay has recently updated heads, when parachain is
					// not progressing, or when our source client is still syncing
					false
				},
				(ParaHashAtSource::Some(_), None) => {
					// parachain is not yet known to the target client. This is true when parachain
					// or bridge has been just onboarded/started
					true
				},
				(ParaHashAtSource::None, Some(_)) => {
					// parachain/parathread has been offboarded removed from the system. It needs to
					// be propageted to the target client
					true
				},
				(ParaHashAtSource::None, None) => {
					// all's good - parachain is unknown to both clients
					false
				},
			};
			if needs_update {
				log::trace!(
					target: "bridge",
					"{} parachain {:?} needs update at {}: {:?} vs {:?}",
					P::SourceChain::NAME,
					para,
					P::TargetChain::NAME,
					head_at_source,
					head_at_target,
				);
			}

			needs_update
		})
		.map(|((para, _), _)| para)
		.collect()
}

/// Returns true if we need to submit update transactions to the target node.
fn is_update_required(sync_params: &ParachainSyncParams, updated_ids: &[ParaId]) -> bool {
	match sync_params.strategy {
		ParachainSyncStrategy::All => updated_ids.len() == sync_params.parachains.len(),
		ParachainSyncStrategy::Any => !updated_ids.is_empty(),
	}
}

/// Reads given parachains heads from the source client.
///
/// Guarantees that the returning map will have an entry for every parachain from `parachains`.
async fn read_heads_at_source<P: ParachainsPipeline>(
	source_client: &impl SourceClient<P>,
	metrics: Option<&ParachainsLoopMetrics>,
	at_relay_block: &HeaderIdOf<P::SourceChain>,
	parachains: &[ParaId],
) -> Result<BTreeMap<ParaId, ParaHashAtSource>, FailedClient> {
	let mut para_head_hashes = BTreeMap::new();
	for para in parachains {
		let para_head = source_client.parachain_head(*at_relay_block, metrics, *para).await;
		match para_head {
			Ok(para_head) => {
				para_head_hashes.insert(*para, para_head);
			},
			Err(e) => {
				log::warn!(
					target: "bridge",
					"Failed to read head of {} parachain {:?}: {:?}",
					P::SourceChain::NAME,
					para,
					e,
				);
				return Err(FailedClient::Source)
			},
		}
	}
	Ok(para_head_hashes)
}

/// Reads given parachains heads from the source client.
///
/// Guarantees that the returning map will have an entry for every parachain from `parachains`.
async fn read_heads_at_target<P: ParachainsPipeline>(
	target_client: &impl TargetClient<P>,
	metrics: Option<&ParachainsLoopMetrics>,
	at_block: &HeaderIdOf<P::TargetChain>,
	parachains: &[ParaId],
) -> Result<BTreeMap<ParaId, Option<BestParaHeadHash>>, FailedClient> {
	let mut para_best_head_hashes = BTreeMap::new();
	for para in parachains {
		let para_best_head = target_client.parachain_head(*at_block, metrics, *para).await;
		match para_best_head {
			Ok(para_best_head) => {
				para_best_head_hashes.insert(*para, para_best_head);
			},
			Err(e) => {
				log::warn!(
					target: "bridge",
					"Failed to read head of {} parachain {:?} at {}: {:?}",
					P::SourceChain::NAME,
					para,
					P::TargetChain::NAME,
					e,
				);
				return Err(FailedClient::Target)
			},
		}
	}
	Ok(para_best_head_hashes)
}

/// Parachain heads transaction tracker.
struct TransactionTracker<P: ParachainsPipeline> {
	/// Ids of parachains which heads were updated in the tracked transaction.
	awaiting_update: BTreeSet<ParaId>,
	/// Number of relay chain block that has been used to craft parachain heads proof.
	relay_block_number: BlockNumberOf<P::SourceChain>,
	/// Transaction submit time.
	submitted_at: Instant,
	/// Transaction death time.
	death_time: Instant,
}

impl<P: ParachainsPipeline> TransactionTracker<P>
where
	P::SourceChain: Chain<BlockNumber = RelayBlockNumber>,
{
	/// Creates new parachain heads transaction tracker.
	pub fn new(
		awaiting_update: impl IntoIterator<Item = ParaId>,
		relay_block_number: BlockNumberOf<P::SourceChain>,
		stall_timeout: Duration,
	) -> Self {
		let now = Instant::now();
		TransactionTracker {
			awaiting_update: awaiting_update.into_iter().collect(),
			relay_block_number,
			submitted_at: now,
			death_time: now + stall_timeout,
		}
	}

	/// Returns `None` if all parachain heads have been updated or we consider our transaction dead.
	pub fn update(
		mut self,
		heads_at_target: &BTreeMap<ParaId, Option<BestParaHeadHash>>,
	) -> Option<Self> {
		// remove all pending heads that were synced
		for (para, best_para_head) in heads_at_target {
			if best_para_head
				.as_ref()
				.map(|best_para_head| {
					best_para_head.at_relay_block_number >= self.relay_block_number
				})
				.unwrap_or(false)
			{
				self.awaiting_update.remove(para);

				log::trace!(
					target: "bridge",
					"Head of parachain {:?} has been updated at {}: {:?}. Outdated parachains remaining: {}",
					para,
					P::TargetChain::NAME,
					best_para_head,
					self.awaiting_update.len(),
				);
			}
		}

		// if we have synced all required heads, we are done
		if self.awaiting_update.is_empty() {
			return None
		}

		// if our transaction is dead now, we may start over again
		let now = Instant::now();
		if now >= self.death_time {
			log::warn!(
				target: "bridge",
				"Parachain heads update transaction {} has been lost: no updates for {}s",
				P::TargetChain::NAME,
				(now - self.submitted_at).as_secs(),
			);

			return None
		}

		Some(self)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use async_std::sync::{Arc, Mutex};
	use codec::Encode;
	use futures::{SinkExt, StreamExt};
	use relay_substrate_client::test_chain::TestChain;
	use relay_utils::{HeaderId, MaybeConnectionError};
	use sp_core::H256;

	const PARA_ID: u32 = 0;
	const PARA_0_HASH: ParaHash = H256([1u8; 32]);
	const PARA_1_HASH: ParaHash = H256([2u8; 32]);

	#[derive(Clone, Debug)]
	enum TestError {
		Error,
		MissingParachainHeadProof,
	}

	impl MaybeConnectionError for TestError {
		fn is_connection_error(&self) -> bool {
			false
		}
	}

	#[derive(Clone, Debug, PartialEq, Eq)]
	struct TestParachainsPipeline;

	impl ParachainsPipeline for TestParachainsPipeline {
		type SourceChain = TestChain;
		type TargetChain = TestChain;
	}

	#[derive(Clone, Debug)]
	struct TestClient {
		data: Arc<Mutex<TestClientData>>,
	}

	#[derive(Clone, Debug)]
	struct TestClientData {
		source_sync_status: Result<bool, TestError>,
		source_heads: BTreeMap<u32, Result<ParaHashAtSource, TestError>>,
		source_proofs: BTreeMap<u32, Result<Vec<u8>, TestError>>,

		target_best_block: Result<HeaderIdOf<TestChain>, TestError>,
		target_best_finalized_source_block: Result<HeaderIdOf<TestChain>, TestError>,
		target_heads: BTreeMap<u32, Result<BestParaHeadHash, TestError>>,
		target_submit_result: Result<(), TestError>,

		exit_signal_sender: Option<Box<futures::channel::mpsc::UnboundedSender<()>>>,
	}

	impl TestClientData {
		pub fn minimal() -> Self {
			TestClientData {
				source_sync_status: Ok(true),
				source_heads: vec![(PARA_ID, Ok(ParaHashAtSource::Some(PARA_0_HASH)))]
					.into_iter()
					.collect(),
				source_proofs: vec![(PARA_ID, Ok(PARA_0_HASH.encode()))].into_iter().collect(),

				target_best_block: Ok(HeaderId(0, Default::default())),
				target_best_finalized_source_block: Ok(HeaderId(0, Default::default())),
				target_heads: BTreeMap::new(),
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
			_metrics: Option<&ParachainsLoopMetrics>,
			para_id: ParaId,
		) -> Result<ParaHashAtSource, TestError> {
			match self.data.lock().await.source_heads.get(&para_id.0).cloned() {
				Some(result) => result,
				None => Ok(ParaHashAtSource::None),
			}
		}

		async fn prove_parachain_heads(
			&self,
			_at_block: HeaderIdOf<TestChain>,
			parachains: &[ParaId],
		) -> Result<(ParaHeadsProof, Vec<ParaHash>), TestError> {
			let mut proofs = Vec::new();
			for para_id in parachains {
				proofs.push(
					self.data
						.lock()
						.await
						.source_proofs
						.get(&para_id.0)
						.cloned()
						.transpose()?
						.ok_or(TestError::MissingParachainHeadProof)?,
				);
			}
			Ok((ParaHeadsProof(proofs), vec![Default::default(); parachains.len()]))
		}
	}

	#[async_trait]
	impl TargetClient<TestParachainsPipeline> for TestClient {
		async fn best_block(&self) -> Result<HeaderIdOf<TestChain>, TestError> {
			self.data.lock().await.target_best_block.clone()
		}

		async fn best_finalized_source_block(
			&self,
			_at_block: &HeaderIdOf<TestChain>,
		) -> Result<HeaderIdOf<TestChain>, TestError> {
			self.data.lock().await.target_best_finalized_source_block.clone()
		}

		async fn parachain_head(
			&self,
			_at_block: HeaderIdOf<TestChain>,
			_metrics: Option<&ParachainsLoopMetrics>,
			para_id: ParaId,
		) -> Result<Option<BestParaHeadHash>, TestError> {
			self.data.lock().await.target_heads.get(&para_id.0).cloned().transpose()
		}

		async fn submit_parachain_heads_proof(
			&self,
			_at_source_block: HeaderIdOf<TestChain>,
			_updated_parachains: Vec<(ParaId, ParaHash)>,
			_proof: ParaHeadsProof,
		) -> Result<(), Self::Error> {
			self.data.lock().await.target_submit_result.clone()?;

			if let Some(mut exit_signal_sender) = self.data.lock().await.exit_signal_sender.take() {
				exit_signal_sender.send(()).await.unwrap();
			}
			Ok(())
		}
	}

	fn default_sync_params() -> ParachainSyncParams {
		ParachainSyncParams {
			parachains: vec![ParaId(PARA_ID)],
			strategy: ParachainSyncStrategy::Any,
			stall_timeout: Duration::from_secs(60),
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
				default_sync_params(),
				None,
				futures::future::pending(),
			)),
			Err(FailedClient::Target),
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
				default_sync_params(),
				None,
				futures::future::pending(),
			)),
			Err(FailedClient::Target),
		);
	}

	#[test]
	fn when_target_client_fails_to_read_heads() {
		let mut test_target_client = TestClientData::minimal();
		test_target_client.target_heads.insert(PARA_ID, Err(TestError::Error));

		assert_eq!(
			async_std::task::block_on(run_until_connection_lost(
				TestClient::from(TestClientData::minimal()),
				TestClient::from(test_target_client),
				default_sync_params(),
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
				default_sync_params(),
				None,
				futures::future::pending(),
			)),
			Err(FailedClient::Target),
		);
	}

	#[test]
	fn when_source_client_fails_to_read_heads() {
		let mut test_source_client = TestClientData::minimal();
		test_source_client.source_heads.insert(PARA_ID, Err(TestError::Error));

		assert_eq!(
			async_std::task::block_on(run_until_connection_lost(
				TestClient::from(test_source_client),
				TestClient::from(TestClientData::minimal()),
				default_sync_params(),
				None,
				futures::future::pending(),
			)),
			Err(FailedClient::Source),
		);
	}

	#[test]
	fn when_source_client_fails_to_prove_heads() {
		let mut test_source_client = TestClientData::minimal();
		test_source_client.source_proofs.insert(PARA_ID, Err(TestError::Error));

		assert_eq!(
			async_std::task::block_on(run_until_connection_lost(
				TestClient::from(test_source_client),
				TestClient::from(TestClientData::minimal()),
				default_sync_params(),
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
				default_sync_params(),
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
				default_sync_params(),
				None,
				exit_signal.into_future().map(|(_, _)| ()),
			)),
			Ok(()),
		);
	}

	const PARA_1_ID: u32 = PARA_ID + 1;
	const SOURCE_BLOCK_NUMBER: u32 = 100;

	fn test_tx_tracker() -> TransactionTracker<TestParachainsPipeline> {
		TransactionTracker::new(
			vec![ParaId(PARA_ID), ParaId(PARA_1_ID)],
			SOURCE_BLOCK_NUMBER,
			Duration::from_secs(1),
		)
	}

	#[test]
	fn tx_tracker_update_when_nothing_is_updated() {
		assert_eq!(
			test_tx_tracker()
				.update(&vec![].into_iter().collect())
				.map(|t| t.awaiting_update),
			Some(test_tx_tracker().awaiting_update),
		);
	}

	#[test]
	fn tx_tracker_update_when_one_of_heads_is_updated_to_previous_value() {
		assert_eq!(
			test_tx_tracker()
				.update(
					&vec![(
						ParaId(PARA_ID),
						Some(BestParaHeadHash {
							at_relay_block_number: SOURCE_BLOCK_NUMBER - 1,
							head_hash: PARA_0_HASH,
						})
					)]
					.into_iter()
					.collect()
				)
				.map(|t| t.awaiting_update),
			Some(test_tx_tracker().awaiting_update),
		);
	}

	#[test]
	fn tx_tracker_update_when_one_of_heads_is_updated() {
		assert_eq!(
			test_tx_tracker()
				.update(
					&vec![(
						ParaId(PARA_ID),
						Some(BestParaHeadHash {
							at_relay_block_number: SOURCE_BLOCK_NUMBER,
							head_hash: PARA_0_HASH,
						})
					)]
					.into_iter()
					.collect()
				)
				.map(|t| t.awaiting_update),
			Some(vec![ParaId(PARA_1_ID)].into_iter().collect()),
		);
	}

	#[test]
	fn tx_tracker_update_when_all_heads_are_updated() {
		assert_eq!(
			test_tx_tracker()
				.update(
					&vec![
						(
							ParaId(PARA_ID),
							Some(BestParaHeadHash {
								at_relay_block_number: SOURCE_BLOCK_NUMBER,
								head_hash: PARA_0_HASH,
							})
						),
						(
							ParaId(PARA_1_ID),
							Some(BestParaHeadHash {
								at_relay_block_number: SOURCE_BLOCK_NUMBER,
								head_hash: PARA_0_HASH,
							})
						),
					]
					.into_iter()
					.collect()
				)
				.map(|t| t.awaiting_update),
			None,
		);
	}

	#[test]
	fn tx_tracker_update_when_tx_is_stalled() {
		let mut tx_tracker = test_tx_tracker();
		tx_tracker.death_time = Instant::now();
		assert_eq!(
			tx_tracker.update(&vec![].into_iter().collect()).map(|t| t.awaiting_update),
			None,
		);
	}

	#[test]
	fn parachain_is_not_updated_if_it_is_unknown_to_both_clients() {
		assert_eq!(
			select_parachains_to_update::<TestParachainsPipeline>(
				vec![(ParaId(PARA_ID), ParaHashAtSource::None)].into_iter().collect(),
				vec![(ParaId(PARA_ID), None)].into_iter().collect(),
				HeaderId(10, Default::default()),
			),
			Vec::<ParaId>::new(),
		);
	}

	#[test]
	fn parachain_is_not_updated_if_it_has_been_updated_at_better_relay_block() {
		assert_eq!(
			select_parachains_to_update::<TestParachainsPipeline>(
				vec![(ParaId(PARA_ID), ParaHashAtSource::Some(PARA_0_HASH))]
					.into_iter()
					.collect(),
				vec![(
					ParaId(PARA_ID),
					Some(BestParaHeadHash { at_relay_block_number: 20, head_hash: PARA_1_HASH })
				)]
				.into_iter()
				.collect(),
				HeaderId(10, Default::default()),
			),
			Vec::<ParaId>::new(),
		);
	}

	#[test]
	fn parachain_is_not_updated_if_hash_is_the_same_at_next_relay_block() {
		assert_eq!(
			select_parachains_to_update::<TestParachainsPipeline>(
				vec![(ParaId(PARA_ID), ParaHashAtSource::Some(PARA_0_HASH))]
					.into_iter()
					.collect(),
				vec![(
					ParaId(PARA_ID),
					Some(BestParaHeadHash { at_relay_block_number: 0, head_hash: PARA_0_HASH })
				)]
				.into_iter()
				.collect(),
				HeaderId(10, Default::default()),
			),
			Vec::<ParaId>::new(),
		);
	}

	#[test]
	fn parachain_is_updated_after_offboarding() {
		assert_eq!(
			select_parachains_to_update::<TestParachainsPipeline>(
				vec![(ParaId(PARA_ID), ParaHashAtSource::None)].into_iter().collect(),
				vec![(
					ParaId(PARA_ID),
					Some(BestParaHeadHash {
						at_relay_block_number: 0,
						head_hash: Default::default(),
					})
				)]
				.into_iter()
				.collect(),
				HeaderId(10, Default::default()),
			),
			vec![ParaId(PARA_ID)],
		);
	}

	#[test]
	fn parachain_is_updated_after_onboarding() {
		assert_eq!(
			select_parachains_to_update::<TestParachainsPipeline>(
				vec![(ParaId(PARA_ID), ParaHashAtSource::Some(PARA_0_HASH))]
					.into_iter()
					.collect(),
				vec![(ParaId(PARA_ID), None)].into_iter().collect(),
				HeaderId(10, Default::default()),
			),
			vec![ParaId(PARA_ID)],
		);
	}

	#[test]
	fn parachain_is_updated_if_newer_head_is_known() {
		assert_eq!(
			select_parachains_to_update::<TestParachainsPipeline>(
				vec![(ParaId(PARA_ID), ParaHashAtSource::Some(PARA_1_HASH))]
					.into_iter()
					.collect(),
				vec![(
					ParaId(PARA_ID),
					Some(BestParaHeadHash { at_relay_block_number: 0, head_hash: PARA_0_HASH })
				)]
				.into_iter()
				.collect(),
				HeaderId(10, Default::default()),
			),
			vec![ParaId(PARA_ID)],
		);
	}

	#[test]
	fn parachain_is_not_updated_if_source_head_is_unavailable() {
		assert_eq!(
			select_parachains_to_update::<TestParachainsPipeline>(
				vec![(ParaId(PARA_ID), ParaHashAtSource::Unavailable)].into_iter().collect(),
				vec![(
					ParaId(PARA_ID),
					Some(BestParaHeadHash { at_relay_block_number: 0, head_hash: PARA_0_HASH })
				)]
				.into_iter()
				.collect(),
				HeaderId(10, Default::default()),
			),
			vec![],
		);
	}

	#[test]
	fn is_update_required_works() {
		let mut sync_params = ParachainSyncParams {
			parachains: vec![ParaId(PARA_ID), ParaId(PARA_1_ID)],
			strategy: ParachainSyncStrategy::Any,
			stall_timeout: Duration::from_secs(60),
		};

		assert!(!is_update_required(&sync_params, &[]));
		assert!(is_update_required(&sync_params, &[ParaId(PARA_ID)]));
		assert!(is_update_required(&sync_params, &[ParaId(PARA_ID), ParaId(PARA_1_ID)]));

		sync_params.strategy = ParachainSyncStrategy::All;
		assert!(!is_update_required(&sync_params, &[]));
		assert!(!is_update_required(&sync_params, &[ParaId(PARA_ID)]));
		assert!(is_update_required(&sync_params, &[ParaId(PARA_ID), ParaId(PARA_1_ID)]));
	}
}
