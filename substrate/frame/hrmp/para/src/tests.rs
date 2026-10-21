// This file is part of Substrate.

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

use crate::{mock::Test, Call, RECEIVE_CALL_INDEX};
use frame_support::traits::GetCallIndex;
use hrmp_primitives::{ChannelId, MessageToPara, MessageToParaV1};

/// The relay chain encodes `receive` with [`RECEIVE_CALL_INDEX`].
#[test]
fn receive_call_index_ok() {
	let channel = ChannelId { sender: 0, recipient: 0 };
	let message =
		MessageToPara::V1(MessageToParaV1::Release { channel, sender: false, recipient: false });
	let call = Call::<Test>::receive { message };
	assert_eq!(call.get_call_index(), RECEIVE_CALL_INDEX);
}
