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

//! The loop basically reads all missing headers and their finality proofs from the source client.
//! The proof for the best possible header is then submitted to the target node. The only exception
//! is the mandatory headers, which we always submit to the target node. For such headers, we
//! assume that the persistent proof either exists, or will eventually become available.

use crate::{sync_loop_metrics::SyncLoopMetrics, Error, FinalitySyncPipeline, SourceHeader};

use crate::{
	base::SourceClientBase,
	finality_proofs::{FinalityProofsBuf, FinalityProofsStream},
	headers::{JustifiedHeader, JustifiedHeaderSelector},
};
use async_trait::async_trait;
use backoff::{backoff::Backoff, ExponentialBackoff};
use futures::{future::Fuse, select, Future, FutureExt};
use num_traits::{Saturating, Zero};
use relay_utils::{
	metrics::MetricsParams, relay_loop::Client as RelayClient, retry_backoff, FailedClient,
	HeaderId, MaybeConnectionError, TrackedTransactionStatus, TransactionTracker,
};
use std::{
	fmt::Debug,
	time::{Duration, Instant},
};

/// Type of headers that we relay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeadersToRelay {
	/// Relay all headers.
	All,
	/// Relay only mandatory headers.
	Mandatory,
	/// Relay only free (including mandatory) headers.
	Free,
}

/// Finality proof synchronization loop parameters.
#[derive(Debug, Clone)]
pub struct FinalitySyncParams {
	/// Interval at which we check updates on both clients. Normally should be larger than
	/// `min(source_block_time, target_block_time)`.
	///
	/// This parameter may be used to limit transactions rate. Increase the value && you'll get
	/// infrequent updates => sparse headers => potential slow down of bridge applications, but
	/// pallet storage won't be super large. Decrease the value to near `source_block_time` and
	/// you'll get transaction for (almost) every block of the source chain => all source headers
	/// will be known to the target chain => bridge applications will run faster, but pallet
	/// storage may explode (but if pruning is there, then it's fine).
	pub tick: Duration,
	/// Number of finality proofs to keep in internal buffer between loop iterations.
	///
	/// While in "major syncing" state, we still read finality proofs from the stream. They're
	/// stored in the internal buffer between loop iterations. When we're close to the tip of the
	/// chain, we may meet finality delays if headers are not finalized frequently. So instead of
	/// waiting for next finality proof to appear in the stream, we may use existing proof from
	/// that buffer.
	pub recent_finality_proofs_limit: usize,
	/// Timeout before we treat our transactions as lost and restart the whole sync process.
	pub stall_timeout: Duration,
	/// If true, only mandatory headers are relayed.
	pub headers_to_relay: HeadersToRelay,
}

/// Source client used in finality synchronization loop.
#[async_trait]
pub trait SourceClient<P: FinalitySyncPipeline>: SourceClientBase<P> {
	/// Get best finalized block number.
	async fn best_finalized_block_number(&self) -> Result<P::Number, Self::Error>;

	/// Get canonical header and its finality proof by number.
	async fn header_and_finality_proof(
		&self,
		number: P::Number,
	) -> Result<(P::Header, Option<P::FinalityProof>), Self::Error>;
}

/// Target client used in finality synchronization loop.
#[async_trait]
pub trait TargetClient<P: FinalitySyncPipeline>: RelayClient {
	/// Transaction tracker to track submitted transactions.
	type TransactionTracker: TransactionTracker;

	/// Get best finalized source block number.
	async fn best_finalized_source_block_id(
		&self,
	) -> Result<HeaderId<P::Hash, P::Number>, Self::Error>;

	/// Get free source headers submission interval, if it is configured in the
	/// target runtime.
	async fn free_source_headers_interval(&self) -> Result<Option<P::Number>, Self::Error>;

	/// Submit header finality proof.
	async fn submit_finality_proof(
		&self,
		header: P::Header,
		proof: P::FinalityProof,
		is_free_execution_expected: bool,
	) -> Result<Self::TransactionTracker, Self::Error>;
}

