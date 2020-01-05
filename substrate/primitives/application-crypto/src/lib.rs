// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Traits and macros for constructing application specific strongly typed crypto wrappers.

#![warn(missing_docs)]

#![cfg_attr(not(feature = "std"), no_std)]

#[doc(hidden)]
pub use sp_core::{self, crypto::{CryptoType, Public, Derive, IsWrappedBy, Wraps}, RuntimeDebug};
#[doc(hidden)]
#[cfg(feature = "full_crypto")]
pub use sp_core::crypto::{SecretStringError, DeriveJunction, Ss58Codec, Pair};
pub use sp_core::{crypto::{KeyTypeId, key_types}};

#[doc(hidden)]
pub use codec;
#[doc(hidden)]
#[cfg(feature = "std")]
pub use serde;
#[doc(hidden)]
pub use sp_std::{ops::Deref, vec::Vec};

pub mod ed25519;
pub mod sr25519;
mod traits;

pub use traits::*;

/// Declares Public, Pair, Signature types which are functionally equivalent to `$pair`, but are new
/// Application-specific types whose identifier is `$key_type`.
///
/// ```rust
///# use sp_application_crypto::{app_crypto, wrap, ed25519, KeyTypeId};
/// // Declare a new set of crypto types using Ed25519 logic that identifies as `KeyTypeId`
/// // of value `b"fuba"`.
/// app_crypto!(ed25519, KeyTypeId(*b"_uba"));
/// ```
#[cfg(feature = "full_crypto")]
#[macro_export]
macro_rules! app_crypto {
	($module:ident, $key_type:expr) => {
		$crate::app_crypto_public_full_crypto!($module::Public, $key_type);
		$crate::app_crypto_public_common!($module::Public, $module::Signature, $key_type);
		$crate::app_crypto_signature_full_crypto!($module::Signature, $key_type);
		$crate::app_crypto_signature_common!($module::Signature, $key_type);
		$crate::app_crypto_pair!($module::Pair, $key_type);
	};
}

/// Declares Public, Pair, Signature types which are functionally equivalent to `$pair`, but are new
/// Application-specific types whose identifier is `$key_type`.
///
/// ```rust
///# use sp_application_crypto::{app_crypto, wrap, ed25519, KeyTypeId};
/// // Declare a new set of crypto types using Ed25519 logic that identifies as `KeyTypeId`
/// // of value `b"fuba"`.
/// app_crypto!(ed25519, KeyTypeId(*b"_uba"));
/// ```
#[cfg(not(feature = "full_crypto"))]
#[macro_export]
macro_rules! app_crypto {
	($module:ident, $key_type:expr) => {
		$crate::app_crypto_public_not_full_crypto!($module::Public, $key_type);
		$crate::app_crypto_public_common!($module::Public, $module::Signature, $key_type);
		$crate::app_crypto_signature_not_full_crypto!($module::Signature, $key_type);
		$crate::app_crypto_signature_common!($module::Signature, $key_type);
	};
}

/// Declares Pair type which is functionally equivalent to `$pair`, but is new
/// Application-specific type whose identifier is `$key_type`.
#[macro_export]
macro_rules! app_crypto_pair {
	($pair:ty, $key_type:expr) => {
		$crate::wrap!{
			/// A generic `AppPublic` wrapper type over $pair crypto; this has no specific App.
			#[derive(Clone)]
			pub struct Pair($pair);
		}

		impl $crate::CryptoType for Pair {
			type Pair = Pair;
		}

		impl $crate::Pair for Pair {
			type Public = Public;
			type Seed = <$pair as $crate::Pair>::Seed;
			type Signature = Signature;
			type DeriveError = <$pair as $crate::Pair>::DeriveError;

			#[cfg(feature = "std")]
			fn generate_with_phrase(password: Option<&str>) -> (Self, String, Self::Seed) {
				let r = <$pair>::generate_with_phrase(password);
				(Self(r.0), r.1, r.2)
			}
			#[cfg(feature = "std")]
			fn from_phrase(phrase: &str, password: Option<&str>)
				-> Result<(Self, Self::Seed), $crate::SecretStringError>
			{
				<$pair>::from_phrase(phrase, password).map(|r| (Self(r.0), r.1))
			}
			fn derive<
				Iter: Iterator<Item=$crate::DeriveJunction>
			>(&self, path: Iter, seed: Option<Self::Seed>) -> Result<(Self, Option<Self::Seed>), Self::DeriveError> {
				self.0.derive(path, seed).map(|x| (Self(x.0), x.1))
			}
			fn from_seed(seed: &Self::Seed) -> Self { Self(<$pair>::from_seed(seed)) }
			fn from_seed_slice(seed: &[u8]) -> Result<Self, $crate::SecretStringError> {
				<$pair>::from_seed_slice(seed).map(Self)
			}
			fn sign(&self, msg: &[u8]) -> Self::Signature {
				Signature(self.0.sign(msg))
			}
			fn verify<M: AsRef<[u8]>>(
				sig: &Self::Signature,
				message: M,
				pubkey: &Self::Public,
			) -> bool {
				<$pair>::verify(&sig.0, message, pubkey.as_ref())
			}
			fn verify_weak<P: AsRef<[u8]>, M: AsRef<[u8]>>(
				sig: &[u8],
				message: M,
				pubkey: P,
			) -> bool {
				<$pair>::verify_weak(sig, message, pubkey)
			}
			fn public(&self) -> Self::Public { Public(self.0.public()) }
			fn to_raw_vec(&self) -> $crate::Vec<u8> { self.0.to_raw_vec() }
		}

		impl $crate::AppKey for Pair {
			type UntypedGeneric = $pair;
			type Public = Public;
			type Pair = Pair;
			type Signature = Signature;
			const ID: $crate::KeyTypeId = $key_type;
		}

		impl $crate::AppPair for Pair {
			type Generic = $pair;
		}
	};
}

