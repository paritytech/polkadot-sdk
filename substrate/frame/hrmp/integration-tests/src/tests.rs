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

//! End-to-end tests for HRMP channel management.
//!
//! One test per flow lands with the flow it covers. What is here is what can be asserted while
//! the pallets' bodies are `todo!()`.

use crate::{relay, senders, MockNet, Relay, PARA_ID};
use frame_support::traits::EnsureOrigin;
use hrmp_primitives::{ChannelId, MessageToRelay, MessageToRelayV1};
use polkadot_runtime_parachains::Origin as ParachainsOrigin;
use xcm_simulator::TestExt;

const CHANNEL: ChannelId = ChannelId { sender: 2000, recipient: 2001 };

#[test]
fn only_the_channel_managing_parachain_may_drive_hrmp() {
	MockNet::reset();

	Relay::execute_with(|| {
		let message = MessageToRelay::V1(MessageToRelayV1::InitOpenChannel {
			channel: CHANNEL,
			message_id: 0,
			max_capacity: crate::MAX_CAPACITY,
			max_message_size: crate::MAX_MESSAGE_SIZE,
		});

		// A different parachain's origin is not accepted...
		let other_para: relay::RuntimeOrigin =
			ParachainsOrigin::Parachain((PARA_ID + 1).into()).into();
		assert!(senders::EnsureHrmpPara::try_origin(other_para.clone()).is_err());
		assert!(relay::Hrmp::receive(other_para, message.clone()).is_err());

		// ...nor is a plain signed account.
		assert!(
			senders::EnsureHrmpPara::try_origin(relay::RuntimeOrigin::signed(crate::BOB)).is_err()
		);
		assert!(relay::Hrmp::receive(relay::RuntimeOrigin::signed(crate::BOB), message).is_err());

		// The configured parachain is. Dispatching it is left to the flow tests, since the
		// handlers are still `todo!()`.
		let ours: relay::RuntimeOrigin = ParachainsOrigin::Parachain(PARA_ID.into()).into();
		assert!(senders::EnsureHrmpPara::try_origin(ours).is_ok());
	});
}