/// Return prefix that will be used by default to expose Prometheus metrics of the finality proofs
/// sync loop.
pub fn metrics_prefix<P: FinalitySyncPipeline>() -> String {
	format!("{}_to_{}_Sync", P::SOURCE_NAME, P::TARGET_NAME)
}

/// Finality sync information.
pub struct SyncInfo<P: FinalitySyncPipeline> {
	/// Best finalized header at the source client.
	pub best_number_at_source: P::Number,
	/// Best source header, known to the target client.
	pub best_number_at_target: P::Number,
	/// Whether the target client follows the same fork as the source client do.
	pub is_using_same_fork: bool,
}

impl<P: FinalitySyncPipeline> SyncInfo<P> {
	/// Checks if both clients are on the same fork.
	async fn is_on_same_fork<SC: SourceClient<P>>(
		source_client: &SC,
		id_at_target: &HeaderId<P::Hash, P::Number>,
	) -> Result<bool, SC::Error> {
		let header_at_source = source_client.header_and_finality_proof(id_at_target.0).await?.0;
		let header_hash_at_source = header_at_source.hash();
		Ok(if id_at_target.1 == header_hash_at_source {
			true
		} else {
			log::error!(
				target: "bridge",
				"Source node ({}) and pallet at target node ({}) have different headers at the same height {:?}: \
				at-source {:?} vs at-target {:?}",
				P::SOURCE_NAME,
				P::TARGET_NAME,
				id_at_target.0,
				header_hash_at_source,
				id_at_target.1,
			);

			false
		})
	}

	async fn new<SC: SourceClient<P>, TC: TargetClient<P>>(
		source_client: &SC,
		target_client: &TC,
	) -> Result<Self, Error<P, SC::Error, TC::Error>> {
		let best_number_at_source =
			source_client.best_finalized_block_number().await.map_err(Error::Source)?;
		let best_id_at_target =
			target_client.best_finalized_source_block_id().await.map_err(Error::Target)?;
		let best_number_at_target = best_id_at_target.0;

		let is_using_same_fork = Self::is_on_same_fork(source_client, &best_id_at_target)
			.await
			.map_err(Error::Source)?;

		Ok(Self { best_number_at_source, best_number_at_target, is_using_same_fork })
	}

	fn update_metrics(&self, metrics_sync: &Option<SyncLoopMetrics>) {
		if let Some(metrics_sync) = metrics_sync {
			metrics_sync.update_best_block_at_source(self.best_number_at_source);
			metrics_sync.update_best_block_at_target(self.best_number_at_target);
			metrics_sync.update_using_same_fork(self.is_using_same_fork);
		}
	}

	pub fn num_headers(&self) -> P::Number {
		self.best_number_at_source.saturating_sub(self.best_number_at_target)
	}
}

/// Information about transaction that we have submitted.
#[derive(Debug, Clone)]
pub struct Transaction<Tracker, Number> {
	/// Submitted transaction tracker.
	tracker: Tracker,
	/// The number of the header we have submitted.
	header_number: Number,
}

impl<Tracker: TransactionTracker, Number: Debug + PartialOrd> Transaction<Tracker, Number> {
	pub async fn submit<
		P: FinalitySyncPipeline<Number = Number>,
		TC: TargetClient<P, TransactionTracker = Tracker>,
	>(
		target_client: &TC,
		header: P::Header,
		justification: P::FinalityProof,
		is_free_execution_expected: bool,
	) -> Result<Self, TC::Error> {
		let header_number = header.number();
		log::debug!(
			target: "bridge",
			"Going to submit finality proof of {} header #{:?} to {}",
			P::SOURCE_NAME,
			header_number,
			P::TARGET_NAME,
		);

		let tracker = target_client
			.submit_finality_proof(header, justification, is_free_execution_expected)
			.await?;
		Ok(Transaction { tracker, header_number })
	}

