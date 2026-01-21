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

use crate::View;
use polkadot_primitives::{BlockNumber, Hash, ValidatorIndex};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ApprovalsStats {
	pub votes: BTreeMap<ValidatorIndex, u32>,
	pub no_shows: BTreeMap<ValidatorIndex, u32>,
}

pub fn handle_candidate_approved(
	view: &mut View,
	block_hash: Hash,
	block_number: BlockNumber,
	approvals: Vec<ValidatorIndex>,
) {
	view.per_relay.entry((block_hash, block_number)).and_modify(|relay_view| {
		for validator_index in approvals {
			relay_view
				.approvals_stats
				.votes
				.entry(validator_index)
				.and_modify(|count| *count = count.saturating_add(1))
				.or_insert(1);
		}
	});
}

pub fn handle_observed_no_shows(
	view: &mut View,
	block_hash: Hash,
	block_number: BlockNumber,
	no_show_validators: Vec<ValidatorIndex>,
) {
	view.per_relay.entry((block_hash, block_number)).and_modify(|relay_view| {
		for validator_index in no_show_validators {
			relay_view
				.approvals_stats
				.no_shows
				.entry(validator_index)
				.and_modify(|count| *count = count.saturating_add(1))
				.or_insert(1);
		}
	});
}
