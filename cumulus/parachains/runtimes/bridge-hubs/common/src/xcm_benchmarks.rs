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

use xcm::latest::prelude::*;

/// Worst-case `(origin, message)` for bridge hub barriers that deny reserve transfers to the relay
/// and exports to a given network, both checked recursively.
pub fn bridge_hub_barrier_check<Call>(
	origin: Location,
	relay_chain: Location,
	ethereum_network: NetworkId,
) -> (Location, Xcm<Call>) {
	let inner = Xcm::<Call>(vec![
		DepositReserveAsset {
			assets: Wild(All),
			dest: relay_chain,
			xcm: Default::default(),
		},
		ExportMessage { network: ethereum_network, destination: Here, xcm: Default::default() },
	]);
	let message = Xcm(vec![ExecuteWithOrigin {
		xcm: Xcm(vec![SetErrorHandler(Xcm(vec![SetAppendix(inner.into())].into()))].into()),
		descendant_origin: None,
	}]);
	(origin, message)
}
