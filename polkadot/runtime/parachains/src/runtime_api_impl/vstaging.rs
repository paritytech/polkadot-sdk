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

use crate::scheduler;
use primitives::{CoreIndex, Id as ParaId};
use sp_std::collections::{btree_map::BTreeMap, vec_deque::VecDeque};

/// Returns the claimqueue from the scheduler
pub fn claim_queue<T: scheduler::Config>() -> BTreeMap<CoreIndex, VecDeque<ParaId>> {
	<scheduler::Pallet<T>>::claimqueue()
		.into_iter()
		.map(|(core_index, entries)| {
			(core_index, entries.into_iter().map(|e| e.para_id()).collect())
		})
		.collect()
}
