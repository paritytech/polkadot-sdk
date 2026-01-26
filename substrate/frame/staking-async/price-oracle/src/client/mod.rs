pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
#[cfg(test)]
pub mod mock;
#[cfg(test)]
pub mod test;

pub use pallet::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::{BlockNumberFor, *};
	extern crate alloc;
	use alloc::vec::Vec;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The origin representing the relay chain.
		type RelayChainOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[derive(
		Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, Default,
	)]
	pub enum ValidatorSet<AccountId> {
		/// We don't have a validator set yet.
		#[default]
		None,
		/// We have a validator set, but we have not given it to the session pallet to be
		/// planned yet.
		ToPlan(Vec<AccountId>),
		/// A validator set was just given to the session pallet to be planned.
		///
		/// We should immediately signal the session pallet to trigger a new session, and
		/// activate it.
		Planned,
	}

	impl<AccountId> ValidatorSet<AccountId> {
		fn should_end_session(&self) -> bool {
			matches!(self, ValidatorSet::ToPlan(_) | ValidatorSet::Planned)
		}

		fn new_session(self) -> (Self, Option<Vec<AccountId>>) {
			match self {
				Self::None => {
					debug_assert!(false, "we should never instruct session to trigger a new session if we have no validator set to plan");
					(Self::None, None)
				},
				// We have something to be planned, return it, and set our next stage to
				// `planned`.
				Self::ToPlan(to_plan) => (Self::Planned, Some(to_plan)),
				// We just planned something, don't plan return anything new to be planned,
				// just let session enact what was previously planned. Set our next stage to
				// `None`.
				Self::Planned => (Self::None, None),
			}
		}
	}

	#[pallet::storage]
	#[pallet::unbounded]
	pub type ValidatorSetStorage<T: Config> =
		StorageValue<_, ValidatorSet<T::AccountId>, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::relay_new_validator_set())]
		pub fn relay_new_validator_set(
			origin: OriginFor<T>,
			validators: Vec<T::AccountId>,
		) -> DispatchResult {
			log::info!(target: "runtime::price-oracle", "relay_new_validator_set: validators: {:?}", validators);
			T::RelayChainOrigin::ensure_origin_or_root(origin)?;
			ValidatorSetStorage::<T>::put(ValidatorSet::ToPlan(validators));
			Ok(())
		}
	}

	impl<T: Config> pallet_session::ShouldEndSession<BlockNumberFor<T>> for Pallet<T> {
		fn should_end_session(_now: BlockNumberFor<T>) -> bool {
			log::info!(target: "runtime::price-oracle", "should_end_session: {:?}", ValidatorSetStorage::<T>::get().should_end_session());
			ValidatorSetStorage::<T>::get().should_end_session()
		}
	}

	impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(new_index: u32) -> Option<Vec<T::AccountId>> {
			log::info!(target: "runtime::price-oracle", "new_session: {:?}", new_index);
			let (next, ret) = ValidatorSetStorage::<T>::get().new_session();
			ValidatorSetStorage::<T>::put(next);
			ret
		}
		fn end_session(_end_index: u32) {
			// nada
		}
		fn start_session(_start_index: u32) {
			// nada
		}
	}
}
