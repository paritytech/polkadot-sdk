// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[doc(hidden)]
pub use codec;
pub use codec as parity_scale_codec;
#[doc(hidden)]
use frame_support::Parameter;
#[doc(hidden)]
pub use paste;
#[doc(hidden)]
pub use scale_info;
pub use sp_runtime::{self, RuntimeDebug};

pub trait RuntimeParameterStore {
	type AggregratedKeyValue: AggregratedKeyValue;

	fn get<KV, K>(key: K) -> Option<K::Value>
	where
		KV: AggregratedKeyValue,
		K: Key + Into<<KV as AggregratedKeyValue>::AggregratedKey>,
		<KV as AggregratedKeyValue>::AggregratedKey:
			IntoKey<<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey>,
		<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue:
			TryIntoKey<<KV as AggregratedKeyValue>::AggregratedValue>,
		<KV as AggregratedKeyValue>::AggregratedValue: TryInto<K::WrappedValue>;
}

pub trait Key {
	type Value;
	type WrappedValue: Into<Self::Value>;
}

pub trait AggregratedKeyValue: Parameter {
	type AggregratedKey: Parameter + codec::MaxEncodedLen;
	type AggregratedValue: Parameter + codec::MaxEncodedLen;

	fn into_parts(self) -> (Self::AggregratedKey, Option<Self::AggregratedValue>);
}

pub trait ParameterStore<KV: AggregratedKeyValue> {
	fn get<K>(key: K) -> Option<K::Value>
	where
		K: Key + Into<<KV as AggregratedKeyValue>::AggregratedKey>,
		<KV as AggregratedKeyValue>::AggregratedValue: TryInto<K::WrappedValue>;
}

pub struct ParameterStoreAdapter<PS, KV>(sp_std::marker::PhantomData<(PS, KV)>);

impl<PS, KV> ParameterStore<KV> for ParameterStoreAdapter<PS, KV>
where
	PS: RuntimeParameterStore,
	KV: AggregratedKeyValue,
	<KV as AggregratedKeyValue>::AggregratedKey:
		IntoKey<<<PS as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey>,
	<KV as AggregratedKeyValue>::AggregratedValue:
		TryFromKey<<<PS as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue>,
{
	fn get<K>(key: K) -> Option<K::Value>
	where
		K: Key + Into<<KV as AggregratedKeyValue>::AggregratedKey>,
		<KV as AggregratedKeyValue>::AggregratedValue: TryInto<K::WrappedValue>,
	{
		PS::get::<KV, K>(key)
	}
}

// workaround for rust bug https://github.com/rust-lang/rust/issues/51445
mod workaround {
	pub trait FromKey<T>: Sized {
		#[must_use]
		fn from_key(value: T) -> Self;
	}

	pub trait IntoKey<T>: Sized {
		#[must_use]
		fn into_key(self) -> T;
	}

	impl<T, U> IntoKey<U> for T
	where
		U: FromKey<T>,
	{
		#[inline]
		fn into_key(self) -> U {
			U::from_key(self)
		}
	}

	pub trait TryIntoKey<T>: Sized {
		type Error;

		fn try_into_key(self) -> Result<T, Self::Error>;
	}

	pub trait TryFromKey<T>: Sized {
		type Error;

		fn try_from_key(value: T) -> Result<Self, Self::Error>;
	}

	impl<T, U> TryIntoKey<U> for T
	where
		U: TryFromKey<T>,
	{
		type Error = U::Error;

		#[inline]
		fn try_into_key(self) -> Result<U, U::Error> {
			U::try_from_key(self)
		}
	}
}
pub use workaround::*;

