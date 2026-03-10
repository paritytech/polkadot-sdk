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

use crate::{utils, MAX_XCM_DECODE_DEPTH};
use alloc::{boxed::Box, vec::Vec};
use codec::{Decode, DecodeLimit, DecodeWithMemTracking, Encode};
use core::any::TypeId;
use frame_support::MAX_EXTRINSIC_DEPTH;
use sp_runtime::Saturating;

const DECODE_MAX_DEPTH_MSG: &str = "Maximum recursion depth reached when decoding";

environmental::environmental!(depth: u8);

pub trait XcmRuntimeCall: 'static + Decode {}

impl<T> XcmRuntimeCall for T where T: 'static + Decode {}

struct NestedInput<'a> {
	main_input: Box<&'a mut dyn codec::Input>,
	opaque: &'a [u8],
}

impl<'a> codec::Input for NestedInput<'a> {
	fn remaining_len(&mut self) -> Result<Option<usize>, codec::Error> {
		self.opaque.remaining_len()
	}

	fn read(&mut self, into: &mut [u8]) -> Result<(), codec::Error> {
		self.opaque.read(into)
	}

	fn read_byte(&mut self) -> Result<u8, codec::Error> {
		self.opaque.read_byte()
	}

	fn descend_ref(&mut self) -> Result<(), codec::Error> {
		depth::using_once(&mut 0, || {
			depth::with(|depth| {
				depth.saturating_inc();
				if *depth as u32 > MAX_EXTRINSIC_DEPTH {
					return Err(DECODE_MAX_DEPTH_MSG.into());
				}

				Ok(())
			})
			.unwrap_or(Err(codec::Error::from("Error calling `instructions_count::with()`")))
		})
	}

	fn ascend_ref(&mut self) {
		depth::using_once(&mut 0, || {
			let _ = depth::with(|depth| {
				depth.saturating_dec();
			});
		});
	}

	fn on_before_alloc_mem(&mut self, size: usize) -> Result<(), codec::Error> {
		self.main_input.on_before_alloc_mem(size)
	}
}

/// Wrapper around the encoded and decoded versions of a value.
/// Caches the decoded value once computed.
#[derive(Encode, DecodeWithMemTracking, scale_info::TypeInfo)]
#[codec(encode_bound())]
#[codec(decode_with_mem_tracking_bound(T: XcmRuntimeCall))]
#[scale_info(bounds(), skip_type_params(T))]
#[scale_info(replace_segment("staging_xcm", "xcm"))]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DoubleEncoded<T> {
	encoded: Vec<u8>,
	#[codec(skip)]
	decoded: Option<T>,
}

impl<T> Decode for DoubleEncoded<T>
where
	T: XcmRuntimeCall,
{
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let mut obj = Self { encoded: Vec::<u8>::decode(input)?, decoded: None };
		if TypeId::of::<T>() == TypeId::of::<()>() {
			return Ok(obj);
		}

		// We also decode the inner double encoded object if possible, in order to make sure that
		// its heap memory and depth are accounted for.
		let mut nested_input =
			NestedInput { main_input: Box::new(input), opaque: &obj.encoded[..] };
		obj.decoded = Some(T::decode(&mut nested_input)?);
		utils::ensure_all_decoded(nested_input.opaque)?;

		Ok(obj)
	}
}

impl<T> Clone for DoubleEncoded<T> {
	fn clone(&self) -> Self {
		Self { encoded: self.encoded.clone(), decoded: None }
	}
}

impl<T> PartialEq for DoubleEncoded<T> {
	fn eq(&self, other: &Self) -> bool {
		self.encoded.eq(&other.encoded)
	}
}
impl<T> Eq for DoubleEncoded<T> {}

impl<T> core::fmt::Debug for DoubleEncoded<T> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		array_bytes::bytes2hex("0x", &self.encoded).fmt(f)
	}
}

impl<T> From<Vec<u8>> for DoubleEncoded<T> {
	fn from(encoded: Vec<u8>) -> Self {
		Self { encoded, decoded: None }
	}
}

impl<T> DoubleEncoded<T> {
	pub fn encoded(&self) -> &[u8] {
		&self.encoded
	}

	/// Converts a `DoubleEncoded<T>` into a `DoubleEncoded<S>`, dropping the decoded value.
	pub fn transmute_encoded<S>(self) -> DoubleEncoded<S> {
		DoubleEncoded { encoded: self.encoded, decoded: None }
	}
}

impl<T: Decode> DoubleEncoded<T> {
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

	#[test]
	fn ensure_decoded_works() {
		let val: u64 = 42;
		let mut encoded: DoubleEncoded<_> = Encode::encode(&val).into();
		assert_eq!(encoded.ensure_decoded(), Ok(&val));
	}

	#[test]
	fn try_into_works() {
		let val: u64 = 42;
		let encoded: DoubleEncoded<_> = Encode::encode(&val).into();
		assert_eq!(encoded.try_into(), Ok(val));
	}
}
