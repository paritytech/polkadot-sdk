// This file is part of Cumulus.

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

// Escalates the para to five cores and opens HRMP channels to two sibling paras. A tiny JAM
// network has two cores and no HRMP, so there is nothing to run it against.
#[cfg(not(feature = "jam"))]
mod basic;
// The four below run against `cumulus-test-runtime`; the JAM path runs the parachain template,
// so their subject does not exist there. Each reason is the error the JAM run actually produced.

// Drives weight/PoV scenarios through `Utility`, which the template runtime does not have:
// "Variant Utility does not exist on type with identifier 8".
#[cfg(not(feature = "jam"))]
mod full_core_usage_scenarios;
// Its PoV-recovery full node and log assertion have no JAM counterpart.
#[cfg(not(feature = "jam"))]
mod pov_recovery;
// `assert_relay_parent_offset` reads relay parent digests, which a JAM para header does not carry.
#[cfg(not(feature = "jam"))]
mod relay_parent_offset;
// Upgrades to a WASM blob; the JAM runtime is PolkaVM, so there is nothing to upgrade to:
// "WASM runtime binary not available".
#[cfg(not(feature = "jam"))]
mod runtime_upgrade;
mod three_cores_glutton;
// subxt cannot decode the JAM `JamParent` digest item the runtime deposits:
// "Could not decode `DigestItemType`, variant doesn't exist".
#[cfg(not(feature = "jam"))]
mod tracing_block;
// Warp sync is a relay-chain concept, and the para does not finalize on JAM inside the window:
// "Timeout (200), waiting for metric block_height{status=\"finalized\"}".
#[cfg(not(feature = "jam"))]
mod warp_sync;
