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

//! XCM utils for internal use.

use crate::MAX_INSTRUCTIONS_TO_DECODE;

use alloc::vec::Vec;
use codec::{decode_vec_with_len, Compact, Decode};
use sp_runtime::SaturatedConversion;

environmental::environmental!(instructions_count: u8);

pub fn decode_xcm_instructions<I: codec::Input, T: Decode>(
	input: &mut I,
) -> Result<Vec<T>, codec::Error> {
	instructions_count::using_once(&mut 0, || {
		let number_of_instructions: u32 = <Compact<u32>>::decode(input)?.into();
		instructions_count::with(|count| {
			*count = count.saturating_add(number_of_instructions.saturated_into());
			if *count > MAX_INSTRUCTIONS_TO_DECODE {
				return Err(codec::Error::from("Max instructions exceeded"))
			}
			Ok(())
		})
		.expect("Called in `using` context and thus can not return `None`; qed")?;
		let decoded_instructions = decode_vec_with_len(input, number_of_instructions as usize)?;
		Ok(decoded_instructions)
	})
}
