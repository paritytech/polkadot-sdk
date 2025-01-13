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

use codec::{
	self, decode_vec_with_len, Codec, Compact, Decode, Encode, Error as CodecError,
	Input as CodecInput,
};
use educe::Educe;
use scale_info::TypeInfo;

#[derive(Educe, Default, Encode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
#[scale_info(bounds(Instruction: TypeInfo))]
pub struct XcmBase<Instruction>(pub Vec<Instruction>);

pub const MAX_INSTRUCTIONS_TO_DECODE: u8 = 100;

environmental::environmental!(instructions_count: u8);

impl<Instruction: Codec + TypeInfo> Decode for XcmBase<Instruction> {
	fn decode<I: CodecInput>(input: &mut I) -> core::result::Result<Self, CodecError> {
		instructions_count::using_once(&mut 0, || {
			let number_of_instructions: u32 = <Compact<u32>>::decode(input)?.into();
			instructions_count::with(|count| {
				*count = count.saturating_add(number_of_instructions as u8);
				if *count > MAX_INSTRUCTIONS_TO_DECODE {
					return Err(CodecError::from("Max instructions exceeded"));
				}
				Ok(())
			})
			.expect("Called in `using` context and thus can not return `None`; qed")?;
			let decoded_instructions = decode_vec_with_len(input, number_of_instructions as usize)?;
			Ok(Self(decoded_instructions))
		})
	}
}

impl<Instruction: Codec + TypeInfo> XcmBase<Instruction> {
	/// Create an instance with the given instructions.
	pub fn new(instructions: Vec<Instruction>) -> Self {
		Self(instructions)
	}

	/// Return `true` if no instructions are held in `self`.
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	/// Return the number of instructions held in `self`.
	pub fn len(&self) -> usize {
		self.0.len()
	}

	/// Return a reference to the inner value.
	pub fn inner(&self) -> &[Instruction] {
		&self.0
	}

	/// Return a mutable reference to the inner value.
	pub fn inner_mut(&mut self) -> &mut Vec<Instruction> {
		&mut self.0
	}

	/// Consume and return the inner value.
	pub fn into_inner(self) -> Vec<Instruction> {
		self.0
	}

	/// Return an iterator over references to the items.
	pub fn iter(&self) -> impl Iterator<Item = &Instruction> {
		self.0.iter()
	}

	/// Return an iterator over mutable references to the items.
	pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Instruction> {
		self.0.iter_mut()
	}

	/// Consume and return an iterator over the items.
	pub fn into_iter(self) -> impl Iterator<Item = Instruction> {
		self.0.into_iter()
	}

	/// Consume and either return `self` if it contains some instructions, or if it's empty, then
	/// instead return the result of `f`.
	pub fn or_else(self, f: impl FnOnce() -> Self) -> Self {
		if self.0.is_empty() {
			f()
		} else {
			self
		}
	}

	/// Return the first instruction, if any.
	pub fn first(&self) -> Option<&Instruction> {
		self.0.first()
	}

	/// Return the last instruction, if any.
	pub fn last(&self) -> Option<&Instruction> {
		self.0.last()
	}

	/// Return the only instruction, contained in `Self`, iff only one exists (`None` otherwise).
	pub fn only(&self) -> Option<&Instruction> {
		if self.0.len() == 1 {
			self.0.first()
		} else {
			None
		}
	}

	/// Return the only instruction, contained in `Self`, iff only one exists (returns `self`
	/// otherwise).
	pub fn into_only(mut self) -> core::result::Result<Instruction, Self> {
		if self.0.len() == 1 {
			self.0.pop().ok_or(self)
		} else {
			Err(self)
		}
	}
}

impl<Instruction: Codec + TypeInfo> From<Vec<Instruction>> for XcmBase<Instruction> {
	fn from(c: Vec<Instruction>) -> Self {
		Self(c)
	}
}

impl<Instruction: Codec + TypeInfo> From<XcmBase<Instruction>> for Vec<Instruction> {
	fn from(c: XcmBase<Instruction>) -> Self {
		c.0
	}
}

