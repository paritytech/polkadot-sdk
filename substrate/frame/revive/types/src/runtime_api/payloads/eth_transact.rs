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
use ethereum_types::U256;
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;
use sp_weights::Weight;

use crate::runtime_api::{GenericTransactionV1, StateOverrideSetV1};

define_versioned_interface! {
	/// Version 1 of the input payload that dry-runs an Ethereum transaction. The runtime simulates
	/// the transaction against the current state (with optional state overrides) and reports back
	/// the resources it would have consumed plus the encoded inner-call output, so that the ETH-RPC
	/// layer can charge the right fee and return data the same way an Ethereum execution client
	/// would.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	pub struct EthTransactVersionedInputPayloadV1<Moment> {
		/// Ethereum transaction to dry-run.
		pub tx: GenericTransactionV1,
		/// Timestamp to use during the simulation. `None` uses the current chain time.
		pub timestamp_override: Option<Moment>,
		/// State overrides applied to the runtime state before simulating the transaction.
		pub state_overrides: StateOverrideSetV1
	}

	/// Version 1 of the output payload returned for an [`EthTransactVersionedInputPayloadV1`]
	/// request, carrying the simulated resource consumption and the encoded inner-call output.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	pub struct EthTransactVersionedOutputPayloadV1<Balance> {
		/// Weight the transaction would require to dispatch.
		pub weight_required: Weight,
		/// Storage deposit charged by the transaction.
		pub storage_deposit: Balance,
		/// Largest storage deposit the transaction would have charged in the worst case.
		pub max_storage_deposit: Balance,
		/// Gas, in Ethereum gas units, consumed by the transaction.
		pub eth_gas: U256,
		/// Encoded inner-call output emitted by the transaction.
		pub data: Vec<u8>
	}
}
