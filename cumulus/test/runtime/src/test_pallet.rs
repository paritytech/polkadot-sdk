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

/// A special pallet that exposes dispatchables that are only useful for testing.
pub use pallet::*;

/// Some key that we set in genesis and only read in
/// [`SingleBlockMigrations`](crate::SingleBlockMigrations) to ensure that
/// [`OnRuntimeUpgrade`](frame_support::traits::OnRuntimeUpgrade) works as expected.
pub const TEST_RUNTIME_UPGRADE_KEY: &[u8] = b"+test_runtime_upgrade_key+";

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use crate::test_pallet::TEST_RUNTIME_UPGRADE_KEY;
	use alloc::{vec, vec::Vec};
	use cumulus_primitives_core::CumulusDigestItem;
	use cumulus_primitives_storage_weight_reclaim::get_proof_size;
	use frame_support::{
		dispatch::DispatchInfo,
		inherent::{InherentData, InherentIdentifier, ProvideInherent},
		pallet_prelude::*,
		traits::IsSubType,
		weights::constants::WEIGHT_REF_TIME_PER_SECOND,
	};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{Dispatchable, Implication, TransactionExtension};

	/// The inherent identifier for weight consumption.
	pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"consume0";

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + cumulus_pallet_parachain_system::Config {}

	/// A simple storage map for testing purposes.
	#[pallet::storage]
	pub type TestMap<T: Config> = StorageMap<_, Twox64Concat, u32, (), ValueQuery>;

	/// Flag to indicate if a 1s weight should be registered in the next `on_initialize`.
	#[pallet::storage]
	pub type ScheduleWeightRegistration<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// Weight to be consumed by the inherent call.
	#[pallet::storage]
	pub type InherentWeightConsume<T: Config> = StorageValue<_, Weight, OptionQuery>;

	/// A map that contains on single big value at the current block.
	///
	/// In every block we are moving the big value from the previous block to current block. This is
	/// done to test that the storage proof size between multiple blocks in the same bundle is
	/// shared.
	#[pallet::storage]
	pub type BigValueMove<T: Config> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, Vec<u8>, OptionQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			if ScheduleWeightRegistration::<T>::get() {
				let weight_to_register = Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND, 0);

				let left_weight = frame_system::Pallet::<T>::remaining_block_weight();

				if left_weight.can_consume(weight_to_register) {
					tracing::info!("Consuming 1s of weight :)");
					// We have enough capacity, consume the flag and register the weight
					ScheduleWeightRegistration::<T>::kill();
					return weight_to_register
				}
			}

			if let Some(mut value) = BigValueMove::<T>::take(n - 1u32.into()) {
				// Modify the value a little bit.
				let parent_hash = frame_system::Pallet::<T>::parent_hash();
				value[..parent_hash.as_ref().len()].copy_from_slice(parent_hash.as_ref());

				BigValueMove::<T>::insert(n, value);

				// Depositing the event is important, because then we write the actual proof size
				// into the state. If some node returns a different proof size on import of this
				// block, we will detect it this way as the storage root will be different.
				Self::deposit_event(Event::MovedBigValue {
					proof_size: get_proof_size().unwrap_or_default(),
				})
			}

			Weight::zero()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A test dispatchable for setting a custom head data in `validate_block`.
		#[pallet::weight(0)]
		pub fn set_custom_validation_head_data(
			_: OriginFor<T>,
			custom_header: alloc::vec::Vec<u8>,
		) -> DispatchResult {
			cumulus_pallet_parachain_system::Pallet::<T>::set_custom_validation_head_data(
				custom_header,
			);
			Ok(())
		}

		/// A dispatchable that first reads two values from two different child tries, asserts they
		/// are the expected values (if the values exist in the state) and then writes two different
		/// values to these child tries.
		#[pallet::weight(0)]
		pub fn read_and_write_child_tries(_: OriginFor<T>) -> DispatchResult {
			let key = &b"hello"[..];
			let first_trie = &b"first"[..];
			let second_trie = &b"second"[..];
			let first_value = "world1".encode();
			let second_value = "world2".encode();

			if let Some(res) = sp_io::default_child_storage::get(first_trie, key) {
				assert_eq!(first_value, res);
			}
			if let Some(res) = sp_io::default_child_storage::get(second_trie, key) {
				assert_eq!(second_value, res);
			}

			sp_io::default_child_storage::set(first_trie, key, &first_value);
			sp_io::default_child_storage::set(second_trie, key, &second_value);

			Ok(())
		}

		/// Reads a key and writes a big value under this key.
		///
		/// At genesis this `key` is empty and thus, will only be set in consequent blocks.
		pub fn read_and_write_big_value(_: OriginFor<T>) -> DispatchResult {
			let key = &b"really_huge_value"[..];
			sp_io::storage::get(key);
			sp_io::storage::set(key, &vec![0u8; 1024 * 1024 * 5]);

			Ok(())
		}

		/// Stores `()` in `TestMap` for keys from 0 up to `max_key`.
		#[pallet::weight(0)]
		pub fn store_values_in_map(_: OriginFor<T>, max_key: u32) -> DispatchResult {
			for i in 0..=max_key {
				TestMap::<T>::insert(i, ());
			}
			Ok(())
		}

		/// Removes the value associated with `key` from `TestMap`.
		#[pallet::weight(0)]
		pub fn remove_value_from_map(_: OriginFor<T>, key: u32) -> DispatchResult {
			TestMap::<T>::remove(key);
			Ok(())
		}

		/// Schedule a 1 second weight registration in the next `on_initialize`.
		#[pallet::weight(0)]
		pub fn schedule_weight_registration(_: OriginFor<T>) -> DispatchResult {
			ScheduleWeightRegistration::<T>::set(true);
			Ok(())
		}

		/// Set the weight to be consumed by the next inherent call.
		#[pallet::weight(0)]
		pub fn set_inherent_weight_consume(_: OriginFor<T>, weight: Weight) -> DispatchResult {
			InherentWeightConsume::<T>::put(weight);
			Ok(())
		}

		/// Consume weight via inherent call (clears the storage after consuming).
		#[pallet::weight((
			InherentWeightConsume::<T>::get().unwrap_or_default(),
			DispatchClass::Mandatory
		))]
		pub fn consume_weight_inherent(origin: OriginFor<T>) -> DispatchResult {
			ensure_none(origin)?;

			// Clear the storage item to ensure this can only be called once per inherent
			InherentWeightConsume::<T>::kill();

			Ok(())
		}

		/// This function registers a high weight usage manually, while it actually only announces
		/// to use a weight of `0` :)
		///
		/// Uses the [`TestTransactionExtension`] logic to ensure the transaction is only accepted
		/// when we can fit the `1s` weight into the block.
		#[pallet::weight(0)]
		pub fn use_more_weight_than_announced(
			_: OriginFor<T>,
			_must_be_first_block_in_core: bool,
		) -> DispatchResult {
			// Register weight manually.
			frame_system::Pallet::<T>::register_extra_weight_unchecked(
				Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND, 0),
				DispatchClass::Normal,
			);

			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = sp_inherents::MakeFatalError<()>;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(_data: &InherentData) -> Option<Self::Call> {
			// Check if there's weight to consume from storage
			let weight_to_consume = InherentWeightConsume::<T>::get()?;

			// Check if the weight fits in the remaining block capacity
			let remaining_weight = frame_system::Pallet::<T>::remaining_block_weight();

			if remaining_weight.can_consume(weight_to_consume) {
				Some(Call::consume_weight_inherent {})
			} else {
				// Weight doesn't fit, don't create the inherent
				None
			}
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::consume_weight_inherent {})
		}
	}

	#[derive(frame_support::DefaultNoBound)]
	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		#[serde(skip)]
		pub _config: core::marker::PhantomData<T>,
		/// Controls if the `BigValueMove` logic is enabled.
		pub enable_big_value_move: bool,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			sp_io::storage::set(TEST_RUNTIME_UPGRADE_KEY, &[1, 2, 3, 4]);

			if self.enable_big_value_move {
				BigValueMove::<T>::insert(BlockNumberFor::<T>::from(0u32), vec![0u8; 4 * 1024]);
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		MovedBigValue { proof_size: u64 },
	}

	#[derive(
		Encode,
		Decode,
		CloneNoBound,
		EqNoBound,
		PartialEqNoBound,
		TypeInfo,
		RuntimeDebugNoBound,
		DecodeWithMemTracking,
	)]
	#[scale_info(skip_type_params(T))]
	pub struct TestTransactionExtension<T>(core::marker::PhantomData<T>);

	impl<T> Default for TestTransactionExtension<T> {
		fn default() -> Self {
			Self(core::marker::PhantomData)
		}
	}

	impl<T: Config> TransactionExtension<T::RuntimeCall> for TestTransactionExtension<T>
	where
		T: Config + Send + Sync,
		T::RuntimeCall: IsSubType<Call<T>> + Dispatchable<Info = DispatchInfo>,
	{
		const IDENTIFIER: &'static str = "TestTransactionExtension";
		type Implicit = ();
		type Val = ();
		type Pre = ();

		fn validate(
			&self,
			origin: T::RuntimeOrigin,
			call: &T::RuntimeCall,
			_info: &DispatchInfo,
			_len: usize,
			_self_implicit: Self::Implicit,
			_inherited_implication: &impl Implication,
			_: TransactionSource,
		) -> ValidateResult<Self::Val, T::RuntimeCall> {
			if let Some(call) = call.is_sub_type() {
				match call {
					Call::use_more_weight_than_announced { must_be_first_block_in_core } =>
						if {
							let digest = frame_system::Pallet::<T>::digest();

							CumulusDigestItem::find_bundle_info(&digest)
								// Default being `true` to support `validate_transaction`
								.map_or(true, |bi| {
									// Either we want that the transaction goes into the first block
									// of a core
									bi.index == 0 && *must_be_first_block_in_core ||
										// Or it goes to any block that isn't the first block
										bi.index > 0 && !*must_be_first_block_in_core
								})
						} {
							Ok((
								ValidTransaction {
									provides: vec![vec![1, 2, 3, 4, 5]],
									..Default::default()
								},
								(),
								origin,
							))
						} else {
							Err(TransactionValidityError::Invalid(
								InvalidTransaction::ExhaustsResources,
							))
						},
					_ => Ok((Default::default(), (), origin)),
				}
			} else {
				Ok((Default::default(), (), origin))
			}
		}

		fn prepare(
			self,
			val: Self::Val,
			_origin: &T::RuntimeOrigin,
			_call: &T::RuntimeCall,
			_info: &DispatchInfo,
			_len: usize,
		) -> Result<Self::Pre, TransactionValidityError> {
			Ok(val)
		}

		fn weight(&self, _: &T::RuntimeCall) -> Weight {
			Weight::zero()
		}
	}
}
