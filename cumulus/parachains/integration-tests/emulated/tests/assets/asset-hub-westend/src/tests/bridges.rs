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

use crate::imports::*;

/// Relay Chain should be able to open and close lane in System Parachain
/// when `OriginKind::Superuser`.
#[test]
fn open_close_lane() {
	let _ = AssetHubWestend::execute_with(||  { <AssetHubWestend as AssetHubWestendPallet>::XcmOverAssetHubRococo::open_bridge(
		AssetHubWestendRuntimeOrigin::root(),
		Box::new(AssetHubRococoUniversalLocation::get().into()),
		Some(Receiver::new(13, 15)),
	)});
	let _ = AssetHubRococo::execute_with(||  { <AssetHubRococo as AssetHubRococoPallet>::XcmOverAssetHubWestend::open_bridge(
		AssetHubRococoRuntimeOrigin::root(),
		Box::new(AssetHubWestendUniversalLocation::get().into()),
		Some(Receiver::new(13, 15)),
	)});

	let events = AssetHubWestend::ext_wrapper(|| AssetHubWestend::events());
	type RuntimeEventWestend = <AssetHubWestend as Chain>::RuntimeEvent;
	assert!(
		events.iter().any(|event| matches!(
			event,
			RuntimeEventWestend::XcmOverAssetHubRococo(
				pallet_xcm_bridge::Event::BridgeOpened { .. }
			),
		)),
		"Event BridgeOpened not found"
	);

	let events = AssetHubRococo::ext_wrapper(|| AssetHubRococo::events());
	type RuntimeEventRococo = <AssetHubRococo as Chain>::RuntimeEvent;
	assert!(
		events.iter().any(|event| matches!(
			event,
			RuntimeEventRococo::XcmOverAssetHubWestend(
				pallet_xcm_bridge::Event::BridgeOpened { .. }
			),
		)),
		"Event BridgeOpened not found"
	);

	let _ = AssetHubWestend::execute_with(|| { <AssetHubWestend as AssetHubWestendPallet>::XcmOverAssetHubRococo::close_bridge(
		AssetHubWestendRuntimeOrigin::root(),
		Box::new(AssetHubRococoUniversalLocation::get().into()),
		0,
	)});
	let _ = AssetHubRococo::execute_with(|| { <AssetHubRococo as AssetHubRococoPallet>::XcmOverAssetHubWestend::close_bridge(
		AssetHubRococoRuntimeOrigin::root(),
		Box::new(AssetHubWestendUniversalLocation::get().into()),
		0,
	)});
}