	async fn track<
		P: FinalitySyncPipeline<Number = Number>,
		SC: SourceClient<P>,
		TC: TargetClient<P>,
	>(
		self,
		target_client: TC,
	) -> Result<(), Error<P, SC::Error, TC::Error>> {
		match self.tracker.wait().await {
			TrackedTransactionStatus::Finalized(_) => {
				// The transaction has been finalized, but it may have been finalized in the
				// "failed" state. So let's check if the block number was actually updated.
				target_client
					.best_finalized_source_block_id()
					.await
					.map_err(Error::Target)
					.and_then(|best_id_at_target| {
						if self.header_number > best_id_at_target.0 {
							return Err(Error::ProofSubmissionTxFailed {
								submitted_number: self.header_number,
								best_number_at_target: best_id_at_target.0,
							})
						}
						Ok(())
					})
			},
			TrackedTransactionStatus::Lost => Err(Error::ProofSubmissionTxLost),
		}
	}
}

/// Finality synchronization loop state.
struct FinalityLoop<P: FinalitySyncPipeline, SC: SourceClient<P>, TC: TargetClient<P>> {
	source_client: SC,
	target_client: TC,

	sync_params: FinalitySyncParams,
	metrics_sync: Option<SyncLoopMetrics>,

	progress: (Instant, Option<P::Number>),
	retry_backoff: ExponentialBackoff,
	finality_proofs_stream: FinalityProofsStream<P, SC>,
	finality_proofs_buf: FinalityProofsBuf<P>,
	best_submitted_number: Option<P::Number>,
}

impl<P: FinalitySyncPipeline, SC: SourceClient<P>, TC: TargetClient<P>> FinalityLoop<P, SC, TC> {
	pub fn new(
		source_client: SC,
		target_client: TC,
		sync_params: FinalitySyncParams,
		metrics_sync: Option<SyncLoopMetrics>,
	) -> Self {
		Self {
			source_client,
			target_client,
			sync_params,
			metrics_sync,
			progress: (Instant::now(), None),
			retry_backoff: retry_backoff(),
			finality_proofs_stream: FinalityProofsStream::new(),
			finality_proofs_buf: FinalityProofsBuf::new(vec![]),
			best_submitted_number: None,
		}
	}

	fn update_progress(&mut self, info: &SyncInfo<P>) {
		let (prev_time, prev_best_number_at_target) = self.progress;
		let now = Instant::now();

		let needs_update = now - prev_time > Duration::from_secs(10) ||
			prev_best_number_at_target
				.map(|prev_best_number_at_target| {
					info.best_number_at_target.saturating_sub(prev_best_number_at_target) >
						10.into()
				})
				.unwrap_or(true);

		if !needs_update {
			return
		}

		log::info!(
			target: "bridge",
			"Synced {:?} of {:?} headers",
			info.best_number_at_target,
			info.best_number_at_source,
		);

		self.progress = (now, Some(info.best_number_at_target))
	}

	pub async fn select_header_to_submit(
		&mut self,
		info: &SyncInfo<P>,
		free_headers_interval: Option<P::Number>,
	) -> Result<Option<JustifiedHeader<P>>, Error<P, SC::Error, TC::Error>> {
		// to see that the loop is progressing
		log::trace!(
			target: "bridge",
			"Considering range of headers ({}; {}]",
			info.best_number_at_target,
			info.best_number_at_source
		);

		// read missing headers
		let selector = JustifiedHeaderSelector::new::<SC, TC>(
			&self.source_client,
			info,
			self.sync_params.headers_to_relay,
			free_headers_interval,
		)
		.await?;
		// if we see that the header schedules GRANDPA change, we need to submit it
		if self.sync_params.headers_to_relay == HeadersToRelay::Mandatory {
			return Ok(selector.select_mandatory())
		}

		// all headers that are missing from the target client are non-mandatory
		// => even if we have already selected some header and its persistent finality proof,
		// we may try to select better header by reading non-persistent proofs from the stream
		self.finality_proofs_buf.fill(&mut self.finality_proofs_stream);
		let maybe_justified_header = selector.select(
			info,
			self.sync_params.headers_to_relay,
			free_headers_interval,
			&self.finality_proofs_buf,
		);

		// remove obsolete 'recent' finality proofs + keep its size under certain limit
		let oldest_finality_proof_to_keep = maybe_justified_header
			.as_ref()
			.map(|justified_header| justified_header.number())
			.unwrap_or(info.best_number_at_target);
		self.finality_proofs_buf.prune(
			oldest_finality_proof_to_keep,
			Some(self.sync_params.recent_finality_proofs_limit),
		);

		Ok(maybe_justified_header)
	}

