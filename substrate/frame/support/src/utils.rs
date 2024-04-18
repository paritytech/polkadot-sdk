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

use codec::{Encode, Decode, Input, MaxEncodedLen};
use frame_support_procedural::{EqNoBound, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound};
use scale_info::TypeInfo;
use sp_core::Get;

use crate::Parameter;

/// TODO: docs
pub struct InputWithMaxLength<'a, I: Input, S: Get<usize>> {
	input: &'a mut I,
	len: usize,
	_phantom: core::marker::PhantomData<S>,
}

impl<'a, I: Input, S: Get<usize>> InputWithMaxLength<'a, I, S> {
	pub fn new(input: &'a mut I) -> Self {
		Self { input, len: S::get(), _phantom: Default::default() }
	}
}

impl<'a, I: Input, S: Get<usize>> Input for InputWithMaxLength<'a, I, S> {
	fn remaining_len(&mut self) -> Result<Option<usize>, codec::Error> {
		let len = self.input.remaining_len()?;
		Ok(len.map(|l| l.min(self.len)))
	}

	fn read(&mut self, into: &mut [u8]) -> Result<(), codec::Error> {
		let max_len = self.len.min(into.len());
		self.len -= max_len;
		self.input.read(&mut into[..max_len])?;
		Ok(())
	}
}

/// TODO: docs
#[derive(Encode, EqNoBound, PartialEqNoBound, CloneNoBound, TypeInfo, RuntimeDebugNoBound)]
#[scale_info(skip_type_params(S))]
pub struct WithMaxSize<T: Parameter, S: Get<usize>> {
	value: T,
	_phantom: core::marker::PhantomData<S>,
}

impl<T: Parameter, S: Get<usize>> WithMaxSize<T, S> {
	/// TODO: docs
	pub fn unchecked_new(value: T) -> Self {
		Self { value, _phantom: Default::default() }
	}

	/// TODO: docs
	pub fn new(value: T) -> Result<Self, ()> {
		if value.encoded_size() <= S::get() {
			Ok(Self { value, _phantom: Default::default() })
		} else {
			Err(())
		}
	}

	pub fn value(self) -> T {
		self.value
	}
}

impl<T: Parameter, S: Get<usize>> MaxEncodedLen for WithMaxSize<T, S> {
	fn max_encoded_len() -> usize {
		S::get()
	}
}

impl<T: Parameter, S: Get<usize>> Decode for WithMaxSize<T, S> {
	fn decode<I: Input>(input: &mut I) -> Result<Self, codec::Error> {
		let mut input = InputWithMaxLength::<I, S>::new(input);
        Ok(Self::unchecked_new(T::decode(&mut input)?))
	}
}
