// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::traits::tokens::Balance as BalanceT;
use snowbridge_router_primitives::inbound::v2::Message;
use sp_runtime::DispatchError;
use xcm::latest::Xcm;

sp_api::decl_runtime_apis! {
	pub trait InboundQueueApiV2<Balance> where Balance: BalanceT
	{
		/// Dry runs the provided message to provide the XCM payload and execution cost.
		fn dry_run(message: Message) -> Result<(Xcm<()>, Balance), DispatchError>;
	}
}
