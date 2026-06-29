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
use xcm::latest::prelude::*;
use xcm_executor::RECURSION_LIMIT;

/// Worst-case `(origin, message)` for the standard parachain XCM barrier's heaviest `ref_time`
/// path.
///
/// The barrier has two paths that are each worst in a *different* weight dimension. This is the
/// **compute / scan** path; the matching `proof_size` path is
/// [`worst_case_barrier_check_proof_size`].
///
/// The standard parachain barrier is
/// `DenyThenTry<DenyRecursively<..>, (.., WithComputedOrigin<.., MaxPrefixes>)>`, which has two
/// `ref_time` cost drivers that this single rejected message maximises together:
///
/// * **`WithComputedOrigin`** appends one junction per leading `DescendOrigin`, up to
///   `max_origin_prefixes`. Starting from an empty interior (`Location::here()`) keeps every append
///   within the junction limit so all prefixes are processed.
/// * **`DenyRecursively`** scans *every* instruction in the message (recursing through nesting
///   instructions up to [`RECURSION_LIMIT`]) before the message is finally rejected, so its cost
///   scales with the total instruction count. The runtime caps that count at `max_instructions`
///   (the weigher rejects longer messages *before* the barrier runs), so the message is filled to
///   exactly that many instructions — including one deeply nested benign chain that contains no
///   reserve transfer to the relay, so the scan never short-circuits on a denied instruction.
///
/// `max_origin_prefixes` must match the runtime's `WithComputedOrigin` bound (e.g. its
/// `ConstU32<..>`) and `max_instructions` its `MaxInstructions`.
pub fn worst_case_barrier_check_ref_time<Call>(
	max_origin_prefixes: u32,
	max_instructions: u32,
) -> (Location, Xcm<Call>) {
	let mut instructions: Vec<Instruction<Call>> = Vec::new();

	// Drive `WithComputedOrigin` through its full prefix budget.
	for _ in 0..max_origin_prefixes {
		instructions.push(DescendOrigin([PalletInstance(0)].into()));
	}

	// One benign nested chain, kept safely below `RECURSION_LIMIT` so `DenyRecursively` performs
	// its full recursive scan instead of bailing out early with `StackLimitReached`.
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

	// Pad with benign top-level instructions until the total instruction count (nested included)
	// reaches `max_instructions`, maximising `DenyRecursively`'s scan. `ClearOrigin` matches none
	// of the inner allow-barriers, so the message is ultimately rejected.
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
