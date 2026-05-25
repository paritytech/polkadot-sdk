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

use xcm::latest::prelude::*;

/// Worst-case `(origin, message)` for barriers using
/// `DenyRecursively<DenyReserveTransferToRelayChain>`.
pub fn deny_reserve_transfer_recursive_barrier_check<Call>(
	origin: Location,
	relay_chain: Location,
) -> (Location, Xcm<Call>) {
	let reserve = Xcm::<Call>(vec![DepositReserveAsset {
		assets: Wild(All),
		dest: relay_chain,
		xcm: Xcm(vec![ClearOrigin].into()),
	}]);
	let message = Xcm(vec![ExecuteWithOrigin {
		xcm: Xcm(vec![SetErrorHandler(Xcm(vec![SetAppendix(reserve.into())].into()))].into()),
		descendant_origin: None,
	}]);
	(origin, message)
}

/// Worst-case `(origin, message)` for barriers using `WithComputedOrigin` with `max_prefixes`
/// origin alterations followed by paid execution.
pub fn with_computed_origin_barrier_check<Call>(
	origin: Location,
	max_prefixes: u32,
	fee_asset: Location,
	fee_amount: u128,
) -> (Location, Xcm<Call>) {
	let mut instructions: Vec<Instruction<Call>> =
		(0..max_prefixes).map(|i| DescendOrigin(Parachain(i + 1000).into())).collect();
	instructions.push(WithdrawAsset((fee_asset.clone(), fee_amount).into()));
	instructions
		.push(BuyExecution { fees: (fee_asset, fee_amount).into(), weight_limit: Unlimited });
	instructions.push(SetTopic([0u8; 32]));
	(origin, Xcm(instructions))
}
