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
use coretime_rococo_runtime::xcm_config::{
	XcmConfig as CoretimeRococoConfig, XcmConfig as RococoXcmConfig,
};
use rococo_system_emulated_network::coretime_rococo_emulated_chain::CoretimeRococo;

#[test]
fn example() {
	// Init tests vars
	// XcmPallet send args
	let sudo_origin = <Rococo as Chain>::RuntimeOrigin::root();
	let destination = Rococo::child_location_of(CoretimeRococo::para_id()).into();
	let weight_limit = WeightLimit::Unlimited;
	let check_origin = None;

	let remove_xcm = Xcm(vec![ClearOrigin]);

	let xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit, check_origin },
		ExportMessage {
			network: WococoId,
			destination: X1(Parachain(CoretimeWococo::para_id().into())),
			xcm: remote_xcm,
		},
	]));
}
