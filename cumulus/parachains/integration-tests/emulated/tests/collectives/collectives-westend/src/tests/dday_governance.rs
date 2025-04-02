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

use crate::*;
use pallet_dday_detection::IsStalled;

#[test]
fn stalled_asset_hub_detection_works() {
	// Check Collectives before - no data, not stalled
	CollectivesWestend::execute_with(|| {
		assert!(AssetHubDDayDetection::last_known_head().is_none());
		assert!(!AssetHubDDayDetection::is_stalled());
	});

	// Let's progress AssetHub with new blocks (which triggers `DDayHook::on_finalize`).
	let asset_hub_parent_head =
		AssetHubWestend::execute_with(|| <AssetHubWestend as Chain>::System::block_number());
	AssetHubWestend::execute_with(|| {
		assert_eq!(asset_hub_parent_head, <AssetHubWestend as Chain>::System::block_number() - 1);
	});

	// Check Collectives that we processed new AssetHub data (header, total issuance).
	CollectivesWestend::execute_with(|| {
		// received some data
		assert_eq!(
			AssetHubDDayDetection::last_known_head().map(|h| h.block_number),
			Some(asset_hub_parent_head)
		);
		// but not stalled
		assert!(!AssetHubDDayDetection::is_stalled());
	});

	// Let's progress blocks only for Collectives after `StalledAssetHubBlockThreshold`,
	// which means that we did not receive AssetHub update for a long time => means is stalled.
	CollectivesWestend::execute_with(|| {
		assert!(!AssetHubDDayDetection::is_stalled());

		// Pretend Collectives progressed lots of blocks hitting `StalledAssetHubBlockThreshold`
		// threshold.
		let block_number = <CollectivesWestend as Chain>::System::block_number();
		<CollectivesWestend as Chain>::System::set_block_number(
			block_number + collectives_dday::StalledAssetHubBlockThreshold::get(),
		);

		// Now the AssetHub is detected as stalled.
		assert!(AssetHubDDayDetection::is_stalled());
	});
}