	pub async fn run_iteration(
		&mut self,
		free_headers_interval: Option<P::Number>,
	) -> Result<
		Option<Transaction<TC::TransactionTracker, P::Number>>,
		Error<P, SC::Error, TC::Error>,
	> {
		// read best source headers ids from source and target nodes
		let info = SyncInfo::new(&self.source_client, &self.target_client).await?;
		info.update_metrics(&self.metrics_sync);
		self.update_progress(&info);

		// if we have already submitted header, then we just need to wait for it
		// if we're waiting too much, then we believe our transaction has been lost and restart sync
		if Some(info.best_number_at_target) < self.best_submitted_number {
			return Ok(None)
		}

		// submit new header if we have something new
		match self.select_header_to_submit(&info, free_headers_interval).await? {
			Some(header) => {
				let transaction = Transaction::submit(
					&self.target_client,
					header.header,
					header.proof,
					self.sync_params.headers_to_relay == HeadersToRelay::Free,
				)
				.await
				.map_err(Error::Target)?;
				self.best_submitted_number = Some(transaction.header_number);
				Ok(Some(transaction))
			},
			None => Ok(None),
		}
	}

	async fn ensure_finality_proofs_stream(&mut self) -> Result<(), FailedClient> {
		if let Err(e) = self.finality_proofs_stream.ensure_stream(&self.source_client).await {
			if e.is_connection_error() {
				return Err(FailedClient::Source)
			}
		}

		Ok(())
	}

	/// Run finality relay loop until connection to one of nodes is lost.
	async fn run_until_connection_lost(
		&mut self,
		exit_signal: impl Future<Output = ()>,
	) -> Result<(), FailedClient> {
		self.ensure_finality_proofs_stream().await?;
		let proof_submission_tx_tracker = Fuse::terminated();
		let exit_signal = exit_signal.fuse();
		futures::pin_mut!(exit_signal, proof_submission_tx_tracker);

		let free_headers_interval = free_headers_interval(&self.target_client).await?;

		loop {
			// run loop iteration
			let next_tick = match self.run_iteration(free_headers_interval).await {
				Ok(Some(tx)) => {
					proof_submission_tx_tracker
						.set(tx.track::<P, SC, _>(self.target_client.clone()).fuse());
					self.retry_backoff.reset();
					self.sync_params.tick
				},
				Ok(None) => {
					self.retry_backoff.reset();
					self.sync_params.tick
				},
				Err(error) => {
					log::error!(target: "bridge", "Finality sync loop iteration has failed with error: {:?}", error);
					error.fail_if_connection_error()?;
					self.retry_backoff
						.next_backoff()
						.unwrap_or(relay_utils::relay_loop::RECONNECT_DELAY)
				},
			};
			self.ensure_finality_proofs_stream().await?;

			// wait till exit signal, or new source block
			select! {
				proof_submission_result = proof_submission_tx_tracker => {
					if let Err(e) = proof_submission_result {
						log::error!(
							target: "bridge",
							"Finality sync proof submission tx to {} has failed with error: {:?}.",
							P::TARGET_NAME,
							e,
						);
						self.best_submitted_number = None;
						e.fail_if_connection_error()?;
					}
				},
				_ = async_std::task::sleep(next_tick).fuse() => {},
				_ = exit_signal => return Ok(()),
			}
		}
	}

	pub async fn run(
		source_client: SC,
		target_client: TC,
		sync_params: FinalitySyncParams,
		metrics_sync: Option<SyncLoopMetrics>,
		exit_signal: impl Future<Output = ()>,
	) -> Result<(), FailedClient> {
		let mut finality_loop = Self::new(source_client, target_client, sync_params, metrics_sync);
		finality_loop.run_until_connection_lost(exit_signal).await
	}
}

