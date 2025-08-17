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

//! Runtime modules for parachains code.
//!
//! It is crucial to include all the modules from this crate in the runtime, in
//! particular the `Initializer` module, as it is responsible for initializing the state
//! of the other modules.

#![cfg_attr(feature = "runtime-benchmarks", recursion_limit = "256")]
#![cfg_attr(not(feature = "std"), no_std)]

pub mod assigner_coretime;
pub mod configuration;
pub mod coretime;
pub mod disputes;
pub mod dmp;
pub mod hrmp;
pub mod inclusion;
pub mod initializer;
pub mod metrics;
pub mod on_demand;
pub mod origin;
pub mod paras;
pub mod paras_inherent;
pub mod reward_points;
pub mod scheduler;
pub mod session_info;
pub mod shared;

pub mod runtime_api_impl;

mod util;

#[cfg(any(feature = "runtime-benchmarks", test))]
mod builder;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod ump_tests;

extern crate alloc;

pub use origin::{ensure_parachain, Origin};
pub use paras::{ParaLifecycle, UpgradeStrategy};
use polkadot_primitives::{HeadData, Id as ParaId, ValidationCode};
use sp_arithmetic::traits::Saturating;
use sp_runtime::{traits::Get, DispatchResult, FixedU128};

/// Trait for tracking message delivery fees on a transport protocol.
pub trait FeeTracker {
	/// Type used for assigning different fee factors to different destinations
	type Id: Copy;

	/// Minimal delivery fee factor.
	const MIN_FEE_FACTOR: FixedU128 = FixedU128::from_u32(1);
	/// The factor that is used to increase the current message fee factor when the transport
	/// protocol is experiencing some lags.
	const EXPONENTIAL_FEE_BASE: FixedU128 = FixedU128::from_rational(105, 100); // 1.05
	/// The factor that is used to increase the current message fee factor for every sent kilobyte.
	const MESSAGE_SIZE_FEE_BASE: FixedU128 = FixedU128::from_rational(1, 1000); // 0.001

	/// Returns the current message fee factor.
	fn get_fee_factor(id: Self::Id) -> FixedU128;

	/// Sets the current message fee factor.
	fn set_fee_factor(id: Self::Id, val: FixedU128);

	fn do_increase_fee_factor(fee_factor: &mut FixedU128, message_size: u128) {
		let message_size_factor = FixedU128::from(message_size.saturating_div(1024))
			.saturating_mul(Self::MESSAGE_SIZE_FEE_BASE);
		*fee_factor = fee_factor
			.saturating_mul(Self::EXPONENTIAL_FEE_BASE.saturating_add(message_size_factor));
	}

	/// Increases the delivery fee factor by a factor based on message size and records the result.
	fn increase_fee_factor(id: Self::Id, message_size: u128) {
		let mut fee_factor = Self::get_fee_factor(id);
		Self::do_increase_fee_factor(&mut fee_factor, message_size);
		Self::set_fee_factor(id, fee_factor);
	}

	fn do_decrease_fee_factor(fee_factor: &mut FixedU128) -> bool {
		const { assert!(Self::EXPONENTIAL_FEE_BASE.into_inner() >= FixedU128::from_u32(1).into_inner()) }

		if *fee_factor == Self::MIN_FEE_FACTOR {
			return false;
		}

		// This should never lead to a panic because of the static assert above.
		*fee_factor = Self::MIN_FEE_FACTOR.max(*fee_factor / Self::EXPONENTIAL_FEE_BASE);
		true
	}

	/// Decreases the delivery fee factor by a constant factor and records the result.
	///
	/// Does not reduce the fee factor below the initial value, which is currently set as 1.
	///
	/// Returns `true` if the fee factor was actually decreased, `false` otherwise.
	fn decrease_fee_factor(id: Self::Id) -> bool {
		let mut fee_factor = Self::get_fee_factor(id);
		let res = Self::do_decrease_fee_factor(&mut fee_factor);
		Self::set_fee_factor(id, fee_factor);
		res
	}
}

/// Helper struct used for accessing `FeeTracker::MIN_FEE_FACTOR`
pub struct GetMinFeeFactor<T>(core::marker::PhantomData<T>);

impl<T: FeeTracker> Get<FixedU128> for GetMinFeeFactor<T> {
	fn get() -> FixedU128 {
		T::MIN_FEE_FACTOR
	}
}

/// Schedule a para to be initialized at the start of the next session with the given genesis data.
pub fn schedule_para_initialize<T: paras::Config>(
	id: ParaId,
	genesis: paras::ParaGenesisArgs,
) -> Result<(), ()> {
	paras::Pallet::<T>::schedule_para_initialize(id, genesis).map_err(|_| ())
}

/// Schedule a para to be cleaned up at the start of the next session.
pub fn schedule_para_cleanup<T: paras::Config>(id: polkadot_primitives::Id) -> Result<(), ()> {
	paras::Pallet::<T>::schedule_para_cleanup(id).map_err(|_| ())
}

/// Schedule a parathread (on-demand parachain) to be upgraded to a lease holding parachain.
pub fn schedule_parathread_upgrade<T: paras::Config>(id: ParaId) -> Result<(), ()> {
	paras::Pallet::<T>::schedule_parathread_upgrade(id).map_err(|_| ())
}

/// Schedule a lease holding parachain to be downgraded to an on-demand parachain.
pub fn schedule_parachain_downgrade<T: paras::Config>(id: ParaId) -> Result<(), ()> {
	paras::Pallet::<T>::schedule_parachain_downgrade(id).map_err(|_| ())
}

/// Schedules a validation code upgrade to a parachain with the given id.
pub fn schedule_code_upgrade<T: paras::Config>(
	id: ParaId,
	new_code: ValidationCode,
	set_go_ahead: UpgradeStrategy,
) -> DispatchResult {
	paras::Pallet::<T>::schedule_code_upgrade_external(id, new_code, set_go_ahead)
}

/// Sets the current parachain head with the given id.
pub fn set_current_head<T: paras::Config>(id: ParaId, new_head: HeadData) {
	paras::Pallet::<T>::set_current_head(id, new_head)
}

/// Ensure more initialization for `ParaId` when benchmarking. (e.g. open HRMP channels, ...)
#[cfg(feature = "runtime-benchmarks")]
pub trait EnsureForParachain {
	fn ensure(para_id: ParaId);
}

#[cfg(feature = "runtime-benchmarks")]
#[impl_trait_for_tuples::impl_for_tuples(30)]
impl EnsureForParachain for Tuple {
	fn ensure(para: ParaId) {
		for_tuples!( #(
			Tuple::ensure(para);
		)* );
	}
}
