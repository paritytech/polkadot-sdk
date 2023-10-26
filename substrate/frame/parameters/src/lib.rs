//! # Parameters
//! Offer a central place to store and configure parameters.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

use frame_support::traits::EnsureOriginWithArg;
use orml_traits::parameters::{AggregratedKeyValue, Into2, Key, RuntimeParameterStore, TryInto2};

mod mock;
mod tests;
mod weights;

pub use module::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The key value type for parameters. Usually created by
		/// orml_traits::parameters::define_aggregrated_parameters
		type AggregratedKeyValue: AggregratedKeyValue;

		/// The origin which may update the parameter.
		type AdminOrigin: EnsureOriginWithArg<Self::RuntimeOrigin, KeyOf<Self>>;

		/// Weight information for extrinsics in this module.
		type WeightInfo: WeightInfo;
	}

	type KeyOf<T> = <<T as Config>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey;
	type ValueOf<T> = <<T as Config>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue;

	#[pallet::error]
	pub enum Error<T> {}

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
		/// Set parameter
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

impl<T: Config> RuntimeParameterStore for Pallet<T> {
	type AggregratedKeyValue = T::AggregratedKeyValue;

	fn get<KV, K>(key: K) -> Option<K::Value>
	where
		KV: AggregratedKeyValue,
		K: Key + Into<<KV as AggregratedKeyValue>::AggregratedKey>,
		<KV as AggregratedKeyValue>::AggregratedKey:
			Into2<<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey>,
		<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue:
			TryInto2<<KV as AggregratedKeyValue>::AggregratedValue>,
		<KV as AggregratedKeyValue>::AggregratedValue: TryInto<K::WrappedValue>,
	{
		let key: <KV as AggregratedKeyValue>::AggregratedKey = key.into();
		let val = Parameters::<T>::get(key.into2());
		val.and_then(|v| {
			let val: <KV as AggregratedKeyValue>::AggregratedValue = v.try_into2().ok()?;
			let val: K::WrappedValue = val.try_into().ok()?;
			let val = val.into();
			Some(val)
		})
	}
}
