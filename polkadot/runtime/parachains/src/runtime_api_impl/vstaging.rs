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

//! Put implementations of functions from staging APIs here.

use crate::{configuration, initializer, shared};
use primitives::{
	vstaging::{ApprovalVotingParams, NodeFeatures},
	ValidatorIndex,
};
use sp_std::{collections::btree_map::BTreeMap, prelude::Vec};

/// Implementation for `DisabledValidators`
// CAVEAT: this should only be called on the node side
// as it might produce incorrect results on session boundaries
pub fn disabled_validators<T>() -> Vec<ValidatorIndex>
where
	T: pallet_session::Config + shared::Config,
{
	let shuffled_indices = <shared::Pallet<T>>::active_validator_indices();
	// mapping from raw validator index to `ValidatorIndex`
	// this computation is the same within a session, but should be cheap
	let reverse_index = shuffled_indices
		.iter()
		.enumerate()
		.map(|(i, v)| (v.0, ValidatorIndex(i as u32)))
		.collect::<BTreeMap<u32, ValidatorIndex>>();

	// we might have disabled validators who are not parachain validators
	<pallet_session::Pallet<T>>::disabled_validators()
		.iter()
		.filter_map(|v| reverse_index.get(v).cloned())
		.collect()
}

/// Returns the current state of the node features.
pub fn node_features<T: initializer::Config>() -> NodeFeatures {
	<configuration::Pallet<T>>::config().node_features
}

/// Approval voting subsystem configuration parameteres
pub fn approval_voting_params<T: initializer::Config>() -> ApprovalVotingParams {
	let config = <configuration::Pallet<T>>::config();
	config.approval_voting_params
}