async fn free_headers_interval<P: FinalitySyncPipeline>(
	target_client: &impl TargetClient<P>,
) -> Result<Option<P::Number>, FailedClient> {
	match target_client.free_source_headers_interval().await {
		Ok(Some(free_headers_interval)) if !free_headers_interval.is_zero() => {
			log::trace!(
				target: "bridge",
				"Free headers interval for {} headers at {} is: {:?}",
				P::SOURCE_NAME,
				P::TARGET_NAME,
				free_headers_interval,
			);
			Ok(Some(free_headers_interval))
		},
		Ok(Some(_free_headers_interval)) => {
			log::trace!(
				target: "bridge",
				"Free headers interval for {} headers at {} is zero. Not submitting any free headers",
				P::SOURCE_NAME,
				P::TARGET_NAME,
			);
			Ok(None)
		},
		Ok(None) => {
			log::trace!(
				target: "bridge",
				"Free headers interval for {} headers at {} is None. Not submitting any free headers",
				P::SOURCE_NAME,
				P::TARGET_NAME,
			);

			Ok(None)
		},
		Err(e) => {
			log::error!(
				target: "bridge",
				"Failed to read free headers interval for {} headers at {}: {:?}",
				P::SOURCE_NAME,
				P::TARGET_NAME,
				e,
			);
			Err(FailedClient::Target)
		},
	}
}

/// Run finality proofs synchronization loop.
pub async fn run<P: FinalitySyncPipeline>(
	source_client: impl SourceClient<P>,
	target_client: impl TargetClient<P>,
	sync_params: FinalitySyncParams,
	metrics_params: MetricsParams,
	exit_signal: impl Future<Output = ()> + 'static + Send,
) -> Result<(), relay_utils::Error> {
	let exit_signal = exit_signal.shared();
	relay_utils::relay_loop(source_client, target_client)
		.with_metrics(metrics_params)
		.loop_metric(SyncLoopMetrics::new(
			Some(&metrics_prefix::<P>()),
			"source",
			"source_at_target",
		)?)?
		.expose()
		.await?
		.run(metrics_prefix::<P>(), move |source_client, target_client, metrics| {
			FinalityLoop::run(
				source_client,
				target_client,
				sync_params.clone(),
				metrics,
				exit_signal.clone(),
			)
		})
		.await
}

#[cfg(test)]
mod tests {
	use super::*;

	use crate::mock::*;
	use futures::{FutureExt, StreamExt};
	use parking_lot::Mutex;
	use relay_utils::{FailedClient, HeaderId, TrackedTransactionStatus};
	use std::{collections::HashMap, sync::Arc};

	fn prepare_test_clients(
		exit_sender: futures::channel::mpsc::UnboundedSender<()>,
		state_function: impl Fn(&mut ClientsData) -> bool + Send + Sync + 'static,
		source_headers: HashMap<TestNumber, (TestSourceHeader, Option<TestFinalityProof>)>,
	) -> (TestSourceClient, TestTargetClient) {
		let internal_state_function: Arc<dyn Fn(&mut ClientsData) + Send + Sync> =
			Arc::new(move |data| {
				if state_function(data) {
					exit_sender.unbounded_send(()).unwrap();
				}
			});
		let clients_data = Arc::new(Mutex::new(ClientsData {
			source_best_block_number: 10,
			source_headers,
			source_proofs: vec![TestFinalityProof(12), TestFinalityProof(14)],

			target_best_block_id: HeaderId(5, 5),
			target_headers: vec![],
			target_transaction_tracker: TestTransactionTracker(
				TrackedTransactionStatus::Finalized(Default::default()),
			),
		}));
		(
			TestSourceClient {
				on_method_call: internal_state_function.clone(),
				data: clients_data.clone(),
			},
			TestTargetClient { on_method_call: internal_state_function, data: clients_data },
		)
	}

