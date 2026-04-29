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

use codec::{Decode, Encode};
use ethereum_types::U256;
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

use crate::runtime_api::{GenericTransactionV1, StateOverrideSetV1};

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime to estimate the gas required for an
	/// Ethereum transaction. The runtime performs an internal binary search to find the minimum gas
	/// value that still allows the transaction to succeed under the supplied (and optionally
	/// overridden) state.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct EthEstimateGasVersionedInputPayloadV1<Moment> {
		/// Transaction to estimate gas for.
		pub tx: GenericTransactionV1,
		/// Timestamp to use during the simulation. `None` falls back to the current chain time.
		pub timestamp_override: Option<Moment>,
		/// Whether the simulation should enforce the sender's balance against the value and fees of
		/// the transaction. `None` defers to the runtime's default policy.
		pub perform_balance_checks: Option<bool>,
		/// State overrides applied to the runtime state before estimating; `None` runs the
		/// estimation against the unmodified state.
		pub state_overrides: Option<StateOverrideSetV1>
	}

	/// Version 1 of the output payload returned for an [`EthEstimateGasVersionedInputPayloadV1`]
	/// request, carrying the estimated gas value.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct EthEstimateGasVersionedOutputPayloadV1 {
		/// Minimum gas amount, in Ethereum gas units, that the transaction needs in order to
		/// succeed under the requested simulation parameters.
		pub gas: U256
	}
}
