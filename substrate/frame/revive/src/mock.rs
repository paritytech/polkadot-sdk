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

//! Helper interfaces and functions, that help with controlling the execution of EVM contracts.
//! It is mostly used to help with the implementation of foundry cheatscodes for forge test
//! integration.

use frame_system::pallet_prelude::OriginFor;
use sp_core::{H160, U256};

use crate::{pallet, DelegateInfo, ExecReturnValue};

/// A trait that provides hooks for mocking EVM contract calls and callers.
/// This is useful for testing and simulating contract interactions within foundry forge tests.
pub trait MockHandler<T: pallet::Config> {
	/// Mock an EVM contract call.
	///
	/// Returns `Some(ExecReturnValue)` if the call is mocked, otherwise `None`.
	fn mock_call(
		&self,
		_callee: H160,
		_call_data: &[u8],
		_value_transferred: U256,
	) -> Option<ExecReturnValue> {
		None
	}

	/// Mock the caller of a contract.
	///
	/// Returns `Some(OriginFor<T>)` if the caller is mocked, otherwise `None`.
	fn mock_caller(&self, _frames_len: usize) -> Option<OriginFor<T>> {
		None
	}

	/// Mock a delegated caller for a contract call.
	///
	/// Returns `Some(DelegateInfo<T>)` if the delegated caller is mocked, otherwise `None`.
	fn mock_delegated_caller(&self, _dest: H160, _input_data: &[u8]) -> Option<DelegateInfo<T>> {
		None
	}
}
