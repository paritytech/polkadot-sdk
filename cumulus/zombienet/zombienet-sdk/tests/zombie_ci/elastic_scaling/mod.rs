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

// A tiny JAM network has exactly two cores, and its runtime is the parachain template, so the
// tests below have no JAM counterpart: `asset_hub_westend` runs the Asset Hub runtime over three
// cores, and `upgrade_to_3_cores` is about the third core itself.
#[cfg(not(feature = "jam"))]
mod asset_hub_westend;
// Its PoV-recovery full node and log assertion have no JAM counterpart.
#[cfg(not(feature = "jam"))]
mod pov_recovery;
mod slot_based_authoring;
// `assert_relay_parent_offset` reads relay parent digests, which a JAM para header does not carry.
#[cfg(not(feature = "jam"))]
mod slot_based_rp_offset;
#[cfg(not(feature = "jam"))]
mod upgrade_to_3_cores;


