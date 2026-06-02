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

/// Worst-case `(origin, message)` for barriers shaped like
/// `DenyThenTry<DenyRecursively<DenyReserveTransferToRelayChain>, (.., WithComputedOrigin<..,
/// MaxPrefixes>)>`.
///
/// The message is built to maximise the work the barrier performs before it rejects the message;
/// rejection is the only path on which the benchmarked weight is charged.
///
/// - It leads with `max_computed_origin_prefixes` `DescendOrigin` instructions, so
///   `WithComputedOrigin` processes its entire prefix budget. Each `DescendOrigin` appends to the
///   computed origin, which is the dominant cost of the whole check. For all of the prefixes to be
///   processed (instead of overflowing the computed origin's junctions and bailing out early),
///   `origin` must have an empty interior — callers pass the relay/parent location.
/// - The computed origin and the trailing instruction match none of the `Allow*` cases, so every
///   allow-barrier in the tuple is evaluated before the message is finally rejected.
/// - It nests a benign `SetAppendix`/`SetErrorHandler`/`ExecuteWithOrigin` chain (containing no
///   reserve transfer to the relay), so `DenyRecursively` recurses through every nesting
///   instruction type without short-circuiting on a denied instruction.
pub fn deny_reserve_transfer_recursive_barrier_check<Call>(
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
