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
	let _ = <AssetHubWestend as AssetHubWestendPallet>::XcmOverAssetHubRococo::open_bridge(
		AssetHubWestendRuntimeOrigin::root(),
		Box::new(AssetHubRococoUniversalLocation::get().into()),
		None,
	);
	let _ = <AssetHubRococo as AssetHubRococoPallet>::XcmOverAssetHubWestend::open_bridge(
		AssetHubRococoRuntimeOrigin::root(),
		Box::new(AssetHubWestendUniversalLocation::get().into()),
		None,
	);
}
