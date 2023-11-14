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
fn coretime_executes_xcm(t: RelayToSystemParaTest) {
	CoretimeRococo::execute_with(|| {
		type RuntimeEvent = <CoretimeRococo as Chain>::RuntimeEvent;

		//@TODO Dummy Weight values for now
		CoretimeRococo::assert_dmp_queue_complete(Some(Weight::from_parts(1_019_210_000, 200_000)));

		assert_expected_events!(
			CoretimeRococo,
			vec![
				RuntimeEvent::CoretimePallet(coretime::Event::CoreAssigned {
					core: t.core,
					who: t.who,
				}) => { core: *core == t.core, who: *who == t.who, },
			]
		);
	})
}
