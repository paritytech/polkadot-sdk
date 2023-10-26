//! # Parameters
//! Pallet to store and configure parameters.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

use frame_support::traits::EnsureOriginWithArg;

pub mod traits;
use traits::AggregratedKeyValue;

mod mock;
mod tests;
mod weights;

pub use module::*;
pub use weights::WeightInfo;

/// The key type of a parameter.
type KeyOf<T> = <<T as Config>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey;

/// The value type of a parameter.
type ValueOf<T> = <<T as Config>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue;

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The key value type for parameters. Usually created by
		/// [`crate::parameters::define_aggregrated_parameters`].
		type AggregratedKeyValue: AggregratedKeyValue;

		/// The origin which may update the parameter.
		type AdminOrigin: EnsureOriginWithArg<Self::RuntimeOrigin, KeyOf<Self>>;

		/// Weight information for extrinsics in this module.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Parameter is updated
		Updated { key_value: T::AggregratedKeyValue },
	}

	/// Stored parameters.
	///
	/// map KeyOf<T> => Option<ValueOf<T>>
	#[pallet::storage]
	pub type Parameters<T: Config> = StorageMap<_, Blake2_128Concat, KeyOf<T>, ValueOf<T>, OptionQuery>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the value of a parameter.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_parameter())]
		pub fn set_parameter(origin: OriginFor<T>, key_value: T::AggregratedKeyValue) -> DispatchResult {
			let (key, value) = key_value.clone().into_parts();

			T::AdminOrigin::ensure_origin(origin, &key)?;

			Parameters::<T>::mutate(key, |v| *v = value);

			Self::deposit_event(Event::Updated { key_value });

			Ok(())
		}
	}
}
