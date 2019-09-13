// Copyright 2019 Parity Technologies (UK) Ltd.
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

use primitives::crypto::{KeyTypeId, CryptoType, IsWrappedBy, Public};
#[cfg(feature = "std")]
use primitives::crypto::Pair;
use codec::Codec;

/// An application-specific key.
pub trait AppKey: 'static + Send + Sync + Sized + CryptoType + Clone {
	/// The corresponding type as a generic crypto type.
	type UntypedGeneric: IsWrappedBy<Self>;

	/// The corresponding public key type in this application scheme.
	type Public: AppPublic;

	/// The corresponding key pair type in this application scheme.
	#[cfg(feature="std")]
	type Pair: AppPair;

	/// The corresponding signature type in this application scheme.
	type Signature: AppSignature;

	/// An identifier for this application-specific key type.
	const ID: KeyTypeId;
}

/// Type which implements Debug and Hash in std, not when no-std (std variant).
#[cfg(feature = "std")]
pub trait MaybeDebugHash: std::fmt::Debug + std::hash::Hash {}
#[cfg(feature = "std")]
impl<T: std::fmt::Debug + std::hash::Hash> MaybeDebugHash for T {}

/// Type which implements Debug and Hash in std, not when no-std (no-std variant).
#[cfg(not(feature = "std"))]
pub trait MaybeDebugHash {}
#[cfg(not(feature = "std"))]
impl<T> MaybeDebugHash for T {}

/// A application's public key.
pub trait AppPublic: AppKey + Public + Ord + PartialOrd + Eq + PartialEq + MaybeDebugHash + codec::Codec {
	/// The wrapped type which is just a plain instance of `Public`.
	type Generic:
		IsWrappedBy<Self> + Public + Ord + PartialOrd + Eq + PartialEq + MaybeDebugHash + codec::Codec;
}

/// A application's key pair.
#[cfg(feature = "std")]
pub trait AppPair: AppKey + Pair<Public=<Self as AppKey>::Public> {
	/// The wrapped type which is just a plain instance of `Pair`.
	type Generic: IsWrappedBy<Self> + Pair<Public=<<Self as AppKey>::Public as AppPublic>::Generic>;
}

/// A application's signature.
pub trait AppSignature: AppKey + Eq + PartialEq + MaybeDebugHash {
	/// The wrapped type which is just a plain instance of `Signature`.
	type Generic: IsWrappedBy<Self> + Eq + PartialEq + MaybeDebugHash;
}

/// A runtime interface for a public key.
pub trait RuntimePublic: Sized {
	/// The signature that will be generated when signing with the corresponding private key.
	type Signature: Codec + MaybeDebugHash + Eq + PartialEq + Clone;

	/// Returns all public keys for the given key type in the keystore.
	fn all(key_type: KeyTypeId) -> crate::Vec<Self>;

	/// Generate a public/private pair for the given key type and store it in the keystore.
	///
	/// Returns the generated public key.
	fn generate_pair(key_type: KeyTypeId, seed: Option<&str>) -> Self;

	/// Sign the given message with the corresponding private key of this public key.
	///
	/// The private key will be requested from the keystore using the given key type.
	///
	/// Returns the signature or `None` if the private key could not be found or some other error
	/// occurred.
	fn sign<M: AsRef<[u8]>>(&self, key_type: KeyTypeId, msg: &M) -> Option<Self::Signature>;

	/// Verify that the given signature matches the given message using this public key.
	fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool;
}

/// A runtime interface for an application's public key.
pub trait RuntimeAppPublic: Sized  {
	/// An identifier for this application-specific key type.
	const ID: KeyTypeId;

	/// The signature that will be generated when signing with the corresponding private key.
	type Signature: Codec + MaybeDebugHash + Eq + PartialEq + Clone;

	/// Returns all public keys for this application in the keystore.
	fn all() -> crate::Vec<Self>;

	/// Generate a public/private pair and store it in the keystore.
	///
	/// Returns the generated public key.
	fn generate_pair(seed: Option<&str>) -> Self;

	/// Sign the given message with the corresponding private key of this public key.
	///
	/// The private key will be requested from the keystore.
	///
	/// Returns the signature or `None` if the private key could not be found or some other error
	/// occurred.
	fn sign<M: AsRef<[u8]>>(&self, msg: &M) -> Option<Self::Signature>;

	/// Verify that the given signature matches the given message using this public key.
	fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool;
}
