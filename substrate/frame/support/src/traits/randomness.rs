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

//! Traits for dealing with on-chain randomness.

/// A trait that is able to provide randomness.
///
/// Being a deterministic blockchain, real randomness is difficult to come by, different
/// implementations of this trait will provide different security guarantees. At best,
/// this will be randomness which was hard to predict a long time ago, but that has become
/// easy to predict recently.
pub trait Randomness<Output, BlockNumber> {
	/// Get the most recently determined random seed, along with the time in the past
	/// since when it was determinable by chain observers.
	///
	/// `subject` is a context identifier and allows you to get a different result to
	/// other callers of this function; use it like `random(&b"my context"[..])`.
	///
	/// NOTE: The returned seed should only be used to distinguish commitments made before
	/// the returned block number. If the block number is too early (i.e. commitments were
	/// made afterwards), then ensure no further commitments may be made and repeatedly
	/// call this on later blocks until the block number returned is later than the latest
	/// commitment.
	fn random(subject: &[u8]) -> (Output, BlockNumber);

	/// Get the basic random seed.
	///
	/// In general you won't want to use this, but rather `Self::random` which allows
	/// you to give a subject for the random result and whose value will be
	/// independently low-influence random from any other such seeds.
	///
	/// NOTE: The returned seed should only be used to distinguish commitments made before
	/// the returned block number. If the block number is too early (i.e. commitments were
	/// made afterwards), then ensure no further commitments may be made and repeatedly
	/// call this on later blocks until the block number returned is later than the latest
	/// commitment.
	fn random_seed() -> (Output, BlockNumber) {
		Self::random(&[][..])
	}
}

#[cfg(any(test, feature = "std"))]
pub mod test_randomness {
	use super::*;
	use sp_core::crypto::FromEntropy;
	use sp_arithmetic::traits::Zero;
	use codec::{Error, Input};

	/// An object that is able to produce an arbitrary length slice using some initial set of bytes
	/// that will repeat over and over.
	pub struct RepeatingSlice<'a>(&'a [u8], usize);
	impl<'a> RepeatingSlice<'a> {
		/// Create a new `RepeatingSlice` given some initial set of input bytes.
		///
		/// Empty slices are valid inputs as they are used in `Randomness::random_seed`.
		pub fn new(s: &'a [u8]) -> Self {
			Self(s, 0)
		}
	}

	impl<'a> Input for RepeatingSlice<'a> {
		fn remaining_len(&mut self) -> Result<Option<usize>, Error> {
			Ok(Some(usize::max_value()))
		}

		fn read(&mut self, mut into: &mut [u8]) -> Result<(), Error> {
			let data = &self.0;
			if data.is_empty() {
				// Empty slices are valid inputs as they are used in `Randomness::random_seed`. The
				// solution here is to just return from `Input::read` leaving the destination
				// untouched, but another option is to fill it with zeroes or some other value (e.g.
				// `0..256`) in the `RepeatingSlice` constructor, though I expect that it would be
				// more useful in testing to be able to control the `random_seed` output knowing
				// it's a noop.
				return Ok(())
			}
			let off = &mut self.1;
			while into.len() != 0 {
				let len = into.len().min(data.len() - *off);
				(&mut into[..len]).copy_from_slice(&data[..len]);
				*off = (*off + len) % data.len();
				into = &mut into[len..];
			}
			Ok(())
		}
	}

	/// This is not a real source of Randomness. Only use this for testing.
	impl<O: FromEntropy, B: Zero> Randomness<O, B> for () {
		fn random(subject: &[u8]) -> (O, B) {
			const REASON: &'static str = "from_entropy is given an infinite input; qed";
			(O::from_entropy(&mut RepeatingSlice::new(subject)).expect(REASON), B::zero())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::test_randomness::*;
	use codec::Input;

	#[test]
	fn repeating_slice_0_sized_input() {
		let mut repeating_slice = RepeatingSlice::new(&[][..]);
		let mut dst: Vec<u8> = (1..=10).into_iter().collect();
		let expected_output = dst.clone();
		repeating_slice.read(&mut dst[..]).unwrap();
		assert_eq!(dst, expected_output);
	}

	#[test]
	fn repeating_slice_0_sized_destination() {
		let mut dst: Vec<u8> = Vec::new();
		for i in 0..10u8 {
			let input: Vec<u8> = (1..=i).into_iter().collect();
			let mut repeating_slice = RepeatingSlice::new(&input);
			repeating_slice.read(&mut dst[..]).unwrap();
			assert!(dst.is_empty());
		}
	}

	#[test]
	fn repeating_slice_works() {
		for in_max in 1..10u8 {
			let input: Vec<u8> = (1..=in_max).into_iter().collect();
			for dst_len in 1..100u8 {
				let mut repeating_slice = RepeatingSlice::new(&input);
				let mut dst: Vec<u8> = (0..dst_len).into_iter().collect();
				repeating_slice.read(&mut dst[..]).unwrap();
				assert!(dst
					.iter()
					.enumerate()
					.all(|(pos, &x)| x > 0 && (x - 1) as usize == pos % in_max as usize));
			}
		}
	}
}
