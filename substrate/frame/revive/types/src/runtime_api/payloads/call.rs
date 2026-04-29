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

use alloc::vec::Vec;
use codec::{Decode, Encode};
use ethereum_types::H160;
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;
use sp_weights::Weight;

use crate::runtime_api::{DispatchErrorV1, ExecReturnValueV1, StorageDepositV1};

define_versioned_interface! {
	/// Version 1 of the input payload that dry-runs a contract call. The runtime executes the call
	/// against the current state, reports back the resources it would have consumed, and returns
	/// either the contract's exec output or the dispatch error that aborted execution.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct CallVersionedInputPayloadV1<AccountId, Balance> {
		/// `AccountId` that should be treated as the caller of the contract.
		pub origin: AccountId,
		/// Ethereum address of the contract being called.
		pub dest: H160,
		/// Native balance to transfer alongside the call.
		pub value: Balance,
		/// Optional weight ceiling for the call. `None` lets the runtime pick the max it allows.
		pub gas_limit: Option<Weight>,
		/// Optional cap on the storage deposit the call may consume. `None` lets the runtime charge
		/// whatever the call legitimately needs up to the caller's balance.
		pub storage_deposit_limit: Option<Balance>,
		/// Call data passed to the contract entrypoint.
		pub input_data: Vec<u8>
	}

	/// Version 1 of the output payload returned for a [`CallVersionedInputPayloadV1`] request,
	/// carrying both the resource consumption of the dry run and the contract's exec result.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct CallVersionedOutputPayloadV1<Balance> {
		/// Weight actually consumed by the call, after the runtime applied refunds.
		pub weight_consumed: Weight,
		/// Weight that the call required to dispatch; this is the value the dispatcher reserves in
		/// the block's weight budget and may exceed `weight_consumed` because of refunds.
		pub weight_required: Weight,
		/// Storage deposit charge or refund produced by the call.
		pub storage_deposit: StorageDepositV1<Balance>,
		/// Largest storage deposit the call would have charged in the worst case, used by callers
		/// that need to size the deposit budget conservatively.
		pub max_storage_deposit: StorageDepositV1<Balance>,
		/// Gas consumed by the call, expressed in the runtime's `Balance` type so it can be folded
		/// directly into fee maths.
		pub gas_consumed: Balance,
		/// Either the exec return value produced by the contract, or the dispatch error that
		/// aborted execution before a return value could be produced.
		pub result: Result<ExecReturnValueV1, DispatchErrorV1>
	}
}
