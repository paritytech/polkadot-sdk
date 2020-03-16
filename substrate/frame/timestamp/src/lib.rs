// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! # Timestamp Module
//!
//! The Timestamp module provides functionality to get and set the on-chain time.
//!
//! - [`timestamp::Trait`](./trait.Trait.html)
//! - [`Call`](./enum.Call.html)
//! - [`Module`](./struct.Module.html)
//!
//! ## Overview
//!
//! The Timestamp module allows the validators to set and validate a timestamp with each block.
//!
//! It uses inherents for timestamp data, which is provided by the block author and validated/verified
//! by other validators. The timestamp can be set only once per block and must be set each block.
//! There could be a constraint on how much time must pass before setting the new timestamp.
//!
//! **NOTE:** The Timestamp module is the recommended way to query the on-chain time instead of using
//! an approach based on block numbers. The block number based time measurement can cause issues
//! because of cumulative calculation errors and hence should be avoided.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! * `set` - Sets the current time.
//!
//! ### Public functions
//!
//! * `get` - Gets the current time for the current block. If this function is called prior to
//! setting the timestamp, it will return the timestamp of the previous block.
//!
//! ### Trait Getters
//!
//! * `MinimumPeriod` - Gets the minimum (and advised) period between blocks for the chain.
//!
//! ## Usage
//!
//! The following example shows how to use the Timestamp module in your custom module to query the current timestamp.
//!
//! ### Prerequisites
//!
//! Import the Timestamp module into your custom module and derive the module configuration
//! trait from the timestamp trait.
//!
//! ### Get current timestamp
//!
//! ```
//! use frame_support::{decl_module, dispatch};
//! # use pallet_timestamp as timestamp;
//! use frame_system::{self as system, ensure_signed};
//!
//! pub trait Trait: timestamp::Trait {}
//!
//! decl_module! {
//! 	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
//! 		pub fn get_time(origin) -> dispatch::DispatchResult {
//! 			let _sender = ensure_signed(origin)?;
//! 			let _now = <timestamp::Module<T>>::get();
//! 			Ok(())
//! 		}
//! 	}
//! }
//! # fn main() {}
//! ```
//!
//! ### Example from the FRAME
//!
//! The [Session module](https://github.com/paritytech/substrate/blob/master/frame/session/src/lib.rs) uses
//! the Timestamp module for session management.
//!
//! ## Related Modules
//!
//! * [Session](../pallet_session/index.html)

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use sp_std::{result, cmp};
use sp_inherents::{ProvideInherent, InherentData, InherentIdentifier};
use frame_support::{Parameter, decl_storage, decl_module};
use frame_support::traits::{Time, Get};
use sp_runtime::{
	RuntimeString,
	traits::{
		AtLeast32Bit, Zero, SaturatedConversion, Scale
	}
};
use frame_support::weights::SimpleDispatchInfo;
use frame_system::ensure_none;
use sp_timestamp::{
	InherentError, INHERENT_IDENTIFIER, InherentType,
	OnTimestampSet,
};

/// The module configuration trait
pub trait Trait: frame_system::Trait {
	/// Type used for expressing timestamp.
	type Moment: Parameter + Default + AtLeast32Bit
		+ Scale<Self::BlockNumber, Output = Self::Moment> + Copy;

	/// Something which can be notified when the timestamp is set. Set this to `()` if not needed.
	type OnTimestampSet: OnTimestampSet<Self::Moment>;

	/// The minimum period between blocks. Beware that this is different to the *expected* period
	/// that the block production apparatus provides. Your chosen consensus system will generally
	/// work with this to determine a sensible block time. e.g. For Aura, it will be double this
	/// period on default settings.
	type MinimumPeriod: Get<Self::Moment>;
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		/// The minimum period between blocks. Beware that this is different to the *expected* period
		/// that the block production apparatus provides. Your chosen consensus system will generally
		/// work with this to determine a sensible block time. e.g. For Aura, it will be double this
		/// period on default settings.
		const MinimumPeriod: T::Moment = T::MinimumPeriod::get();

		/// Set the current time.
		///
		/// This call should be invoked exactly once per block. It will panic at the finalization
		/// phase, if this call hasn't been invoked by that time.
		///
		/// The timestamp should be greater than the previous one by the amount specified by
		/// `MinimumPeriod`.
		///
		/// The dispatch origin for this call must be `Inherent`.
		#[weight = SimpleDispatchInfo::FixedOperational(10_000)]
		fn set(origin, #[compact] now: T::Moment) {
			ensure_none(origin)?;
			assert!(!<Self as Store>::DidUpdate::exists(), "Timestamp must be updated only once in the block");
			assert!(
				Self::now().is_zero() || now >= Self::now() + T::MinimumPeriod::get(),
				"Timestamp must increment by at least <MinimumPeriod> between sequential blocks"
			);
			<Self as Store>::Now::put(now);
			<Self as Store>::DidUpdate::put(true);

			<T::OnTimestampSet as OnTimestampSet<_>>::on_timestamp_set(now);
		}

		fn on_finalize() {
			assert!(<Self as Store>::DidUpdate::take(), "Timestamp must be updated once in the block");
		}
	}
}

