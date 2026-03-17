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

	#[pallet::config]
	pub trait Config: frame_system::Config {
		#[pallet::constant]
		type Epsilon: Get<FixedU128>;

		#[pallet::constant]
		type MinNudges: Get<u32>;

		/// Number of slots a nudge remains valid: [slot, slot + NudgeValidity).
		#[pallet::constant]
		type NudgeValidity: Get<u64>;

		type AuthorityProvider: AuthorityProvider;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type CurrentPrice<T: Config> = StorageValue<_, FixedU128, ValueQuery>;

	#[pallet::storage]
	pub(crate) type DidUpdate<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			T::DbWeight::get().reads(1)
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			if T::MinNudges::get() > 0 {
				assert!(DidUpdate::<T>::take(), "Price oracle inherent must be included in block");
			} else {
				DidUpdate::<T>::kill();
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
				!DidUpdate::<T>::exists(),
				"Price oracle inherent must be submitted only once per block"
			);

			let authorities = T::AuthorityProvider::authorities();
			let current_slot = T::AuthorityProvider::current_slot();
			let validity = T::NudgeValidity::get();
			let epsilon = T::Epsilon::get();

			let mut ups: u32 = 0;
			let mut downs: u32 = 0;

			for nudge in &nudges {
				let nudge_slot_val: u64 = (*nudge.slot).into();
				let current_slot_val: u64 = (*current_slot).into();
				if current_slot_val.saturating_sub(nudge_slot_val) >= validity {
					log::warn!(
						target: LOG_TARGET,
						"Stale nudge from slot {:?}, current slot {:?}, validity {}",
						nudge.slot, current_slot, validity,
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

			let current_price = CurrentPrice::<T>::get();
			if ups > 0 || downs > 0 {
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
			}

			DidUpdate::<T>::put(true);
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
				let nudge_slot_val: u64 = (*nudge.slot).into();
				let current_slot_val: u64 = (*current_slot).into();
				if current_slot_val.saturating_sub(nudge_slot_val) >= validity {
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
