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
//! Helpers for XCM barrier benchmarks on bridge hubs.

#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};
use xcm::latest::prelude::*;

/// Worst-case `(origin, message)` for bridge hub barriers shaped like
/// `DenyThenTry<(DenyRecursively<DenyReserveTransferToRelayChain>,
/// DenyRecursively<DenyExportMessageFrom<..>>), (.., WithComputedOrigin<.., MaxPrefixes>)>`.
///
/// Same shape as
/// `parachains_common::xcm_benchmarks::deny_reserve_transfer_recursive_barrier_check`,
/// but the message deliberately contains neither a reserve transfer to the relay nor an
/// `ExportMessage`. That way both deny filters scan (and recurse through) the whole message without
/// short-circuiting on a denied instruction, and the more expensive `WithComputedOrigin` +
/// allow-tuple path still runs all the way to rejection.
///
/// `origin` must not be the asset hub (so `DenyExportMessageFrom` scans the message rather than
/// skipping it) and must have an empty interior (so all `max_computed_origin_prefixes` prefixes are
/// processed by `WithComputedOrigin`) — callers pass the relay/parent location.
pub fn bridge_hub_barrier_check<Call>(
	origin: Location,
	max_computed_origin_prefixes: u32,
) -> (Location, Xcm<Call>) {
	let mut instructions: Vec<Instruction<Call>> = (0..max_computed_origin_prefixes)
		.map(|_| DescendOrigin(OnlyChild.into()))
		.collect();
	instructions.push(SetAppendix(Xcm(vec![SetErrorHandler(Xcm(vec![ExecuteWithOrigin {
		descendant_origin: None,
		xcm: Xcm(vec![ClearOrigin]),
	}]))])));
	(origin, Xcm(instructions))
}