/// Define parameters key value types.
/// Example:
///
/// ```
/// # #[macro_use]
/// # extern crate orml_traits;
/// # fn main() {}
/// define_parameters! {
///     pub Pallet = {
///         Key1: u64 = 0,
///         Key2(u32): u32 = 1,
///         Key3((u8, u8)): u128 = 2,
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_parameters {
	(
		$vis:vis $name:ident = {
			$(
				$key_name:ident $( ($key_para: ty) )? : $value_type:ty = $index:expr
			),+ $(,)?
		}
	) => {
		$crate::traits::dynamic_params::paste::item! {
			#[derive(
				Clone,
				PartialEq,
				Eq,
				$crate::traits::dynamic_params::codec::Encode,
				$crate::traits::dynamic_params::codec::Decode,
				$crate::traits::dynamic_params::codec::MaxEncodedLen,
				$crate::traits::dynamic_params::RuntimeDebug,
				$crate::traits::dynamic_params::scale_info::TypeInfo
			)]
			$vis enum $name {
				$(
					#[codec(index = $index)]
					$key_name($key_name, Option<$value_type>),
				)*
			}

			#[derive(
				Clone,
				PartialEq,
				Eq,
				$crate::traits::dynamic_params::codec::Encode,
				$crate::traits::dynamic_params::codec::Decode,
				$crate::traits::dynamic_params::codec::MaxEncodedLen,
				$crate::traits::dynamic_params::RuntimeDebug,
				$crate::traits::dynamic_params::scale_info::TypeInfo
			)]
			$vis enum [<$name Key>] {
				$(
					#[codec(index = $index)]
					$key_name($key_name),
				)*
			}

			#[derive(
				Clone,
				PartialEq,
				Eq,
				$crate::traits::dynamic_params::codec::Encode,
				$crate::traits::dynamic_params::codec::Decode,
				$crate::traits::dynamic_params::codec::MaxEncodedLen,
				$crate::traits::dynamic_params::RuntimeDebug,
				$crate::traits::dynamic_params::scale_info::TypeInfo
			)]
			$vis enum [<$name Value>] {
				$(
					#[codec(index = $index)]
					$key_name($value_type),
				)*
			}

			impl $crate::traits::dynamic_params::AggregratedKeyValue for $name {
				type AggregratedKey = [<$name Key>];
				type AggregratedValue = [<$name Value>];

				fn into_parts(self) -> (Self::AggregratedKey, Option<Self::AggregratedValue>) {
					match self {
						$(
							$name::$key_name(key, value) => ([<$name Key>]::$key_name(key), value.map([<$name Value>]::$key_name)),
						)*
					}
				}
			}

			$(
				#[derive(
					Clone,
					PartialEq,
					Eq,
					$crate::traits::dynamic_params::codec::Encode,
					$crate::traits::dynamic_params::codec::Decode,
					$crate::traits::dynamic_params::codec::MaxEncodedLen,
					$crate::traits::dynamic_params::RuntimeDebug,
					$crate::traits::dynamic_params::scale_info::TypeInfo
				)]
				$vis struct $key_name $( (pub $key_para) )?;

				impl $crate::traits::dynamic_params::Key for $key_name {
					type Value = $value_type;
					type WrappedValue = [<$key_name Value>];
				}

				impl From<$key_name> for [<$name Key>] {
					fn from(key: $key_name) -> Self {
						[<$name Key>]::$key_name(key)
					}
				}

				impl TryFrom<[<$name Key>]> for $key_name {
					type Error = ();

					fn try_from(key: [<$name Key>]) -> Result<Self, Self::Error> {
						match key {
							[<$name Key>]::$key_name(key) => Ok(key),
							_ => Err(()),
						}
					}
				}

				#[derive(
					Clone,
					PartialEq,
					Eq,
					$crate::traits::dynamic_params::RuntimeDebug
				)]
				$vis struct [<$key_name Value>](pub $value_type);

				impl From<[<$key_name Value>]> for [<$name Value>] {
					fn from(value: [<$key_name Value>]) -> Self {
						[<$name Value>]::$key_name(value.0)
					}
				}

				impl From<($key_name, $value_type)> for $name {
					fn from((key, value): ($key_name, $value_type)) -> Self {
						$name::$key_name(key, Some(value))
					}
				}

				impl From<$key_name> for $name {
					fn from(key: $key_name) -> Self {
						$name::$key_name(key, None)
					}
				}

				impl TryFrom<[<$name Value>]> for [<$key_name Value>] {
					type Error = ();

					fn try_from(value: [<$name Value>]) -> Result<Self, Self::Error> {
						match value {
							[<$name Value>]::$key_name(value) => Ok([<$key_name Value>](value)),
							_ => Err(()),
						}
					}
				}

				impl From<[<$key_name Value>]> for $value_type {
					fn from(value: [<$key_name Value>]) -> Self {
						value.0
					}
				}
			)*
		}
	};
}

