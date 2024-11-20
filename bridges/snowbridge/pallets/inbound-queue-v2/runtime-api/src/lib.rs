// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
#![cfg_attr(not(feature = "std"), no_std)]

use snowbridge_core::inbound::Proof;
use snowbridge_router_primitives::inbound::v2::Message;
use xcm::latest::Xcm;

sp_api::decl_runtime_apis! {
	pub trait InboundQueueApiV2
	{
		/// Dry runs the provided message on AH to provide the XCM payload and execution cost.
		fn dry_run(message: Message, proof: Proof) -> (Xcm<()>, u128);
	}
}
