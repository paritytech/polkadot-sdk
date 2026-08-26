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

//! Helpers for XCM barrier benchmarks.

#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};
use xcm::{latest::prelude::*, RECURSION_LIMIT};

/// Worst-case `(origin, message)` for a standard parachain barrier's heaviest `ref_time` path (the
/// matching `proof_size` path is [`worst_case_barrier_check_proof_size`]).
///
/// The barrier is `DenyThenTry<DenyRecursively<..>, (.., WithComputedOrigin<.., MaxPrefixes>)>`.
/// One rejected message maximises both `ref_time` drivers: `max_origin_prefixes` leading
/// `DescendOrigin`s drive `WithComputedOrigin` (an empty `Location::here()` interior keeps every
/// append within the junction limit), and the message is padded to `max_instructions` so
/// `DenyRecursively` performs its full scan. The nested chain carries no reserve transfer to the
/// relay, so the scan is never short-circuited by a denied instruction.
///
/// `max_origin_prefixes` and `max_instructions` must match the runtime's `WithComputedOrigin` bound
/// and `MaxInstructions`.
pub fn worst_case_barrier_check_ref_time<Call>(
	max_origin_prefixes: u32,
	max_instructions: u32,
) -> (Location, Xcm<Call>) {
	let mut instructions: Vec<Instruction<Call>> = Vec::new();

	// Drive `WithComputedOrigin` through its full prefix budget.
	for _ in 0..max_origin_prefixes {
		instructions.push(DescendOrigin([PalletInstance(0)].into()));
	}

	// Benign nested chain kept below `RECURSION_LIMIT`, so `DenyRecursively` scans it fully instead
	// of bailing out early with `StackLimitReached`.
	let nesting_depth = (RECURSION_LIMIT as u32).saturating_sub(2).max(1);
	let mut nested = Xcm::<Call>(vec![ClearOrigin]);
	for level in 1..nesting_depth {
		nested = match level % 3 {
			0 => Xcm(vec![SetAppendix(nested)]),
			1 => Xcm(vec![SetErrorHandler(nested)]),
			_ => Xcm(vec![ExecuteWithOrigin { descendant_origin: None, xcm: nested }]),
		};
	}
	instructions.append(&mut nested.0);

	// Pad to `max_instructions` total (nested included) to maximise the scan. `ClearOrigin` matches
	// no inner allow-barrier, so the message is ultimately rejected.
	let used = max_origin_prefixes.saturating_add(nesting_depth);
	for _ in used..max_instructions {
		instructions.push(ClearOrigin);
	}

	(Location::here(), Xcm(instructions))
}

/// Worst-case `(origin, message)` for the standard parachain XCM barrier's heaviest `proof_size`
/// path.
///
/// This is the **storage-read** path: `AllowKnownQueryResponses` calls `expecting_response`, which
/// reads the `Queries` map (high `proof_size`, low `ref_time`). The matching `ref_time` path is
/// [`worst_case_barrier_check_ref_time`].
///
/// `query_id` must refer to a query previously registered (e.g. via `QueryHandler::new_query`)
/// with a responder that does **not** match `origin`, so the barrier performs the `Queries` read
/// and then rejects the message.
pub fn worst_case_barrier_check_proof_size<Call>(
	origin: Location,
	query_id: QueryId,
) -> (Location, Xcm<Call>) {
	// A single `QueryResponse` that forces the `Queries` read in `expecting_response` before the
	// message is rejected.
	let query_response = Xcm(vec![QueryResponse {
		query_id,
		response: Response::Null,
		max_weight: Weight::zero(),
		querier: None,
	}]);

	(origin, query_response)
}