/// Define aggregrated parameters types.
///
/// Example:
/// ```
/// # #[macro_use]
/// # extern crate orml_traits;
/// # fn main() {}
/// mod pallet1 {
///     define_parameters! {
///         pub Pallet = {
///             Key1: u64 = 0,
///             Key2(u32): u32 = 1,
///             Key3((u8, u8)): u128 = 2,
///         }
///     }
/// }
///
/// mod pallet2 {
///     define_parameters! {
///         pub Pallet = {
///             Key1: u64 = 0,
///             Key2(u32): u32 = 1,
///             Key3((u8, u8)): u128 = 2,
///         }
///     }
/// }
///
/// define_aggregrated_parameters! {
///     pub AggregratedPallet = {
///         Pallet1: pallet1::Pallet = 0,
///         Pallet2: pallet2::Pallet = 1,
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_aggregrated_parameters {
	(
		$vis:vis $name:ident = {
			$(
				$parameter_name:ident: $parameter_type:ty = $index:expr
			),+ $(,)?
		}
	) => {
		$crate::traits::dynamic_params::paste::item! {
			#[derive(
				Clone,
				PartialEq,
				Eq,
				$crate::traits::dynamic_params::codec::Encode,
				$crate::traits::dynamic_params::codec::Decode,
				$crate::traits::dynamic_params::codec::MaxEncodedLen,
				$crate::traits::dynamic_params::RuntimeDebug,
				$crate::traits::dynamic_params::scale_info::TypeInfo
			)]
			$vis enum $name {
				$(
					#[codec(index = $index)]
					$parameter_name($parameter_type),
				)*
			}

			#[derive(
				Clone,
				PartialEq,
				Eq,
				$crate::traits::dynamic_params::codec::Encode,
				$crate::traits::dynamic_params::codec::Decode,
				$crate::traits::dynamic_params::codec::MaxEncodedLen,
				$crate::traits::dynamic_params::RuntimeDebug,
				$crate::traits::dynamic_params::scale_info::TypeInfo
			)]
			$vis enum [<$name Key>] {
				$(
					#[codec(index = $index)]
					$parameter_name(<$parameter_type as $crate::traits::dynamic_params::AggregratedKeyValue>::AggregratedKey),
				)*
			}

			#[derive(
				Clone,
				PartialEq,
				Eq,
				$crate::traits::dynamic_params::codec::Encode,
				$crate::traits::dynamic_params::codec::Decode,
				$crate::traits::dynamic_params::codec::MaxEncodedLen,
				$crate::traits::dynamic_params::RuntimeDebug,
				$crate::traits::dynamic_params::scale_info::TypeInfo
			)]
			$vis enum [<$name Value>] {
				$(
					#[codec(index = $index)]
					$parameter_name(<$parameter_type as $crate::traits::dynamic_params::AggregratedKeyValue>::AggregratedValue),
				)*
			}

			impl $crate::traits::dynamic_params::AggregratedKeyValue for $name {
				type AggregratedKey = [<$name Key>];
				type AggregratedValue = [<$name Value>];

				fn into_parts(self) -> (Self::AggregratedKey, Option<Self::AggregratedValue>) {
					match self {
						$(
							$name::$parameter_name(parameter) => {
								let (key, value) = parameter.into_parts();
								([<$name Key>]::$parameter_name(key), value.map([<$name Value>]::$parameter_name))
							},
						)*
					}
				}
			}

			$(
				impl $crate::traits::dynamic_params::FromKey<<$parameter_type as $crate::traits::dynamic_params::AggregratedKeyValue>::AggregratedKey> for [<$name Key>] {
					fn from_key(key: <$parameter_type as $crate::traits::dynamic_params::AggregratedKeyValue>::AggregratedKey) -> Self {
						[<$name Key>]::$parameter_name(key)
					}
				}

				impl $crate::traits::dynamic_params::TryFromKey<[<$name Value>]> for <$parameter_type as $crate::traits::dynamic_params::AggregratedKeyValue>::AggregratedValue {
					type Error = ();

					fn try_from_key(value: [<$name Value>]) -> Result<Self, Self::Error> {
						match value {
							[<$name Value>]::$parameter_name(value) => Ok(value),
							_ => Err(()),
						}
					}
				}
			)*
		}
	};
}

#[cfg(test)]
mod tests {
	pub mod pallet1 {
		define_parameters! {
			pub Parameters = {
				Key1: u64 = 0,
				Key2(u32): u32 = 1,
				Key3((u8, u8)): u128 = 2,
			}
		}
	}
	pub mod pallet2 {
		define_parameters! {
			pub Parameters = {
				Key1: u64 = 0,
				Key2(u32): u32 = 2,
				Key3((u8, u8)): u128 = 4,
			}
		}
	}
	define_aggregrated_parameters! {
		pub RuntimeParameters = {
			Pallet1: pallet1::Parameters = 0,
			Pallet2: pallet2::Parameters = 3,
		}
	}

