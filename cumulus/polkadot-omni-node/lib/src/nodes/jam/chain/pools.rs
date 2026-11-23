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

//! Which cores hold this para's authorizer, read off JAM's authorizer pools.

use crate::nodes::jam::LOG_TARGET;
use cumulus_client_jam_authorizer::authorizer::AuraAuthorizer;
use jam_interface::{AuthPool, AuthorizerHash, CoreIndex, HeaderHash, JamStateSource};
use std::{fmt::Debug, time::Instant};

/// Which cores currently hold this para's authorizer, and therefore where its packages may go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PoolScan {
	/// The core the first package of a turn goes to. `None` means no core holds the para's
	/// authorizer at all — the builder keeps authoring, but nothing may be submitted.
	pub target: Option<CoreIndex>,
	/// The cores the following packages of the same turn go to, in the order the scan found
	/// them. A turn puts one package per core, so its packages land on different cores of the
	/// same JAM block — and a core that silently held our packages would otherwise be invisible.
	pub also_on: Vec<CoreIndex>,
}

impl PoolScan {
	/// The core the `package`-th package of a turn goes to: the lowest-indexed core first, then
	/// the others in the order the scan found them. `None` means the turn has no core left.
	///
	/// One package per core is what makes a turn elastic: each package lands on a core of its
	/// own, all in the same JAM block, and every package after the first chains on the previous
	/// one's block.
	pub(crate) fn core_for_package(&self, package: usize) -> Option<CoreIndex> {
		match package {
			0 => self.target,
			_ => self.also_on.get(package - 1).copied(),
		}
	}

	/// How many cores hold this para's authorizer, and therefore how many packages one turn may
	/// put on different cores.
	pub(crate) fn core_count(&self) -> usize {
		self.target.map_or(0, |_| 1 + self.also_on.len())
	}
}

/// Find the cores whose authorizer pool holds `wanted`, lowest index first.
///
/// The pool rather than the queue: our cores are dedicated and a queue holds 80 copies of one
/// hash, so the pool converges to 8 copies of it and a scan sees an assignment in the very block
/// it lands. It also drains for 8 blocks after a core is freed, which is the handover overlap
/// that keeps packages already in flight valid.
pub(crate) fn scan_pools<'a>(
	pools: impl IntoIterator<Item = &'a AuthPool>,
	wanted: AuthorizerHash,
) -> PoolScan {
	let mut holders = pools
		.into_iter()
		.enumerate()
		.filter(|(_, pool)| pool.contains(&wanted))
		.map(|(core, _)| core as CoreIndex);
	PoolScan { target: holders.next(), also_on: holders.collect() }
}

/// Read the authorizer pools at `anchor` and work out where this para's packages belong.
pub(crate) async fn scan_pools_at<Jam: JamStateSource + ?Sized>(
	jam: &Jam,
	anchor: HeaderHash,
	authorizer: &AuraAuthorizer,
) -> Result<PoolScan, String> {
	let wanted = authorizer.hash();
	let started = Instant::now();
	let pools = jam
		.auth_pools(anchor)
		.await
		.map_err(|error| format!("authorizer pools at {anchor:?}: {error}"))?;
	let elapsed_ms = started.elapsed().as_millis();

	for (core, pool) in pools.iter().enumerate() {
		tracing::debug!(
			target: LOG_TARGET,
			method = "stateValue",
			key = "C(1) AuthPools",
			at = ?anchor,
			core,
			pool_len = pool.len(),
			holds_ours = pool.contains(&wanted),
			copies_of_ours = pool.iter().filter(|hash| **hash == wanted).count(),
			?pool,
			elapsed_ms,
			"JAM read: one core's authorizer pool.",
		);
	}

	let scan = scan_pools(pools.iter(), wanted);
	tracing::debug!(
		target: LOG_TARGET,
		at = ?anchor,
		authorizer_hash = ?wanted,
		cores = pools.len(),
		target = ?scan.target,
		also_on = ?scan.also_on,
		elapsed_ms,
		"Scanned the authorizer pools for this para's authorizer.",
	);
	Ok(scan)
}
