// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Primitives that may be used at (bridges) runtime level.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use sp_io::hashing::blake2_256;

pub use chain::{BlockNumberOf, Chain, HashOf, HasherOf, HeaderOf};

mod chain;

/// Use this when something must be shared among all instances.
pub const NO_INSTANCE_ID: InstanceId = [0, 0, 0, 0];

/// Call-dispatch module prefix.
pub const CALL_DISPATCH_MODULE_PREFIX: &[u8] = b"pallet-bridge/call-dispatch";

/// Message-lane module prefix.
pub const MESSAGE_LANE_MODULE_PREFIX: &[u8] = b"pallet-bridge/message-lane";

/// Id of deployed module instance. We have a bunch of pallets that may be used in
/// different bridges. E.g. message-lane pallet may be deployed twice in the same
/// runtime to bridge ThisChain with Chain1 and Chain2. Sometimes we need to be able
/// to identify deployed instance dynamically. This type is used for that.
pub type InstanceId = [u8; 4];

/// Returns id of account that acts as "system" account of given bridge instance.
/// The `module_prefix` (arbitrary slice) may be used to generate module-level
/// "system" account, so you could have separate "system" accounts for currency
/// exchange, message dispatch and other modules.
///
/// The account is not supposed to actually exists on the chain, or to have any funds.
/// It is only used to
pub fn bridge_account_id<AccountId>(bridge: InstanceId, module_prefix: &[u8]) -> AccountId
where
	AccountId: Decode + Default,
{
	let entropy = (module_prefix, bridge).using_encoded(blake2_256);
	AccountId::decode(&mut &entropy[..]).unwrap_or_default()
}