	#[test]
	fn test_define_parameters_key_convert() {
		let key1 = pallet1::Key1;
		let parameter_key: pallet1::ParametersKey = key1.clone().into();
		let key1_2: pallet1::Key1 = parameter_key.clone().try_into().unwrap();

		assert_eq!(key1, key1_2);
		assert_eq!(parameter_key, pallet1::ParametersKey::Key1(key1));

		let key2 = pallet1::Key2(1);
		let parameter_key: pallet1::ParametersKey = key2.clone().into();
		let key2_2: pallet1::Key2 = parameter_key.clone().try_into().unwrap();

		assert_eq!(key2, key2_2);
		assert_eq!(parameter_key, pallet1::ParametersKey::Key2(key2));
	}

	#[test]
	fn test_define_parameters_value_convert() {
		let value1 = pallet1::Key1Value(1);
		let parameter_value: pallet1::ParametersValue = value1.clone().into();
		let value1_2: pallet1::Key1Value = parameter_value.clone().try_into().unwrap();

		assert_eq!(value1, value1_2);
		assert_eq!(parameter_value, pallet1::ParametersValue::Key1(1));

		let value2 = pallet1::Key2Value(2);
		let parameter_value: pallet1::ParametersValue = value2.clone().into();
		let value2_2: pallet1::Key2Value = parameter_value.clone().try_into().unwrap();

		assert_eq!(value2, value2_2);
		assert_eq!(parameter_value, pallet1::ParametersValue::Key2(2));
	}

	#[test]
	fn test_define_parameters_aggregrated_key_value() {
		use crate::parameters::AggregratedKeyValue;

		let kv1 = pallet1::Parameters::Key1(pallet1::Key1, None);
		let (key1, value1) = kv1.clone().into_parts();

		assert_eq!(key1, pallet1::ParametersKey::Key1(pallet1::Key1));
		assert_eq!(value1, None);

		let kv2 = pallet1::Parameters::Key2(pallet1::Key2(1), Some(2));
		let (key2, value2) = kv2.clone().into_parts();

		assert_eq!(key2, pallet1::ParametersKey::Key2(pallet1::Key2(1)));
		assert_eq!(value2, Some(pallet1::ParametersValue::Key2(2)));
	}

	#[test]
	fn test_define_aggregrated_parameters_key_convert() {
		use crate::parameters::workaround::IntoKey;
		use codec::Encode;

		let key1 = pallet1::Key1;
		let parameter_key: pallet1::ParametersKey = key1.clone().into();
		let runtime_key: RuntimeParametersKey = parameter_key.clone().into_key();

		assert_eq!(runtime_key, RuntimeParametersKey::Pallet1(pallet1::ParametersKey::Key1(key1)));
		assert_eq!(runtime_key.encode(), vec![0, 0]);

		let key2 = pallet2::Key2(1);
		let parameter_key: pallet2::ParametersKey = key2.clone().into();
		let runtime_key: RuntimeParametersKey = parameter_key.clone().into_key();

		assert_eq!(runtime_key, RuntimeParametersKey::Pallet2(pallet2::ParametersKey::Key2(key2)));
		assert_eq!(runtime_key.encode(), vec![3, 2, 1, 0, 0, 0]);
	}

	#[test]
	fn test_define_aggregrated_parameters_aggregrated_key_value() {
		use crate::parameters::AggregratedKeyValue;

		let kv1 = RuntimeParameters::Pallet1(pallet1::Parameters::Key1(pallet1::Key1, None));
		let (key1, value1) = kv1.clone().into_parts();

		assert_eq!(
			key1,
			RuntimeParametersKey::Pallet1(pallet1::ParametersKey::Key1(pallet1::Key1))
		);
		assert_eq!(value1, None);

		let kv2 = RuntimeParameters::Pallet2(pallet2::Parameters::Key2(pallet2::Key2(1), Some(2)));
		let (key2, value2) = kv2.clone().into_parts();

		assert_eq!(
			key2,
			RuntimeParametersKey::Pallet2(pallet2::ParametersKey::Key2(pallet2::Key2(1)))
		);
		assert_eq!(
			value2,
			Some(RuntimeParametersValue::Pallet2(pallet2::ParametersValue::Key2(2)))
		);
	}
}
