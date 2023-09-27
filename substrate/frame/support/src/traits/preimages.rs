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

//! Stuff for dealing with hashed preimages.

use codec::{Decode, Encode, EncodeLike, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;
use sp_runtime::{
	traits::{ConstU32, Hash},
	DispatchError,
};
use sp_std::borrow::Cow;

pub type BoundedInline = crate::BoundedVec<u8, ConstU32<128>>;

/// The maximum we expect a single legacy hash lookup to be.
const MAX_LEGACY_LEN: u32 = 1_000_000;

#[derive(Encode, Decode, MaxEncodedLen, Clone, Eq, PartialEq, TypeInfo, RuntimeDebug)]
#[codec(mel_bound())]
pub enum Bounded<T, H: Hash> {
	/// A hash with no preimage length. We do not support creation of this except
	/// for transitioning from legacy state. In the future we will make this a pure
	/// `Dummy` item storing only the final `dummy` field.
	Legacy { hash: H::Output, dummy: sp_std::marker::PhantomData<T> },
	/// A an bounded `Call`. Its encoding must be at most 128 bytes.
	Inline(BoundedInline),
	/// A hash of the call together with an upper limit for its size.`
	Lookup { hash: H::Output, len: u32 },
}

impl<T, H: Hash> Bounded<T, H> {
	/// Casts the wrapped type into something that encodes alike.
	///
	/// # Examples
	/// ```
	/// use frame_support::{traits::Bounded, sp_runtime::traits::BlakeTwo256};
	///
	/// // Transmute from `String` to `&str`.
	/// let x: Bounded<String, BlakeTwo256> = Bounded::Inline(Default::default());
	/// let _: Bounded<&str, BlakeTwo256> = x.transmute();
	/// ```
	pub fn transmute<S: Encode>(self) -> Bounded<S, H>
	where
		T: Encode + EncodeLike<S>,
	{
		use Bounded::*;
		match self {
			Legacy { hash, .. } => Legacy { hash, dummy: sp_std::marker::PhantomData },
			Inline(x) => Inline(x),
			Lookup { hash, len } => Lookup { hash, len },
		}
	}

	/// Returns the hash of the preimage.
	///
	/// The hash is re-calculated every time if the preimage is inlined.
	pub fn hash(&self) -> H::Output {
		use Bounded::*;
		match self {
			Lookup { hash, .. } | Legacy { hash, .. } => *hash,
			Inline(x) => <H as Hash>::hash(x.as_ref()),
		}
	}

	/// Returns the hash to lookup the preimage.
	///
	/// If this is a `Bounded::Inline`, `None` is returned as no lookup is required.
	pub fn lookup_hash(&self) -> Option<H::Output> {
		use Bounded::*;
		match self {
			Lookup { hash, .. } | Legacy { hash, .. } => Some(*hash),
			Inline(_) => None,
		}
	}

	/// Returns the length of the preimage or `None` if the length is unknown.
	pub fn len(&self) -> Option<u32> {
		match self {
			Self::Legacy { .. } => None,
			Self::Inline(i) => Some(i.len() as u32),
			Self::Lookup { len, .. } => Some(*len),
		}
	}

	/// Returns whether the image will require a lookup to be peeked.
	pub fn lookup_needed(&self) -> bool {
		match self {
			Self::Inline(..) => false,
			Self::Legacy { .. } | Self::Lookup { .. } => true,
		}
	}

	/// The maximum length of the lookup that is needed to peek `Self`.
	pub fn lookup_len(&self) -> Option<u32> {
		match self {
			Self::Inline(..) => None,
			Self::Legacy { .. } => Some(MAX_LEGACY_LEN),
			Self::Lookup { len, .. } => Some(*len),
		}
	}

	/// Constructs a `Lookup` bounded item.
	pub fn unrequested(hash: H::Output, len: u32) -> Self {
		Self::Lookup { hash, len }
	}

	/// Constructs a `Legacy` bounded item.
	#[deprecated = "This API is only for transitioning to Scheduler v3 API"]
	pub fn from_legacy_hash(hash: impl Into<H::Output>) -> Self {
		Self::Legacy { hash: hash.into(), dummy: sp_std::marker::PhantomData }
	}
}

pub type FetchResult = Result<Cow<'static, [u8]>, DispatchError>;

/// A interface for looking up preimages from their hash on chain.
pub trait QueryPreimage {
	/// The hasher used in the runtime.
	type H: Hash;

	/// Returns whether a preimage exists for a given hash and if so its length.
	fn len(hash: &<Self::H as sp_core::Hasher>::Out) -> Option<u32>;

	/// Returns the preimage for a given hash. If given, `len` must be the size of the preimage.
	fn fetch(hash: &<Self::H as sp_core::Hasher>::Out, len: Option<u32>) -> FetchResult;

	/// Returns whether a preimage request exists for a given hash.
	fn is_requested(hash: &<Self::H as sp_core::Hasher>::Out) -> bool;

	/// Request that someone report a preimage. Providers use this to optimise the economics for
	/// preimage reporting.
	fn request(hash: &<Self::H as sp_core::Hasher>::Out);

	/// Cancel a previous preimage request.
	fn unrequest(hash: &<Self::H as sp_core::Hasher>::Out);

	/// Request that the data required for decoding the given `bounded` value is made available.
	fn hold<T>(bounded: &Bounded<T, Self::H>) {
		use Bounded::*;
		match bounded {
			Inline(..) => {},
			Legacy { hash, .. } | Lookup { hash, .. } => Self::request(hash),
		}
	}

	/// No longer request that the data required for decoding the given `bounded` value is made
	/// available.
	fn drop<T>(bounded: &Bounded<T, Self::H>) {
		use Bounded::*;
		match bounded {
			Inline(..) => {},
			Legacy { hash, .. } | Lookup { hash, .. } => Self::unrequest(hash),
		}
	}

	/// Check to see if all data required for the given `bounded` value is available for its
	/// decoding.
	fn have<T>(bounded: &Bounded<T, Self::H>) -> bool {
		use Bounded::*;
		match bounded {
			Inline(..) => true,
			Legacy { hash, .. } | Lookup { hash, .. } => Self::len(hash).is_some(),
		}
	}

	/// Create a `Bounded` instance based on the `hash` and `len` of the encoded value.
	///
	/// It also directly requests the given `hash` using [`Self::request`].
	///
	/// This may not be `peek`-able or `realize`-able.
	fn pick<T>(hash: <Self::H as sp_core::Hasher>::Out, len: u32) -> Bounded<T, Self::H> {
		Self::request(&hash);
		Bounded::Lookup { hash, len }
	}

	/// Convert the given `bounded` instance back into its original instance, also returning the
	/// exact size of its encoded form if it needed to be looked-up from a stored preimage).
	///
	/// NOTE: This does not remove any data needed for realization. If you will no longer use the
	/// `bounded`, call `realize` instead or call `drop` afterwards.
	fn peek<T: Decode>(bounded: &Bounded<T, Self::H>) -> Result<(T, Option<u32>), DispatchError> {
		use Bounded::*;
		match bounded {
			Inline(data) => T::decode(&mut &data[..]).ok().map(|x| (x, None)),
			Lookup { hash, len } => {
				let data = Self::fetch(hash, Some(*len))?;
				T::decode(&mut &data[..]).ok().map(|x| (x, Some(data.len() as u32)))
			},
			Legacy { hash, .. } => {
				let data = Self::fetch(hash, None)?;
				T::decode(&mut &data[..]).ok().map(|x| (x, Some(data.len() as u32)))
			},
		}
		.ok_or(DispatchError::Corruption)
	}

	/// Convert the given `bounded` value back into its original instance. If successful,
	/// `drop` any data backing it. This will not break the realisability of independently
	/// created instances of `Bounded` which happen to have identical data.
	fn realize<T: Decode>(
		bounded: &Bounded<T, Self::H>,
	) -> Result<(T, Option<u32>), DispatchError> {
		let r = Self::peek(bounded)?;
		Self::drop(bounded);
		Ok(r)
	}
}

/// A interface for managing preimages to hashes on chain.
///
/// Note that this API does not assume any underlying user is calling, and thus
/// does not handle any preimage ownership or fees. Other system level logic that
/// uses this API should implement that on their own side.
pub trait StorePreimage: QueryPreimage {
	/// The maximum length of preimage we can store.
	///
	/// This is the maximum length of the *encoded* value that can be passed to `bound`.
	const MAX_LENGTH: usize;

	/// Request and attempt to store the bytes of a preimage on chain.
	///
	/// May return `DispatchError::Exhausted` if the preimage is just too big.
	fn note(bytes: Cow<[u8]>) -> Result<<Self::H as sp_core::Hasher>::Out, DispatchError>;

	/// Attempt to clear a previously noted preimage. Exactly the same as `unrequest` but is
	/// provided for symmetry.
	fn unnote(hash: &<Self::H as sp_core::Hasher>::Out) {
		Self::unrequest(hash)
	}

	/// Convert an otherwise unbounded or large value into a type ready for placing in storage.
	///
	/// The result is a type whose `MaxEncodedLen` is 131 bytes.
	///
	/// NOTE: Once this API is used, you should use either `drop` or `realize`.
	/// The value is also noted using [`Self::note`].
	fn bound<T: Encode>(t: T) -> Result<Bounded<T, Self::H>, DispatchError> {
		let data = t.encode();
		let len = data.len() as u32;
		Ok(match BoundedInline::try_from(data) {
			Ok(bounded) => Bounded::Inline(bounded),
			Err(unbounded) => Bounded::Lookup { hash: Self::note(unbounded.into())?, len },
		})
	}
}

impl QueryPreimage for () {
	type H = sp_runtime::traits::BlakeTwo256;

	fn len(_: &sp_core::H256) -> Option<u32> {
		None
	}
	fn fetch(_: &sp_core::H256, _: Option<u32>) -> FetchResult {
		Err(DispatchError::Unavailable)
	}
	fn is_requested(_: &sp_core::H256) -> bool {
		false
	}
	fn request(_: &sp_core::H256) {}
	fn unrequest(_: &sp_core::H256) {}
}

impl StorePreimage for () {
	const MAX_LENGTH: usize = 0;
	fn note(_: Cow<[u8]>) -> Result<sp_core::H256, DispatchError> {
		Err(DispatchError::Exhausted)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::BoundedVec;
	use sp_runtime::{bounded_vec, traits::BlakeTwo256};

	#[test]
	fn bounded_size_is_correct() {
		assert_eq!(<Bounded<Vec<u8>, BlakeTwo256> as MaxEncodedLen>::max_encoded_len(), 131);
	}

	#[test]
	fn bounded_basic_works() {
		let data: BoundedVec<u8, _> = bounded_vec![b'a', b'b', b'c'];
		let len = data.len() as u32;
		let hash = BlakeTwo256::hash(&data).into();

		// Inline works
		{
			let bound: Bounded<Vec<u8>, BlakeTwo256> = Bounded::Inline(data.clone());
			assert_eq!(bound.hash(), hash);
			assert_eq!(bound.len(), Some(len));
			assert!(!bound.lookup_needed());
			assert_eq!(bound.lookup_len(), None);
		}
		// Legacy works
		{
			let bound: Bounded<Vec<u8>, BlakeTwo256> =
				Bounded::Legacy { hash, dummy: Default::default() };
			assert_eq!(bound.hash(), hash);
			assert_eq!(bound.len(), None);
			assert!(bound.lookup_needed());
			assert_eq!(bound.lookup_len(), Some(1_000_000));
		}
		// Lookup works
		{
			let bound: Bounded<Vec<u8>, BlakeTwo256> =
				Bounded::Lookup { hash, len: data.len() as u32 };
			assert_eq!(bound.hash(), hash);
			assert_eq!(bound.len(), Some(len));
			assert!(bound.lookup_needed());
			assert_eq!(bound.lookup_len(), Some(len));
		}
	}

	#[test]
	fn bounded_transmuting_works() {
		let data: BoundedVec<u8, _> = bounded_vec![b'a', b'b', b'c'];

		// Transmute a `String` into a `&str`.
		let x: Bounded<String, BlakeTwo256> = Bounded::Inline(data.clone());
		let y: Bounded<&str, BlakeTwo256> = x.transmute();
		assert_eq!(y, Bounded::Inline(data));
	}
}
