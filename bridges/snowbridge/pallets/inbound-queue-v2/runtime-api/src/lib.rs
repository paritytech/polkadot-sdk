// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

//! Ethereum Inbound Queue V2 Runtime API
//!
//! * `is_message_relayed`: Check if a message with the given nonce has been relayed.

#![cfg_attr(not(feature = "std"), no_std)]

sp_api::decl_runtime_apis! {
	pub trait InboundQueueV2Api
	{
		/// Check if a message with the given nonce has been relayed.
		fn is_message_relayed(nonce: u64) -> bool;
	}
}
