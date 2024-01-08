// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! This is a wrapper for the parameters pallet that allows to also associate a key with a param. It
//! is forked from the ORML repo.

// macro re-exports
pub mod __private {
	#[doc(hidden)]
	pub use codec;
	#[doc(hidden)]
	pub use frame_support;
	use frame_support::Parameter;
	#[doc(hidden)]
	pub use paste;
	#[doc(hidden)]
	pub use scale_info;
}

pub trait RuntimeParameterStore {
	type AggregratedKeyValue: AggregratedKeyValue;

	fn get<KV, K>(key: K) -> Option<K::Value>
	where
		KV: AggregratedKeyValue,
		K: Key + Into<<KV as AggregratedKeyValue>::AggregratedKey>,
		<KV as AggregratedKeyValue>::AggregratedKey:
			Into2<<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey>,
		<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue:
			TryInto2<<KV as AggregratedKeyValue>::AggregratedValue>,
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
		Into2<<<PS as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey>,
	<KV as AggregratedKeyValue>::AggregratedValue:
		TryFrom2<<<PS as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue>,
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
	pub trait From2<T>: Sized {
		#[must_use]
		fn from2(value: T) -> Self;
	}

	pub trait Into2<T>: Sized {
		#[must_use]
		fn into2(self) -> T;
	}

	impl<T, U> Into2<U> for T
	where
		U: From2<T>,
	{
		#[inline]
		fn into2(self) -> U {
			U::from2(self)
		}
	}

	pub trait TryInto2<T>: Sized {
		type Error;

		fn try_into2(self) -> Result<T, Self::Error>;
	}

	pub trait TryFrom2<T>: Sized {
		type Error;

		fn try_from2(value: T) -> Result<Self, Self::Error>;
	}

	impl<T, U> TryInto2<U> for T
	where
		U: TryFrom2<T>,
	{
		type Error = U::Error;

		#[inline]
		fn try_into2(self) -> Result<U, U::Error> {
			U::try_from2(self)
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
		$super::paste::item! {
			#[derive(
				Clone,
				PartialEq,
				Eq,
				$super::codec::Encode,
				$super::codec::Decode,
				$super::codec::MaxEncodedLen,
				$super::frame_support::RuntimeDebug,
				$super::scale_info::TypeInfo
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
				$super::codec::Encode,
				$super::codec::Decode,
				$super::codec::MaxEncodedLen,
				$super::frame_support::RuntimeDebug,
				$super::scale_info::TypeInfo
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
				$super::codec::Encode,
				$super::codec::Decode,
				$super::codec::MaxEncodedLen,
				$super::frame_support::RuntimeDebug,
				$super::scale_info::TypeInfo
			)]
			$vis enum [<$name Value>] {
				$(
					#[codec(index = $index)]
					$key_name($value_type),
				)*
			}

			impl $super::AggregratedKeyValue for $name {
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
					$super::codec::Encode,
					$super::codec::Decode,
					$super::codec::MaxEncodedLen,
					$super::frame_support::RuntimeDebug,
					$super::scale_info::TypeInfo
				)]
				$vis struct $key_name $( (pub $key_para) )?;

				impl $super::Key for $key_name {
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
					$super::frame_support::RuntimeDebug
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
		$super::paste::item! {
			#[derive(
				Clone,
				PartialEq,
				Eq,
				$super::codec::Encode,
				$super::codec::Decode,
				$super::codec::MaxEncodedLen,
				$super::frame_support::RuntimeDebug,
				$super::scale_info::TypeInfo
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
				$super::codec::Encode,
				$super::codec::Decode,
				$super::codec::MaxEncodedLen,
				$super::frame_support::RuntimeDebug,
				$super::scale_info::TypeInfo
			)]
			$vis enum [<$name Key>] {
				$(
					#[codec(index = $index)]
					$parameter_name(<$parameter_type as $super::AggregratedKeyValue>::AggregratedKey),
				)*
			}

			#[derive(
				Clone,
				PartialEq,
				Eq,
				$super::codec::Encode,
				$super::codec::Decode,
				$super::codec::MaxEncodedLen,
				$super::frame_support::RuntimeDebug,
				$super::scale_info::TypeInfo
			)]
			$vis enum [<$name Value>] {
				$(
					#[codec(index = $index)]
					$parameter_name(<$parameter_type as $super::AggregratedKeyValue>::AggregratedValue),
				)*
			}

			impl $super::AggregratedKeyValue for $name {
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
				impl $super::From2<<$parameter_type as $super::AggregratedKeyValue>::AggregratedKey> for [<$name Key>] {
					fn from2(key: <$parameter_type as $super::AggregratedKeyValue>::AggregratedKey) -> Self {
						[<$name Key>]::$parameter_name(key)
					}
				}

				impl $super::TryFrom2<[<$name Value>]> for <$parameter_type as $super::AggregratedKeyValue>::AggregratedValue {
					type Error = ();

					fn try_from2(value: [<$name Value>]) -> Result<Self, Self::Error> {
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
		use super::AggregratedKeyValue;

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
		use super::workaround::Into2;
		use codec::Encode;

		let key1 = pallet1::Key1;
		let parameter_key: pallet1::ParametersKey = key1.clone().into();
		let runtime_key: RuntimeParametersKey = parameter_key.clone().into2();

		assert_eq!(runtime_key, RuntimeParametersKey::Pallet1(pallet1::ParametersKey::Key1(key1)));
		assert_eq!(runtime_key.encode(), vec![0, 0]);

		let key2 = pallet2::Key2(1);
		let parameter_key: pallet2::ParametersKey = key2.clone().into();
		let runtime_key: RuntimeParametersKey = parameter_key.clone().into2();

		assert_eq!(runtime_key, RuntimeParametersKey::Pallet2(pallet2::ParametersKey::Key2(key2)));
		assert_eq!(runtime_key.encode(), vec![3, 2, 1, 0, 0, 0]);
	}

	#[test]
	fn test_define_aggregrated_parameters_aggregrated_key_value() {
		use super::AggregratedKeyValue;

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