/// Declares Public type which is functionally equivalent to `$public`, but is new
/// Application-specific type whose identifier is `$key_type`.
/// can only be used together with `full_crypto` feature
/// For full functionality, app_crypto_public_common! must be called too.
#[macro_export]
macro_rules! app_crypto_public_full_crypto {
	($public:ty, $key_type:expr) => {
		$crate::wrap!{
			/// A generic `AppPublic` wrapper type over $public crypto; this has no specific App.
			#[derive(
				Clone, Default, Eq, PartialEq, Ord, PartialOrd,
				$crate::codec::Encode,
				$crate::codec::Decode,
				$crate::RuntimeDebug,
			)]
			#[derive(Hash)]
			pub struct Public($public);
		}

		impl $crate::CryptoType for Public {
			type Pair = Pair;
		}

		impl $crate::AppKey for Public {
			type UntypedGeneric = $public;
			type Public = Public;
			type Pair = Pair;
			type Signature = Signature;
			const ID: $crate::KeyTypeId = $key_type;
		}
	}
}

/// Declares Public type which is functionally equivalent to `$public`, but is new
/// Application-specific type whose identifier is `$key_type`.
/// can only be used without `full_crypto` feature
/// For full functionality, app_crypto_public_common! must be called too.
#[macro_export]
macro_rules! app_crypto_public_not_full_crypto {
	($public:ty, $key_type:expr) => {
		$crate::wrap!{
			/// A generic `AppPublic` wrapper type over $public crypto; this has no specific App.
			#[derive(
				Clone, Default, Eq, PartialEq, Ord, PartialOrd,
				$crate::codec::Encode,
				$crate::codec::Decode,
				$crate::RuntimeDebug,
			)]
			pub struct Public($public);
		}

		impl $crate::CryptoType for Public {}

		impl $crate::AppKey for Public {
			type UntypedGeneric = $public;
			type Public = Public;
			type Signature = Signature;
			const ID: $crate::KeyTypeId = $key_type;
		}
	}
}

/// Declares Public type which is functionally equivalent to `$public`, but is new
/// Application-specific type whose identifier is `$key_type`.
/// For full functionality, app_crypto_public_(not)_full_crypto! must be called too.
#[macro_export]
macro_rules! app_crypto_public_common {
	($public:ty, $sig:ty, $key_type:expr) => {
		impl $crate::Derive for Public {
			#[cfg(feature = "std")]
			fn derive<Iter: Iterator<Item=$crate::DeriveJunction>>(&self,
				path: Iter
			) -> Option<Self> {
				self.0.derive(path).map(Self)
			}
		}

		#[cfg(feature = "std")]
		impl std::fmt::Display for Public {
			fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
				use $crate::Ss58Codec;
				write!(f, "{}", self.0.to_ss58check())
			}
		}
		#[cfg(feature = "std")]
		impl $crate::serde::Serialize for Public {
			fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> where
				S: $crate::serde::Serializer
			{
				use $crate::Ss58Codec;
				serializer.serialize_str(&self.to_ss58check())
			}
		}
		#[cfg(feature = "std")]
		impl<'de> $crate::serde::Deserialize<'de> for Public {
			fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error> where
				D: $crate::serde::Deserializer<'de>
			{
				use $crate::Ss58Codec;
				Public::from_ss58check(&String::deserialize(deserializer)?)
					.map_err(|e| $crate::serde::de::Error::custom(format!("{:?}", e)))
			}
		}

		impl AsRef<[u8]> for Public {
			fn as_ref(&self) -> &[u8] { self.0.as_ref() }
		}

		impl AsMut<[u8]> for Public {
			fn as_mut(&mut self) -> &mut [u8] { self.0.as_mut() }
		}

		impl $crate::Public for Public {
			fn from_slice(x: &[u8]) -> Self { Self(<$public>::from_slice(x)) }
		}

		impl $crate::AppPublic for Public {
			type Generic = $public;
		}

		impl $crate::RuntimeAppPublic for Public where $public: $crate::RuntimePublic<Signature=$sig> {
			const ID: $crate::KeyTypeId = $key_type;
			type Signature = Signature;

			fn all() -> $crate::Vec<Self> {
				<$public as $crate::RuntimePublic>::all($key_type).into_iter().map(Self).collect()
			}

			fn generate_pair(seed: Option<$crate::Vec<u8>>) -> Self {
				Self(<$public as $crate::RuntimePublic>::generate_pair($key_type, seed))
			}

			fn sign<M: AsRef<[u8]>>(&self, msg: &M) -> Option<Self::Signature> {
				<$public as $crate::RuntimePublic>::sign(
					self.as_ref(),
					$key_type,
					msg,
				).map(Signature)
			}

			fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool {
				<$public as $crate::RuntimePublic>::verify(self.as_ref(), msg, &signature.as_ref())
			}
		}
	}
}

