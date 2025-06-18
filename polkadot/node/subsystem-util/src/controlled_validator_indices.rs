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

//! `ControlledValidatorIndices` implementation.

use polkadot_primitives::{IndexedVec, SessionIndex, ValidatorId, ValidatorIndex, ValidatorPair};
use schnellru::{ByLength, LruMap};
use sp_application_crypto::{AppCrypto, ByteArray};
use sp_keystore::{Keystore, KeystorePtr};
use std::collections::HashSet;

/// Keeps track of the validator indices controlled by the local validator in a given session. For
/// better performance, the values for each session are cached.
pub struct ControlledValidatorIndices {
	/// The indices of the controlled validators, cached by session.
	controlled_validator_indices: LruMap<SessionIndex, HashSet<ValidatorIndex>>,
	keystore: KeystorePtr,
}

impl ControlledValidatorIndices {
	/// Create a new instance of `ControlledValidatorIndices`.
	pub fn new(keystore: KeystorePtr, cache_size: u32) -> Self {
		let controlled_validator_indices = LruMap::new(ByLength::new(cache_size));
		Self { controlled_validator_indices, keystore }
	}

	/// Get the controlled validator indices for a given session. If the indices are not known they
	/// will be fetched from `session_validators` and cached.
	pub fn get(
		&mut self,
		session: SessionIndex,
		session_validators: &IndexedVec<ValidatorIndex, ValidatorId>,
	) -> &HashSet<ValidatorIndex> {
		self.controlled_validator_indices
			.get_or_insert(session, || {
				Self::find_controlled_validator_indices(&self.keystore, session_validators)
			})
			.expect("We just inserted the controlled indices; qed")
	}

	/// Find indices controlled by this validator.
	///
	/// That is all `ValidatorIndex`es we have private keys for. Usually this will only be one.
	fn find_controlled_validator_indices(
		keystore: &KeystorePtr,
		validators: &IndexedVec<ValidatorIndex, ValidatorId>,
	) -> HashSet<ValidatorIndex> {
		validators
			.iter()
			.enumerate()
			.filter(|(_, validator)| {
				Keystore::has_keys(keystore, &[(validator.to_raw_vec(), ValidatorPair::ID)])
			})
			.map(|(index, _)| ValidatorIndex(index as _))
			.collect()
	}
}