	fn test_sync_params() -> FinalitySyncParams {
		FinalitySyncParams {
			tick: Duration::from_secs(0),
			recent_finality_proofs_limit: 1024,
			stall_timeout: Duration::from_secs(1),
			headers_to_relay: HeadersToRelay::All,
		}
	}

	fn run_sync_loop(
		state_function: impl Fn(&mut ClientsData) -> bool + Send + Sync + 'static,
	) -> (ClientsData, Result<(), FailedClient>) {
		let (exit_sender, exit_receiver) = futures::channel::mpsc::unbounded();
		let (source_client, target_client) = prepare_test_clients(
			exit_sender,
			state_function,
			vec![
				(5, (TestSourceHeader(false, 5, 5), None)),
				(6, (TestSourceHeader(false, 6, 6), None)),
				(7, (TestSourceHeader(false, 7, 7), Some(TestFinalityProof(7)))),
				(8, (TestSourceHeader(true, 8, 8), Some(TestFinalityProof(8)))),
				(9, (TestSourceHeader(false, 9, 9), Some(TestFinalityProof(9)))),
				(10, (TestSourceHeader(false, 10, 10), None)),
			]
			.into_iter()
			.collect(),
		);
		let sync_params = test_sync_params();

		let clients_data = source_client.data.clone();
		let result = async_std::task::block_on(FinalityLoop::run(
			source_client,
			target_client,
			sync_params,
			None,
			exit_receiver.into_future().map(|(_, _)| ()),
		));

		let clients_data = clients_data.lock().clone();
		(clients_data, result)
	}

	#[test]
	fn finality_sync_loop_works() {
		let (client_data, result) = run_sync_loop(|data| {
			// header#7 has persistent finality proof, but it isn't mandatory => it isn't submitted,
			// because header#8 has persistent finality proof && it is mandatory => it is submitted
			// header#9 has persistent finality proof, but it isn't mandatory => it is submitted,
			// because   there are no more persistent finality proofs
			//
			// once this ^^^ is done, we generate more blocks && read proof for blocks 12 and 14
			// from the stream
			if data.target_best_block_id.0 == 9 {
				data.source_best_block_number = 14;
				data.source_headers.insert(11, (TestSourceHeader(false, 11, 11), None));
				data.source_headers
					.insert(12, (TestSourceHeader(false, 12, 12), Some(TestFinalityProof(12))));
				data.source_headers.insert(13, (TestSourceHeader(false, 13, 13), None));
				data.source_headers
					.insert(14, (TestSourceHeader(false, 14, 14), Some(TestFinalityProof(14))));
			}
			// once this ^^^ is done, we generate more blocks && read persistent proof for block 16
			if data.target_best_block_id.0 == 14 {
				data.source_best_block_number = 17;
				data.source_headers.insert(15, (TestSourceHeader(false, 15, 15), None));
				data.source_headers
					.insert(16, (TestSourceHeader(false, 16, 16), Some(TestFinalityProof(16))));
				data.source_headers.insert(17, (TestSourceHeader(false, 17, 17), None));
			}

			data.target_best_block_id.0 == 16
		});

		assert_eq!(result, Ok(()));
		assert_eq!(
			client_data.target_headers,
			vec![
				// before adding 11..14: finality proof for mandatory header#8
				(TestSourceHeader(true, 8, 8), TestFinalityProof(8)),
				// before adding 11..14: persistent finality proof for non-mandatory header#9
				(TestSourceHeader(false, 9, 9), TestFinalityProof(9)),
				// after adding 11..14: ephemeral finality proof for non-mandatory header#14
				(TestSourceHeader(false, 14, 14), TestFinalityProof(14)),
				// after adding 15..17: persistent finality proof for non-mandatory header#16
				(TestSourceHeader(false, 16, 16), TestFinalityProof(16)),
			],
		);
	}