decl_storage! {
	trait Store for Module<T: Trait> as Timestamp {
		/// Current time for the current block.
		pub Now get(fn now) build(|_| 0.into()): T::Moment;

		/// Did the timestamp get updated in this block?
		DidUpdate: bool;
	}
}

impl<T: Trait> Module<T> {
	/// Get the current time for the current block.
	///
	/// NOTE: if this function is called prior to setting the timestamp,
	/// it will return the timestamp of the previous block.
	pub fn get() -> T::Moment {
		Self::now()
	}

	/// Set the timestamp to something in particular. Only used for tests.
	#[cfg(feature = "std")]
	pub fn set_timestamp(now: T::Moment) {
		<Self as Store>::Now::put(now);
	}
}

fn extract_inherent_data(data: &InherentData) -> Result<InherentType, RuntimeString> {
	data.get_data::<InherentType>(&INHERENT_IDENTIFIER)
		.map_err(|_| RuntimeString::from("Invalid timestamp inherent data encoding."))?
		.ok_or_else(|| "Timestamp inherent data is not provided.".into())
}

impl<T: Trait> ProvideInherent for Module<T> {
	type Call = Call<T>;
	type Error = InherentError;
	const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

	fn create_inherent(data: &InherentData) -> Option<Self::Call> {
		let data: T::Moment = extract_inherent_data(data)
			.expect("Gets and decodes timestamp inherent data")
			.saturated_into();

		let next_time = cmp::max(data, Self::now() + T::MinimumPeriod::get());
		Some(Call::set(next_time.into()))
	}

	fn check_inherent(call: &Self::Call, data: &InherentData) -> result::Result<(), Self::Error> {
		const MAX_TIMESTAMP_DRIFT_MILLIS: u64 = 30 * 1000;

		let t: u64 = match call {
			Call::set(ref t) => t.clone().saturated_into::<u64>(),
			_ => return Ok(()),
		};

		let data = extract_inherent_data(data).map_err(|e| InherentError::Other(e))?;

		let minimum = (Self::now() + T::MinimumPeriod::get()).saturated_into::<u64>();
		if t > data + MAX_TIMESTAMP_DRIFT_MILLIS {
			Err(InherentError::Other("Timestamp too far in future to accept".into()))
		} else if t < minimum {
			Err(InherentError::ValidAtTimestamp(minimum))
		} else {
			Ok(())
		}
	}
}

impl<T: Trait> Time for Module<T> {
	type Moment = T::Moment;

	/// Before the first set of now with inherent the value returned is zero.
	fn now() -> Self::Moment {
		Self::now()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use frame_support::{impl_outer_origin, assert_ok, parameter_types, weights::Weight};
	use sp_io::TestExternalities;
	use sp_core::H256;
	use sp_runtime::{Perbill, traits::{BlakeTwo256, IdentityLookup}, testing::Header};

	impl_outer_origin! {
		pub enum Origin for Test  where system = frame_system {}
	}

	#[derive(Clone, Eq, PartialEq)]
	pub struct Test;
	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub const MaximumBlockWeight: Weight = 1024;
		pub const MaximumBlockLength: u32 = 2 * 1024;
		pub const AvailableBlockRatio: Perbill = Perbill::one();
	}
	impl frame_system::Trait for Test {
		type Origin = Origin;
		type Index = u64;
		type BlockNumber = u64;
		type Call = ();
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = ();
		type BlockHashCount = BlockHashCount;
		type MaximumBlockWeight = MaximumBlockWeight;
		type AvailableBlockRatio = AvailableBlockRatio;
		type MaximumBlockLength = MaximumBlockLength;
		type Version = ();
		type ModuleToIndex = ();
		type AccountData = ();
		type MigrateAccount = (); type OnNewAccount = ();
		type OnKilledAccount = ();
	}
	parameter_types! {
		pub const MinimumPeriod: u64 = 5;
	}
	impl Trait for Test {
		type Moment = u64;
		type OnTimestampSet = ();
		type MinimumPeriod = MinimumPeriod;
	}
	type Timestamp = Module<Test>;

	#[test]
	fn timestamp_works() {
		let t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
		TestExternalities::new(t).execute_with(|| {
			Timestamp::set_timestamp(42);
			assert_ok!(Timestamp::dispatch(Call::set(69), Origin::NONE));
			assert_eq!(Timestamp::now(), 69);
		});
	}

	#[test]
	#[should_panic(expected = "Timestamp must be updated only once in the block")]
	fn double_timestamp_should_fail() {
		let t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
		TestExternalities::new(t).execute_with(|| {
			Timestamp::set_timestamp(42);
			assert_ok!(Timestamp::dispatch(Call::set(69), Origin::NONE));
			let _ = Timestamp::dispatch(Call::set(70), Origin::NONE);
		});
	}

	#[test]
	#[should_panic(expected = "Timestamp must increment by at least <MinimumPeriod> between sequential blocks")]
	fn block_period_minimum_enforced() {
		let t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
		TestExternalities::new(t).execute_with(|| {
			Timestamp::set_timestamp(42);
			let _ = Timestamp::dispatch(Call::set(46), Origin::NONE);
		});
	}
}
