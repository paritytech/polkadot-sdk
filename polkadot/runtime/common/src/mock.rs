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

//! Mocking utilities for testing.

use crate::traits::Registrar;
use codec::{Decode, Encode};
use frame_support::{dispatch::DispatchResult, weights::Weight};
use polkadot_primitives::{
	HeadData, Id as ParaId, PvfCheckStatement, SessionIndex, ValidationCode,
};
use polkadot_runtime_parachains::paras;
use sp_keyring::Sr25519Keyring;
use sp_runtime::{DispatchError, Permill};
use std::{cell::RefCell, collections::HashMap};

thread_local! {
	static PARACHAINS: RefCell<Vec<ParaId>> = RefCell::new(Vec::new());
	static LOCKS: RefCell<HashMap<ParaId, bool>> = RefCell::new(HashMap::new());
	static MANAGERS: RefCell<HashMap<ParaId, Vec<u8>>> = RefCell::new(HashMap::new());
}

pub struct TestRegistrar<T>(core::marker::PhantomData<T>);

impl<T: frame_system::Config> Registrar for TestRegistrar<T> {
	type AccountId = T::AccountId;

	fn manager_of(id: ParaId) -> Option<Self::AccountId> {
		MANAGERS.with(|x| x.borrow().get(&id).and_then(|v| T::AccountId::decode(&mut &v[..]).ok()))
	}

	fn parachains() -> Vec<ParaId> {
		PARACHAINS.with(|x| x.borrow().clone())
	}

	fn apply_lock(id: ParaId) {
		LOCKS.with(|x| x.borrow_mut().insert(id, true));
	}

	fn remove_lock(id: ParaId) {
		LOCKS.with(|x| x.borrow_mut().insert(id, false));
	}

	fn register(
		manager: Self::AccountId,
		id: ParaId,
		_genesis_head: HeadData,
		_validation_code: ValidationCode,
	) -> DispatchResult {
		// Every registered para is a parachain.
		PARACHAINS.with(|x| {
			let mut parachains = x.borrow_mut();
			match parachains.binary_search(&id) {
				Ok(_) => Err(DispatchError::Other("Already registered")),
				Err(i) => {
					parachains.insert(i, id);
					Ok(())
				},
			}
		})?;
		MANAGERS.with(|x| x.borrow_mut().insert(id, manager.encode()));
		Ok(())
	}

	fn deregister(id: ParaId) -> DispatchResult {
		PARACHAINS.with(|x| {
			let mut parachains = x.borrow_mut();
			match parachains.binary_search(&id) {
				Ok(i) => {
					parachains.remove(i);
					Ok(())
				},
				Err(_) => Err(DispatchError::Other("not registered, cannot `deregister`")),
			}
		})?;
		MANAGERS.with(|x| x.borrow_mut().remove(&id));
		Ok(())
	}

	/// All registered paras are already parachains, so this is a no-op that mirrors the production
	/// registrar.
	fn make_parachain(_id: ParaId) -> DispatchResult {
		Ok(())
	}

	/// Downgrading to a parathread no longer changes any lifecycle (all paras stay parachains), so
	/// this is a no-op that mirrors the production registrar.
	fn make_parathread(_id: ParaId) -> DispatchResult {
		Ok(())
	}

	#[cfg(test)]
	fn worst_head_data() -> HeadData {
		vec![0u8; 1000].into()
	}

	#[cfg(test)]
	fn worst_validation_code() -> ValidationCode {
		let validation_code = vec![0u8; 1000];
		validation_code.into()
	}

	#[cfg(test)]
	fn execute_pending_transitions() {}
}

impl<T: frame_system::Config> TestRegistrar<T> {
	#[allow(dead_code)]
	pub fn parachains() -> Vec<ParaId> {
		PARACHAINS.with(|x| x.borrow().clone())
	}

	#[allow(dead_code)]
	pub fn clear_storage() {
		PARACHAINS.with(|x| x.borrow_mut().clear());
		MANAGERS.with(|x| x.borrow_mut().clear());
	}
}

/// A very dumb implementation of `EstimateNextSessionRotation`. At the moment of writing, this
/// is more to satisfy type requirements rather than to test anything.
pub struct TestNextSessionRotation;

impl frame_support::traits::EstimateNextSessionRotation<u32> for TestNextSessionRotation {
	fn average_session_length() -> u32 {
		10
	}

	fn estimate_current_session_progress(_now: u32) -> (Option<Permill>, Weight) {
		(None, Weight::zero())
	}

	fn estimate_next_session_rotation(_now: u32) -> (Option<u32>, Weight) {
		(None, Weight::zero())
	}
}

pub fn validators_public_keys(
	validators: &[Sr25519Keyring],
) -> Vec<polkadot_primitives::ValidatorId> {
	validators.iter().map(|v| v.public().into()).collect()
}

pub fn conclude_pvf_checking<T: paras::Config>(
	validation_code: &ValidationCode,
	validators: &[Sr25519Keyring],
	session_index: SessionIndex,
) {
	let num_required = polkadot_primitives::supermajority_threshold(validators.len());
	validators.iter().enumerate().take(num_required).for_each(|(idx, key)| {
		let validator_index = idx as u32;
		let statement = PvfCheckStatement {
			accept: true,
			subject: validation_code.hash(),
			session_index,
			validator_index: validator_index.into(),
		};
		let signature = key.sign(&statement.signing_payload());
		let _ = paras::Pallet::<T>::include_pvf_check_statement(
			frame_system::Origin::<T>::None.into(),
			statement,
			signature.into(),
		);
	});
}
