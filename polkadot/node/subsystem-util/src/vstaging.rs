// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Contains helpers for staging runtime calls.
//!
//! This module is intended to contain common boiler plate code handling unreleased runtime API
//! calls.

use std::collections::{BTreeMap, VecDeque};

use polkadot_node_subsystem_types::messages::{RuntimeApiMessage, RuntimeApiRequest};
use polkadot_overseer::SubsystemSender;
use polkadot_primitives::{CoreIndex, Hash, Id as ParaId, ScheduledCore, ValidatorIndex};

use crate::{has_required_runtime, request_claim_queue, request_disabled_validators, runtime};

const LOG_TARGET: &'static str = "parachain::subsystem-util-vstaging";

/// A snapshot of the runtime claim queue at an arbitrary relay chain block.
pub type ClaimQueueSnapshot = BTreeMap<CoreIndex, VecDeque<ParaId>>;

// TODO: https://github.com/paritytech/polkadot-sdk/issues/1940
/// Returns disabled validators list if the runtime supports it. Otherwise logs a debug messages and
/// returns an empty vec.
/// Once runtime ver `DISABLED_VALIDATORS_RUNTIME_REQUIREMENT` is released remove this function and
/// replace all usages with `request_disabled_validators`
pub async fn get_disabled_validators_with_fallback<Sender: SubsystemSender<RuntimeApiMessage>>(
	sender: &mut Sender,
	relay_parent: Hash,
) -> Result<Vec<ValidatorIndex>, runtime::Error> {
	let disabled_validators = if has_required_runtime(
		sender,
		relay_parent,
		RuntimeApiRequest::DISABLED_VALIDATORS_RUNTIME_REQUIREMENT,
	)
	.await
	{
		request_disabled_validators(relay_parent, sender)
			.await
			.await
			.map_err(runtime::Error::RuntimeRequestCanceled)??
	} else {
		gum::debug!(target: LOG_TARGET, "Runtime doesn't support `DisabledValidators` - continuing with an empty disabled validators set");
		vec![]
	};

	Ok(disabled_validators)
}

/// Checks if the runtime supports `request_claim_queue` and attempts to fetch the claim queue.
/// Returns `ClaimQueueSnapshot` or `None` if claim queue API is not supported by runtime.
/// Any specific [`RuntimeApiError`]s are bubbled up to the caller.
pub async fn fetch_claim_queue(
	sender: &mut impl SubsystemSender<RuntimeApiMessage>,
	relay_parent: Hash,
) -> Result<Option<ClaimQueueSnapshot>, runtime::Error> {
	if has_required_runtime(
		sender,
		relay_parent,
		RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT,
	)
	.await
	{
		let res = request_claim_queue(relay_parent, sender)
			.await
			.await
			.map_err(runtime::Error::RuntimeRequestCanceled)??;
		Ok(Some(res))
	} else {
		gum::trace!(target: LOG_TARGET, "Runtime doesn't support `request_claim_queue`");
		Ok(None)
	}
}

/// Returns the next scheduled `ParaId` for a core in the claim queue, wrapped in `ScheduledCore`.
pub fn fetch_next_scheduled_on_core(
	claim_queue: &ClaimQueueSnapshot,
	core_idx: CoreIndex,
) -> Option<ScheduledCore> {
	claim_queue
		.get(&core_idx)?
		.front()
		.cloned()
		.map(|para_id| ScheduledCore { para_id, collator: None })
}
