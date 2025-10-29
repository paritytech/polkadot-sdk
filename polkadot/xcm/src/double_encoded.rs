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
use alloc::vec::Vec;
use codec::{Decode, DecodeLimit, DecodeWithMemTracking, Encode, Input};

use sp_runtime::nested_mem;

const DECODE_ALL_ERR_MSG: &str = "Input buffer has still data left after decoding!";

/// Wrapper around the encoded and decoded versions of a value.
/// Caches the decoded value once computed.
#[derive(Encode, Decode, DecodeWithMemTracking, scale_info::TypeInfo)]
#[codec(encode_bound())]
#[codec(decode_bound())]
#[scale_info(bounds(), skip_type_params(T))]
#[scale_info(replace_segment("staging_xcm", "xcm"))]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DoubleEncoded<T> {
	encoded: Vec<u8>,
	#[codec(skip)]
	#[cfg_attr(feature = "json-schema", schemars(skip))]
	decoded: Option<(T, Option<nested_mem::DeallocationReminder>)>,
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
	fn try_decode(&self) -> Result<(T, Option<nested_mem::DeallocationReminder>), codec::Error> {
		nested_mem::decode_with_limiter(&mut &self.encoded[..], |mem_tracking_input| {
			let decoded = T::decode_with_depth_limit(MAX_XCM_DECODE_DEPTH, mem_tracking_input)?;
			if mem_tracking_input.remaining_len() != Ok(Some(0)) {
				return Err(DECODE_ALL_ERR_MSG.into());
			}
			Ok(decoded)
		})
	}

	/// Decode the inner encoded value and store it.
	/// Returns a reference to the value in case of success and `Err(())` in case the decoding
	/// fails.
	pub fn ensure_decoded(&mut self) -> Result<&T, codec::Error> {
		if self.decoded.is_none() {
			self.decoded = Some(self.try_decode()?);
		}
		Ok(self
			.decoded
			.as_ref()
			.map(|(decoded, _deallocation_reminder)| decoded)
			.expect("The value has just been decoded"))
	}

	/// Do something with the decoded value, consuming `self`.
	pub fn try_using_decoded<F, R>(mut self, f: F) -> Result<R, codec::Error>
	where
		F: FnOnce(T) -> R,
	{
		self.ensure_decoded()?;
		let (decoded, _deallocation_reminder) =
			self.decoded.expect("The value has just been decoded");
		Ok(f(decoded))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use sp_runtime::generic::DEFAULT_CALL_SIZE_LIMIT;

	const DECODE_OOM_MSG: &str = "Heap memory limit exceeded while decoding";

	#[test]
	fn ensure_decoded_works() {
		let val: u64 = 42;
		let mut encoded: DoubleEncoded<_> = Encode::encode(&val).into();
		assert_eq!(encoded.ensure_decoded(), Ok(&val));
	}

	#[test]
	fn try_using_decoded_works() {
		let val_1 = vec![1; DEFAULT_CALL_SIZE_LIMIT - 1000];
		let encoded_val_1: DoubleEncoded<Vec<u8>> = Encode::encode(&val_1).into();

		assert_eq!(nested_mem::get_current_limit(), None);
		nested_mem::using_limiter_once(|| {
			assert_eq!(nested_mem::get_current_limit(), Some(DEFAULT_CALL_SIZE_LIMIT));
			encoded_val_1
				.try_using_decoded(|decoded_val| {
					assert_eq!(nested_mem::get_current_limit(), Some(1000));
					assert_eq!(decoded_val, val_1);

					let val_2 = vec![2; 999];
					let encoded_val_2: DoubleEncoded<Vec<u8>> = Encode::encode(&val_2).into();
					let res = encoded_val_2.try_using_decoded(|decoded_val| {
						assert_eq!(decoded_val, val_2);
						assert_eq!(nested_mem::get_current_limit(), Some(1));
					});
					assert_eq!(res, Ok(()));
					assert_eq!(nested_mem::get_current_limit(), Some(1000));

					let val_2 = vec![2; 1000];
					let encoded_val_2: DoubleEncoded<Vec<u8>> = Encode::encode(&val_2).into();
					let res = encoded_val_2.try_using_decoded(|decoded_val| {
						assert_eq!(decoded_val, val_2);
					});
					assert_eq!(res, Err(DECODE_OOM_MSG.into()));
					assert_eq!(nested_mem::get_current_limit(), Some(1000));
				})
				.unwrap();
			assert_eq!(nested_mem::get_current_limit(), Some(DEFAULT_CALL_SIZE_LIMIT));
		});
		assert_eq!(nested_mem::get_current_limit(), None);
	}
}
