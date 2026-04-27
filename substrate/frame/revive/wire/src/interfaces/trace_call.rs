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
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

use crate::{GenericTransactionV1, StateOverrideSetV1, TraceV1, TracerTypeV1};

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime to trace a single Ethereum call without
	/// committing it. The runtime simulates the transaction against the current state (with
	/// optional overrides) and returns the trace shape selected by the tracer config.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct TraceCallVersionedInputPayloadV1 {
		/// Transaction to trace.
		pub tx: GenericTransactionV1,
		/// Tracer configuration controlling which kind of trace is produced.
		pub tracer_type: TracerTypeV1,
		/// State overrides applied before tracing. `None` traces against the unmodified state.
		pub state_overrides: Option<StateOverrideSetV1>
	}

	/// Version 1 of the output payload returned for a [`TraceCallVersionedInputPayloadV1`] request,
	/// carrying the trace produced by replaying the transaction.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct TraceCallVersionedOutputPayloadV1 {
		/// Trace produced by executing the transaction under the requested tracer.
		pub trace: TraceV1
	}
}
