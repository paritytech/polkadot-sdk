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
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;
use sp_runtime::DispatchError;
use sp_weights::Weight;

use crate::{CodeV1, InstantiateReturnValueV1, StorageDepositV1};

define_versioned_interface! {
	/// Version 1 of the input payload that dry-runs a contract instantiation. The runtime simulates
	/// the instantiation against the current state and reports back the resources it would have
	/// consumed plus the result of the constructor, including the deterministic address of the
	/// would-be contract.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct InstantiateVersionedInputPayloadV1<AccountId, Balance> {
		/// `AccountId` that should be treated as the caller initiating the instantiation.
		pub origin: AccountId,
		/// Native balance to transfer into the new contract as part of the instantiation.
		pub value: Balance,
		/// Optional weight ceiling for the instantiation. `None` lets the runtime pick the max that
		/// the caller could afford.
		pub gas_limit: Option<Weight>,
		/// Optional cap on the storage deposit the instantiation may consume. `None` lets the
		/// runtime charge whatever the instantiation legitimately needs.
		pub storage_deposit_limit: Option<Balance>,
		/// Contract code to instantiate, supplied either as raw bytes to upload or as a hash
		/// referencing previously-uploaded code.
		pub code: CodeV1,
		/// Constructor input passed to the contract.
		pub data: Vec<u8>,
		/// Optional salt used for deterministic address derivation. `None` uses the standard
		/// nonce-based address-derivation scheme.
		pub salt: Option<[u8; 32]>
	}

	/// Version 1 of the output payload returned for an [`InstantiateVersionedInputPayloadV1`]
	/// request, carrying the simulated resource consumption and the constructor's result.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct InstantiateVersionedOutputPayloadV1<Balance> {
		/// Weight actually consumed by the instantiation, after refunds.
		pub weight_consumed: Weight,
		/// Weight that the instantiation required to dispatch; this is the value the dispatcher
		/// reserves and may exceed `weight_consumed` because of refunds.
		pub weight_required: Weight,
		/// Storage deposit charge or refund produced by the instantiation.
		pub storage_deposit: StorageDepositV1<Balance>,
		/// Largest storage deposit the instantiation would have charged in the worst case.
		pub max_storage_deposit: StorageDepositV1<Balance>,
		/// Gas consumed by the instantiation, expressed in the runtime's `Balance` type so it can
		/// be folded directly into fee maths.
		pub gas_consumed: Balance,
		/// Either the instantiation's return value (containing the new contract's address and the
		/// constructor output) or the `DispatchError` that aborted the instantiation.
		pub result: Result<InstantiateReturnValueV1, DispatchError>
	}
}
