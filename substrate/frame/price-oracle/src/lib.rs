#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use frame_support::{pallet_prelude::*, traits::Time, traits::EnsureOrigin};
use frame_system::pallet_prelude::*;
use sp_consensus_babe::AuthorityId;
use sp_consensus_slots::Slot;
use sp_price_oracle::{
	InherentError, Nudge, PriceOracleInherentData, SignedNudge, INHERENT_IDENTIFIER,
};
use sp_runtime::{traits::Saturating, FixedPointNumber, FixedU128};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod tests;

pub use pallet::*;

const LOG_TARGET: &str = "runtime::price-oracle";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use sp_inherents::{InherentData, InherentIdentifier};

	pub trait AuthorityProvider {
		fn authorities() -> Vec<AuthorityId>;
		fn current_slot() -> Slot;
	}

	/// Called whenever the on-chain price is updated.
	/// Can be used to propagate the price via XCM to other chains (e.g. Asset Hub).
	#[impl_trait_for_tuples::impl_for_tuples(8)]
	pub trait OnPriceUpdate<BlockNumber> {
		fn on_price_update(new_price: FixedU128, block_number: BlockNumber, timestamp: u64);
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Absolute price change per net nudge (e.g. 0.001 means each net Up adds $0.001).
		#[pallet::constant]
		type Epsilon: Get<FixedU128>;

		/// Minimum valid nudges required per block. Block panics in `on_finalize` if not met.
		/// Set to 0 to make oracle inherents optional.
		#[pallet::constant]
		type MinNudges: Get<u32>;

		/// Number of slots a nudge remains valid: [slot, slot + NudgeValidity).
		#[pallet::constant]
		type NudgeValidity: Get<u64>;

		type AuthorityProvider: AuthorityProvider;

		type TimeProvider: Time;

		/// Hook called when the price is updated. Set to `()` if unused.
		type OnPriceUpdate: OnPriceUpdate<BlockNumberFor<Self>>;

		/// Origin allowed to toggle the panic switch.
		type PriceOracleOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type CurrentPrice<T: Config> = StorageValue<_, FixedU128, ValueQuery>;

	/// Number of valid nudges accepted in the current block's inherent.
	#[pallet::storage]
	pub(crate) type NudgeCount<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// When enabled, `on_finalize` panics if no inherent was included in the block.
	#[pallet::storage]
	pub type PanicSwitch<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// Too few nudges were provided (below the runtime minimum).
		TooFewNudges,
		/// A nudge in the inherent is too old (slot is beyond the validity window).
		StaleNudge,
		/// A nudge in the inherent is a duplicate.
		DuplicateNudge,
		/// A nudge in the inherent has an invalid authority.
		InvalidAuthority,
		/// A nudge in the inherent has an invalid signature.
		InvalidSignature,
		/// Duplicate inherent in the same block.
		DuplicateInherent,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			T::DbWeight::get().reads(1)
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			let inherent_included = NudgeCount::<T>::exists();
			let count = NudgeCount::<T>::take();

			if PanicSwitch::<T>::get() {
				assert!(
					inherent_included,
					"Price oracle: panic switch is on but no inherent was included in this block",
				);
			}

			let min = T::MinNudges::get();
			if min > 0 {
				assert!(
					count >= min,
					"Price oracle: got {} valid nudges, need at least {}",
					count,
					min,
				);
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a set of signed nudges as an inherent.
		///
		/// Validation is strict: any invalid nudge (stale, duplicate authority, bad signature,
		/// unknown authority) causes this inherent to fail and the block to be rejected.
		///
		/// `check_inherent` (run by importers) only enforces `MinNudges` as a reasonableness
		/// check. All per-nudge validation happens here.
		#[pallet::call_index(0)]
		#[pallet::weight((
			Weight::from_parts(10_000, 0).saturating_mul(nudges.len() as u64),
			DispatchClass::Mandatory
		))]
		pub fn submit_nudges(origin: OriginFor<T>, nudges: Vec<SignedNudge>) -> DispatchResult {
			ensure_none(origin)?;
			ensure!(!NudgeCount::<T>::exists(), Error::<T>::DuplicateInherent);
			ensure!(nudges.len() >= T::MinNudges::get() as usize, Error::<T>::TooFewNudges);

			let authorities = T::AuthorityProvider::authorities();
			let current_slot = T::AuthorityProvider::current_slot();
			let validity = T::NudgeValidity::get();
			let epsilon = T::Epsilon::get();

			let mut ups: u32 = 0;
			let mut downs: u32 = 0;
			let mut seen_authorities = alloc::collections::BTreeSet::<u32>::new();

			for nudge in &nudges {
				ensure!(*nudge.slot + validity > *current_slot, Error::<T>::StaleNudge);

				ensure!(seen_authorities.insert(nudge.authority_index), Error::<T>::DuplicateNudge);

				let authority = authorities
					.get(nudge.authority_index as usize)
					.ok_or(Error::<T>::InvalidAuthority)?;

				ensure!(nudge.verify(authority), Error::<T>::InvalidSignature);

				match nudge.nudge {
					Nudge::Up => ups += 1,
					Nudge::Down => downs += 1,
				}
			}

			let total_valid = ups + downs;
			let current_price = CurrentPrice::<T>::get();
			if total_valid > 0 {
				let net = ups.abs_diff(downs);
				let delta = epsilon.saturating_mul(FixedU128::saturating_from_integer(net));

				let new_price = if ups >= downs {
					current_price.saturating_add(delta)
				} else {
					current_price.saturating_sub(delta)
				};

				log::info!(
					target: LOG_TARGET,
					"Price oracle: {} ups, {} downs, price {} -> {}",
					ups, downs, current_price, new_price,
				);

				CurrentPrice::<T>::put(new_price);
				let block_number = frame_system::Pallet::<T>::block_number();
				let timestamp: u64 = T::TimeProvider::now().try_into().unwrap_or(0u64);
				T::OnPriceUpdate::on_price_update(new_price, block_number, timestamp);
			}

			NudgeCount::<T>::put(total_valid);
			Ok(())
		}

		/// Enable or disable the panic switch.
		///
		/// When enabled, `on_finalize` will panic if no inherent was included in the block.
		#[pallet::call_index(1)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn set_panic_switch(origin: OriginFor<T>, enabled: bool) -> DispatchResult {
			T::PriceOracleOrigin::ensure_origin(origin)?;
			PanicSwitch::<T>::put(enabled);
			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = InherentError;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let nudges = data
				.get_data::<PriceOracleInherentData>(&INHERENT_IDENTIFIER)
				.expect("Price oracle inherent data encoded correctly")?;

			Some(Call::submit_nudges { nudges })
		}

		fn check_inherent(call: &Self::Call, _data: &InherentData) -> Result<(), Self::Error> {
			let nudges = match call {
				Call::submit_nudges { ref nudges } => nudges,
				_ => return Ok(()),
			};

			let min = T::MinNudges::get();
			if (nudges.len() as u32) < min {
				return Err(InherentError::TooFewNudges(nudges.len() as u32, min));
			}

			Ok(())
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::submit_nudges { .. })
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn current_price() -> FixedU128 {
			CurrentPrice::<T>::get()
		}

		pub fn nudge_validity() -> u64 {
			T::NudgeValidity::get()
		}

		pub fn minimum_nudges_required() -> u32 {
			T::MinNudges::get()
		}
	}
}
