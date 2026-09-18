// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! JAM work-package bundle recovery.
//!
//! A JAM collator that did not author a block cannot receive it via gossip. This crate
//! provides the recovery engine: it schedules randomised recoveries from the DA layer via
//! the JAM RPC and returns the recovered bundle bytes to the caller for decoding and import.
//!
//! Modelled on `cumulus-client-pov-recovery`.

pub mod bundle_decode;
pub mod state_machine;

pub use bundle_decode::{bundle_work_package_hash, decode_bundle};
pub use state_machine::{
	ImportBlocksSink, JamWorkPackageRecovery, RecoveredBlock, RecoveredHashFn,
	WorkReportNotification,
};

use cumulus_jam_interface::{EpochIndex, JamWorkPackageSubmission, WorkReportHash};
use futures::{stream::FuturesUnordered, Future, FutureExt, StreamExt};
use futures_timer::Delay;
use rand::{distributions::Uniform, prelude::Distribution, thread_rng};
use std::{collections::VecDeque, pin::Pin, time::Duration};

const LOG_TARGET: &str = "cumulus-jam-work-package-recovery";

/// The delay between observing an unknown work-report and triggering bundle recovery.
/// Randomising the start within this interval prevents self-DOS when multiple
/// non-authoring collators trigger recovery simultaneously.
#[derive(Clone, Copy)]
pub struct RecoveryDelayRange {
	/// Start recovering after at least this delay.
	pub min: Duration,
	/// Start recovering before this delay has elapsed.
	pub max: Duration,
}

impl RecoveryDelayRange {
	/// Sample a randomised duration uniformly in `[min, max]`.
	fn duration(&self) -> Duration {
		Uniform::from(self.min..=self.max).sample(&mut thread_rng())
	}
}

/// Queue deciding when each recovery attempt fires.
///
/// Each call to `push_recovery` schedules a timer. When the timer fires,
/// `next_recovery` returns the corresponding hash in FIFO order.
pub struct RecoveryQueue {
	recovery_delay_range: RecoveryDelayRange,
	/// Hashes waiting to be recovered, in submission order.
	recovery_queue: VecDeque<WorkReportHash>,
	/// Futures that complete when the delay for each queued hash has elapsed.
	signaling_queue: FuturesUnordered<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

impl RecoveryQueue {
	/// Create a new queue with the given delay range.
	pub fn new(recovery_delay_range: RecoveryDelayRange) -> Self {
		Self {
			recovery_delay_range,
			recovery_queue: Default::default(),
			signaling_queue: Default::default(),
		}
	}

	/// Enqueue `hash` for recovery; a slot fires after a randomised delay.
	pub fn push_recovery(&mut self, hash: WorkReportHash) {
		let delay = self.recovery_delay_range.duration();
		tracing::debug!(
			target: LOG_TARGET,
			?hash,
			"Queuing work-report for recovery in {:?}",
			delay,
		);
		self.recovery_queue.push_back(hash);
		self.signaling_queue.push(async move { Delay::new(delay).await }.boxed());
	}

	/// Wait for the next recovery slot and return the hash.
	pub async fn next_recovery(&mut self) -> WorkReportHash {
		loop {
			if self.signaling_queue.next().await.is_some() {
				if let Some(hash) = self.recovery_queue.pop_front() {
					return hash;
				} else {
					tracing::error!(
						target: LOG_TARGET,
						"Recovery signaled but no hash available — this is a bug.",
					);
				}
			}
			futures::pending!()
		}
	}
}

/// Abstract over DA-layer bundle retrieval.
///
/// `RpcBundleRecovery` is the production implementation, calling the JAM node's
/// `recoverBundle` RPC. Tests supply a mock.
#[async_trait::async_trait]
pub trait JamBundleRecovery: Send {
	/// Recover the encoded bundle for the work report `report_hash` guaranteed in
	/// `assurance_epoch`. Returns `None` if the bundle is not yet available in the DA
	/// layer (caller should retry), or `Err` on a non-retriable failure.
	async fn recover_bundle(
		&mut self,
		report_hash: WorkReportHash,
		assurance_epoch: EpochIndex,
	) -> Result<Option<Vec<u8>>, String>;
}

/// `JamBundleRecovery` backed by a JAM node connection implementing
/// `JamWorkPackageSubmission`.
pub struct RpcBundleRecovery<J> {
	jam_interface: J,
}

impl<J: JamWorkPackageSubmission> RpcBundleRecovery<J> {
	/// Wrap `jam_interface` to serve `JamBundleRecovery`.
	pub fn new(jam_interface: J) -> Self {
		Self { jam_interface }
	}
}

#[async_trait::async_trait]
impl<J: JamWorkPackageSubmission> JamBundleRecovery for RpcBundleRecovery<J> {
	async fn recover_bundle(
		&mut self,
		report_hash: WorkReportHash,
		assurance_epoch: EpochIndex,
	) -> Result<Option<Vec<u8>>, String> {
		self.jam_interface
			.recover_bundle(report_hash, assurance_epoch)
			.await
			.map(Some)
			.map_err(|e| e.to_string())
	}
}

#[cfg(test)]
mod tests;
