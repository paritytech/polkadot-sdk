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

use crate::{disputes, initializer, paras};
use alloc::vec::Vec;

use polkadot_primitives::{slashing, CandidateHash, Id as ParaId, SessionIndex};

/// Implementation of `para_ids` runtime API
pub fn para_ids<T: initializer::Config>() -> Vec<ParaId> {
	paras::Heads::<T>::iter_keys().collect()
}

/// Implementation of `unapplied_slashes_v2` runtime API
pub fn unapplied_slashes_v2<T: disputes::slashing::Config>(
) -> Vec<(SessionIndex, CandidateHash, slashing::PendingSlashes)> {
	disputes::slashing::Pallet::<T>::unapplied_slashes()
}
