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
use pallet_broker::{CoreAssignment, CoreIndex, CoretimeInterface, PartsOf57600};

#[test]
fn assign_core() {
	CoretimeRococo::execute_with(|| {
		type RuntimeEvent = <CoretimeRococo as Chain>::RuntimeEvent;
		let core_index = CoreIndex::from(1_u16);
		let assignment = vec![(CoreAssignment::Idle, PartsOf57600::from(1_u8))];

		assert_ok!(<CoretimeRococo as CoretimeRococoPallet>::CoretimeProvider::assign_core(
			<CoretimeRococo as Chain>::RuntimeOrigin::signed(CoretimeRococSender::get()),
			core_index,
			1,
			assignment,
			None,
		));
	})
}
