// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
#![cfg_attr(not(feature = "std"), no_std)]

use snowbridge_beacon_primitives::CompactExecutionHeader;
use snowbridge_core::inbound::Message;
use sp_core::RuntimeDebug;

pub mod register_token;
pub mod send_token;

#[derive(Clone, RuntimeDebug)]
pub struct InboundQueueFixture {
	pub execution_header: CompactExecutionHeader,
	pub message: Message,
}