/// Declares Signature type which is functionally equivalent to `$sig`, but is new
/// Application-specific type whose identifier is `$key_type`.
/// can only be used together with `full_crypto` feature
/// For full functionality, app_crypto_public_common! must be called too.
#[macro_export]
macro_rules! app_crypto_signature_full_crypto {
	($sig:ty, $key_type:expr) => {
		$crate::wrap! {
			/// A generic `AppPublic` wrapper type over $public crypto; this has no specific App.
			#[derive(Clone, Default, Eq, PartialEq,
				$crate::codec::Encode,
				$crate::codec::Decode,
				$crate::RuntimeDebug,
			)]
			#[derive(Hash)]
			pub struct Signature($sig);
		}

		impl $crate::CryptoType for Signature {
			type Pair = Pair;
		}

		impl $crate::AppKey for Signature {
			type UntypedGeneric = $sig;
			type Public = Public;
			type Pair = Pair;
			type Signature = Signature;
			const ID: $crate::KeyTypeId = $key_type;
		}
	}
}

/// Declares Signature type which is functionally equivalent to `$sig`, but is new
/// Application-specific type whose identifier is `$key_type`.
/// can only be used without `full_crypto` feature
/// For full functionality, app_crypto_public_common! must be called too.
#[macro_export]
macro_rules! app_crypto_signature_not_full_crypto {
	($sig:ty, $key_type:expr) => {
		$crate::wrap! {
			/// A generic `AppPublic` wrapper type over $public crypto; this has no specific App.
			#[derive(Clone, Default, Eq, PartialEq,
				$crate::codec::Encode,
				$crate::codec::Decode,
				$crate::RuntimeDebug,
			)]
			pub struct Signature($sig);
		}

		impl $crate::CryptoType for Signature {}

		impl $crate::AppKey for Signature {
			type UntypedGeneric = $sig;
			type Public = Public;
			type Signature = Signature;
			const ID: $crate::KeyTypeId = $key_type;
		}
	}
}

/// Declares Signature type which is functionally equivalent to `$sig`, but is new
/// Application-specific type whose identifier is `$key_type`.
/// For full functionality, app_crypto_public_(not)_full_crypto! must be called too.
#[macro_export]
macro_rules! app_crypto_signature_common {
	($sig:ty, $key_type:expr) => {
		impl $crate::Deref for Signature {
			type Target = [u8];

			fn deref(&self) -> &Self::Target { self.0.as_ref() }
		}

		impl AsRef<[u8]> for Signature {
			fn as_ref(&self) -> &[u8] { self.0.as_ref() }
		}

		impl $crate::AppSignature for Signature {
			type Generic = $sig;
		}
	}
}

/// Implement bidirectional `From` and on-way `AsRef`/`AsMut` for two types, `$inner` and `$outer`.
///
/// ```rust
/// sp_application_crypto::wrap! {
///     pub struct Wrapper(u32);
/// }
/// ```
#[macro_export]
macro_rules! wrap {
	($( #[ $attr:meta ] )* struct $outer:ident($inner:ty);) => {
		$( #[ $attr ] )*
		struct $outer( $inner );
		$crate::wrap!($inner, $outer);
	};
	($( #[ $attr:meta ] )* pub struct $outer:ident($inner:ty);) => {
		$( #[ $attr ] )*
		pub struct $outer( $inner );
		$crate::wrap!($inner, $outer);
	};
	($inner:ty, $outer:ty) => {
		impl $crate::Wraps for $outer {
			type Inner = $inner;
		}
		impl From<$inner> for $outer {
			fn from(inner: $inner) -> Self {
				Self(inner)
			}
		}
		impl From<$outer> for $inner {
			fn from(outer: $outer) -> Self {
				outer.0
			}
		}
		impl AsRef<$inner> for $outer {
			fn as_ref(&self) -> &$inner {
				&self.0
			}
		}
		impl AsMut<$inner> for $outer {
			fn as_mut(&mut self) -> &mut $inner {
				&mut self.0
			}
		}
	}
}
