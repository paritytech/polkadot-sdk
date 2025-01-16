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
// See the License fsor the specific language governing permissions and
// limitations under the License.

//! Traits for querying pallet view functions.

use codec::{DecodeAll, Encode, Output};
use sp_core::{ViewFunctionDispatchError, ViewFunctionId};

/// implemented by the runtime dispatching by prefix and then the pallet dispatching by suffix
pub trait DispatchViewFunction {
	fn dispatch_view_function<O: Output>(
		id: &ViewFunctionId,
		input: &mut &[u8],
		output: &mut O,
	) -> Result<(), ViewFunctionDispatchError>;

	/// Convenience function to dispatch a view function and return the result as a vector.
	fn execute_view_function(
		id: &ViewFunctionId,
		mut input: &[u8],
	) -> Result<alloc::vec::Vec<u8>, ViewFunctionDispatchError> {
		let mut output = Default::default();
		Self::dispatch_view_function(id, &mut input, &mut output)?;
		Ok(output)
	}
}

impl DispatchViewFunction for () {
	fn dispatch_view_function<O: Output>(
		_id: &ViewFunctionId,
		_input: &mut &[u8],
		_output: &mut O,
	) -> Result<(), ViewFunctionDispatchError> {
		Err(ViewFunctionDispatchError::NotImplemented)
	}
}

pub trait ViewFunctionIdPrefix {
	fn prefix() -> [u8; 16];
}

pub trait ViewFunctionIdSuffix {
	const SUFFIX: [u8; 16];
}

/// implemented for each pallet view function method
#[deprecated] // no longer used?
pub trait ViewFunction: DecodeAll {
	fn id() -> ViewFunctionId;
	type ReturnType: Encode;

	fn invoke(self) -> Self::ReturnType;

	fn execute<O: Output>(
		input: &mut &[u8],
		output: &mut O,
	) -> Result<(), ViewFunctionDispatchError> {
		let view_function = Self::decode_all(input)?;
		let result = view_function.invoke();
		Encode::encode_to(&result, output);
		Ok(())
	}
}
