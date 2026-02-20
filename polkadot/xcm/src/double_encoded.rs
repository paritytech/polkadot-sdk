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

use crate::MAX_XCM_DECODE_DEPTH;
use alloc::{boxed::Box, vec::Vec};
use codec::{Decode, DecodeLimit, DecodeWithMemTracking, Encode};
use sp_runtime::traits::TryGetDecodeFn;

struct DoubleEncodedInput<'a> {
	outer_input: Box<&'a mut dyn codec::Input>,
	inner_input: &'a mut &'a [u8],
}

impl<'a> codec::Input for DoubleEncodedInput<'a> {
	fn remaining_len(&mut self) -> Result<Option<usize>, codec::Error> {
		self.inner_input.remaining_len()
	}

	fn read(&mut self, into: &mut [u8]) -> Result<(), codec::Error> {
		self.inner_input.read(into)
	}

	fn read_byte(&mut self) -> Result<u8, codec::Error> {
		self.inner_input.read_byte()
	}

	fn descend_ref(&mut self) -> Result<(), codec::Error> {
		self.outer_input.descend_ref()
	}

	fn ascend_ref(&mut self) {
		self.outer_input.ascend_ref()
	}

	fn on_before_alloc_mem(&mut self, size: usize) -> Result<(), codec::Error> {
		self.outer_input.on_before_alloc_mem(size)
	}
}

/// Wrapper around the encoded and decoded versions of a value.
/// Caches the decoded value once computed.
#[derive(Encode, DecodeWithMemTracking, scale_info::TypeInfo)]
#[codec(encode_bound())]
#[codec(decode_bound())]
#[scale_info(bounds(), skip_type_params(T))]
#[scale_info(replace_segment("staging_xcm", "xcm"))]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DoubleEncoded<T: TryGetDecodeFn> {
	encoded: Vec<u8>,
	#[codec(skip)]
	decoded: Option<T>,
}

impl<T: TryGetDecodeFn> Decode for DoubleEncoded<T> {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let mut obj = Self { encoded: Vec::<u8>::decode(input)?, decoded: None };

		// We also decode the inner double encoded object if possible, in order to make sure that
		// its heap memory and depth are accounted for.
		let mut double_encoded_input =
			DoubleEncodedInput { outer_input: Box::new(input), inner_input: &mut &obj.encoded[..] };
		if let Some(decode_fn) = T::try_get_decode_fn() {
			obj.decoded = Some(decode_fn(&mut double_encoded_input)?);
		}

		Ok(obj)
	}
}

impl<T: TryGetDecodeFn> Clone for DoubleEncoded<T> {
	fn clone(&self) -> Self {
		Self { encoded: self.encoded.clone(), decoded: None }
	}
}

impl<T: TryGetDecodeFn> PartialEq for DoubleEncoded<T> {
	fn eq(&self, other: &Self) -> bool {
		self.encoded.eq(&other.encoded)
	}
}
impl<T: TryGetDecodeFn> Eq for DoubleEncoded<T> {}

impl<T: TryGetDecodeFn> core::fmt::Debug for DoubleEncoded<T> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		array_bytes::bytes2hex("0x", &self.encoded).fmt(f)
	}
}

impl<T: TryGetDecodeFn> From<Vec<u8>> for DoubleEncoded<T> {
	fn from(encoded: Vec<u8>) -> Self {
		Self { encoded, decoded: None }
	}
}

impl<T: TryGetDecodeFn> DoubleEncoded<T> {
	pub fn encoded(&self) -> &[u8] {
		&self.encoded
	}

	/// Converts a `DoubleEncoded<T>` into a `DoubleEncoded<S>`, dropping the decoded value.
	pub fn transmute_encoded<S: TryGetDecodeFn>(self) -> DoubleEncoded<S> {
		DoubleEncoded { encoded: self.encoded, decoded: None }
	}
}

impl<T: Decode + TryGetDecodeFn> DoubleEncoded<T> {
	/// Decode the inner encoded value and store it.
	/// Returns a reference to the value in case of success and `Err(())` in case the decoding
	/// fails.
	pub fn ensure_decoded(&mut self) -> Result<&T, ()> {
		if self.decoded.is_none() {
			self.decoded =
				T::decode_all_with_depth_limit(MAX_XCM_DECODE_DEPTH, &mut &self.encoded[..]).ok();
		}
		self.decoded.as_ref().ok_or(())
	}

	/// Provides an API similar to `TryInto` that allows fallible conversion to the inner value
	/// type. `TryInto` implementation would collide with std blanket implementation based on
	/// `TryFrom`.
	pub fn try_into(mut self) -> Result<T, ()> {
		self.ensure_decoded()?;
		self.decoded.ok_or(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[derive(Debug, PartialEq, Encode, Decode)]
	struct WrappedU64(u64);

	impl TryGetDecodeFn for WrappedU64 {}

	#[test]
	fn ensure_decoded_works() {
		let val: WrappedU64 = WrappedU64(42);
		let mut encoded: DoubleEncoded<_> = Encode::encode(&val).into();
		assert_eq!(encoded.ensure_decoded(), Ok(&val));
	}

	#[test]
	fn try_into_works() {
		let val: WrappedU64 = WrappedU64(42);
		let encoded: DoubleEncoded<_> = Encode::encode(&val).into();
		assert_eq!(encoded.try_into(), Ok(val));
	}
}
