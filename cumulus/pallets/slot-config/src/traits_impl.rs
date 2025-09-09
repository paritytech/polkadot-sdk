// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Trait implementations for integration with pallet-aura and pallet-timestamp

use crate::{AuraInterface, TimestampProvider};
use sp_consensus_aura::Slot;
// Trait implementations for pallet integration

/// Implementation of AuraInterface for pallet-aura
pub struct AuraPalletWrapper<T>(core::marker::PhantomData<T>);

impl<T> AuraInterface for AuraPalletWrapper<T>
where
	T: pallet_aura::Config,
{
	fn current_slot() -> Slot {
		pallet_aura::CurrentSlot::<T>::get()
	}

	fn set_current_slot(slot: Slot) {
		pallet_aura::CurrentSlot::<T>::put(slot);
	}
}

/// Implementation of TimestampProvider for pallet-timestamp
pub struct TimestampPalletWrapper<T>(core::marker::PhantomData<T>);

impl<T> TimestampProvider<T::Moment> for TimestampPalletWrapper<T>
where
	T: pallet_timestamp::Config,
	T::Moment: Copy + Default,
{
	fn now() -> T::Moment {
		pallet_timestamp::Now::<T>::get()
	}
}

/// Helper type aliases for runtime configuration
/// 
/// # Usage in runtime:
/// 
/// ```rust,ignore
/// impl cumulus_pallet_slot_config::Config for Runtime {
///     type AuraPallet = cumulus_pallet_slot_config::traits_impl::AuraPalletWrapper<Runtime>;
///     type TimestampProvider = cumulus_pallet_slot_config::traits_impl::TimestampPalletWrapper<Runtime>;
///     type Moment = <Runtime as pallet_timestamp::Config>::Moment;
///     // ... other config
/// }
/// ```

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{
		derive_impl, parameter_types,
		traits::{ConstU32, ConstU64},
	};
	use sp_runtime::{
		traits::{BlakeTwo256, IdentityLookup},
		BuildStorage,
	};

	type Block = frame_system::mocking::MockBlock<Test>;

	frame_support::construct_runtime!(
		pub enum Test {
			System: frame_system,
			Timestamp: pallet_timestamp,
			Aura: pallet_aura,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = Block;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type RuntimeEvent = RuntimeEvent;
	}

	impl pallet_timestamp::Config for Test {
		type Moment = u64;
		type OnTimestampSet = Aura;
		type MinimumPeriod = ConstU64<1>;
		type WeightInfo = ();
	}

	impl pallet_aura::Config for Test {
		type AuthorityId = sp_consensus_aura::sr25519::AuthorityId;
		type DisabledValidators = ();
		type MaxAuthorities = ConstU32<32>;
		type AllowMultipleBlocksPerSlot = ConstU64<1>;
		type SlotDuration = ConstU64<6000>;
	}

	#[test]
	fn aura_wrapper_works() {
		let mut ext = sp_io::TestExternalities::new(Default::default());
		ext.execute_with(|| {
			// Test getting current slot
			let slot = AuraPalletWrapper::<Test>::current_slot();
			assert_eq!(*slot, 0u64);

			// Test setting current slot
			AuraPalletWrapper::<Test>::set_current_slot(Slot::from(42u64));
			let new_slot = AuraPalletWrapper::<Test>::current_slot();
			assert_eq!(*new_slot, 42u64);
		});
	}

	#[test]
	fn timestamp_wrapper_works() {
		let mut ext = sp_io::TestExternalities::new(Default::default());
		ext.execute_with(|| {
			// Set timestamp
			pallet_timestamp::Now::<Test>::put(12345);
			
			// Test getting timestamp
			let timestamp = TimestampPalletWrapper::<Test>::now();
			assert_eq!(timestamp, 12345);
		});
	}
}
