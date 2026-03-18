#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use frame_support::pallet_prelude::*;
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
	pub trait Config: frame_system::Config + pallet_timestamp::Config {
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

		/// Hook called when the price is updated. Set to `()` if unused.
		type OnPriceUpdate: OnPriceUpdate<BlockNumberFor<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type CurrentPrice<T: Config> = StorageValue<_, FixedU128, ValueQuery>;

	/// Number of valid nudges accepted in the current block's inherent.
	#[pallet::storage]
	pub(crate) type NudgeCount<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			T::DbWeight::get().reads(1)
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			let count = NudgeCount::<T>::take();
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
		#[pallet::call_index(0)]
		#[pallet::weight((
			Weight::from_parts(10_000, 0).saturating_mul(nudges.len() as u64),
			DispatchClass::Mandatory
		))]
		pub fn submit_nudges(origin: OriginFor<T>, nudges: Vec<SignedNudge>) -> DispatchResult {
			ensure_none(origin)?;
			assert!(
				!NudgeCount::<T>::exists(),
				"Price oracle inherent must be submitted only once per block"
			);

			let authorities = T::AuthorityProvider::authorities();
			let current_slot = T::AuthorityProvider::current_slot();
			let validity = T::NudgeValidity::get();
			let epsilon = T::Epsilon::get();

			let mut ups: u32 = 0;
			let mut downs: u32 = 0;
			let mut seen_authorities = alloc::collections::BTreeSet::<u32>::new();

			for nudge in &nudges {
				if *nudge.slot + validity <= *current_slot {
					log::warn!(
						target: LOG_TARGET,
						"Stale nudge from slot {:?}, current slot {:?}, validity {}",
						nudge.slot, current_slot, validity,
					);
					continue;
				}

				if !seen_authorities.insert(nudge.authority_index) {
					log::warn!(
						target: LOG_TARGET,
						"Duplicate nudge from authority index {}, skipping",
						nudge.authority_index,
					);
					continue;
				}

				let authority = match authorities.get(nudge.authority_index as usize) {
					Some(a) => a,
					None => {
						log::warn!(
							target: LOG_TARGET,
							"Invalid authority index {}",
							nudge.authority_index,
						);
						continue;
					},
				};

				if !nudge.verify(authority) {
					log::warn!(
						target: LOG_TARGET,
						"Invalid signature for authority index {}",
						nudge.authority_index,
					);
					continue;
				}

				match nudge.nudge {
					Nudge::Up => ups += 1,
					Nudge::Down => downs += 1,
				}
			}

			let total_valid = ups + downs;
			let current_price = CurrentPrice::<T>::get();
			if total_valid > 0 {
				let net = if ups >= downs { ups - downs } else { downs - ups };
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
				let timestamp: u64 =
					pallet_timestamp::Pallet::<T>::get().try_into().unwrap_or(0u64);
				T::OnPriceUpdate::on_price_update(new_price, block_number, timestamp);
			}

			NudgeCount::<T>::put(total_valid);
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
				.expect("Price oracle inherent data encoded correctly")
				.unwrap_or_default();

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

			let authorities = T::AuthorityProvider::authorities();
			let current_slot = T::AuthorityProvider::current_slot();
			let validity = T::NudgeValidity::get();

			for nudge in nudges {
				if *nudge.slot + validity <= *current_slot {
					return Err(InherentError::StaleNudge(nudge.slot));
				}

				let authority = authorities
					.get(nudge.authority_index as usize)
					.ok_or(InherentError::InvalidSignature(nudge.authority_index))?;

				if !nudge.verify(authority) {
					return Err(InherentError::InvalidSignature(nudge.authority_index));
				}
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
	}
}
