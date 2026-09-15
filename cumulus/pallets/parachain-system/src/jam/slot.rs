// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! The JAM slot as the parachain's observed "relay block number".
//!
//! On JAM there is no relay chain: the `relay_parent_number` in the inherent is fabricated by the
//! collator, so it is not trustworthy as the relay block number. This module provides
//! [`JamSlotNumber`], a [`BlockNumberProvider`] whose current block number is the refine
//! context's `lookup_anchor_slot` — read through the shared
//! [`parachain_service_core::refine::refine_context`] fetch wrapper (backed by the child-PVM
//! `Fetch::RefineContext` host call) whenever a block number is read — the fetch is synchronous
//! and serves the work package currently being executed, so there is nothing to thread through
//! block execution.
//!
//! The module only exists on JAM builds: relay/wasm runtimes have no host to fetch from.

use sp_runtime::traits::BlockNumberProvider;

/// A [`BlockNumberProvider`] returning the JAM slot (`RefineContext.lookup_anchor_slot`) as the
/// "relay block number" during JAM block execution.
///
/// Outside JAM block execution there is no refine context to fetch and the wrapper aborts — a
/// block number read then is a protocol error, exactly as in the service's own refine entry
/// point.
pub struct JamSlotNumber;

impl BlockNumberProvider for JamSlotNumber {
	type BlockNumber = u32;

	fn current_block_number() -> u32 {
		parachain_service_core::refine::refine_context().lookup_anchor_slot
	}
}