	fn run_headers_to_relay_mode_test(
		headers_to_relay: HeadersToRelay,
		has_mandatory_headers: bool,
	) -> Option<JustifiedHeader<TestFinalitySyncPipeline>> {
		let (exit_sender, _) = futures::channel::mpsc::unbounded();
		let (source_client, target_client) = prepare_test_clients(
			exit_sender,
			|_| false,
			vec![
				(6, (TestSourceHeader(false, 6, 6), Some(TestFinalityProof(6)))),
				(7, (TestSourceHeader(false, 7, 7), Some(TestFinalityProof(7)))),
				(8, (TestSourceHeader(has_mandatory_headers, 8, 8), Some(TestFinalityProof(8)))),
				(9, (TestSourceHeader(false, 9, 9), Some(TestFinalityProof(9)))),
				(10, (TestSourceHeader(false, 10, 10), Some(TestFinalityProof(10)))),
			]
			.into_iter()
			.collect(),
		);
		async_std::task::block_on(async {
			let mut finality_loop = FinalityLoop::new(
				source_client,
				target_client,
				FinalitySyncParams {
					tick: Duration::from_secs(0),
					recent_finality_proofs_limit: 0,
					stall_timeout: Duration::from_secs(0),
					headers_to_relay,
				},
				None,
			);
			let info = SyncInfo {
				best_number_at_source: 10,
				best_number_at_target: 5,
				is_using_same_fork: true,
			};
			finality_loop.select_header_to_submit(&info, Some(3)).await.unwrap()
		})
	}

	#[test]
	fn select_header_to_submit_may_select_non_mandatory_header() {
		assert_eq!(run_headers_to_relay_mode_test(HeadersToRelay::Mandatory, false), None);
		assert_eq!(
			run_headers_to_relay_mode_test(HeadersToRelay::Free, false),
			Some(JustifiedHeader {
				header: TestSourceHeader(false, 10, 10),
				proof: TestFinalityProof(10)
			}),
		);
		assert_eq!(
			run_headers_to_relay_mode_test(HeadersToRelay::All, false),
			Some(JustifiedHeader {
				header: TestSourceHeader(false, 10, 10),
				proof: TestFinalityProof(10)
			}),
		);
	}

	#[test]
	fn select_header_to_submit_may_select_mandatory_header() {
		assert_eq!(
			run_headers_to_relay_mode_test(HeadersToRelay::Mandatory, true),
			Some(JustifiedHeader {
				header: TestSourceHeader(true, 8, 8),
				proof: TestFinalityProof(8)
			}),
		);
		assert_eq!(
			run_headers_to_relay_mode_test(HeadersToRelay::Free, true),
			Some(JustifiedHeader {
				header: TestSourceHeader(true, 8, 8),
				proof: TestFinalityProof(8)
			}),
		);
		assert_eq!(
			run_headers_to_relay_mode_test(HeadersToRelay::All, true),
			Some(JustifiedHeader {
				header: TestSourceHeader(true, 8, 8),
				proof: TestFinalityProof(8)
			}),
		);
	}

	#[test]
	fn different_forks_at_source_and_at_target_are_detected() {
		let (exit_sender, _exit_receiver) = futures::channel::mpsc::unbounded();
		let (source_client, target_client) = prepare_test_clients(
			exit_sender,
			|_| false,
			vec![
				(5, (TestSourceHeader(false, 5, 42), None)),
				(6, (TestSourceHeader(false, 6, 6), None)),
				(7, (TestSourceHeader(false, 7, 7), None)),
				(8, (TestSourceHeader(false, 8, 8), None)),
				(9, (TestSourceHeader(false, 9, 9), None)),
				(10, (TestSourceHeader(false, 10, 10), None)),
			]
			.into_iter()
			.collect(),
		);

		let metrics_sync = SyncLoopMetrics::new(None, "source", "target").unwrap();
		async_std::task::block_on(async {
			let mut finality_loop = FinalityLoop::new(
				source_client,
				target_client,
				test_sync_params(),
				Some(metrics_sync.clone()),
			);
			finality_loop.run_iteration(None).await.unwrap()
		});

		assert!(!metrics_sync.is_using_same_fork());
	}
}
