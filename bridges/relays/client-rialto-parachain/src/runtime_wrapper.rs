// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Types that are specific to the `RialtoParachain` runtime. Normally we could use the full
//! `RialtoParachain` runtime here, since it is constructed in this repo and we have access to it.
//! However we use a wrapped runtime instead in order to test the indirect runtime calls
//! functionality.

use codec::{Decode, Encode};
use scale_info::TypeInfo;

use bp_header_chain::BridgeGrandpaCallOf;
use bridge_runtime_common::messages::BridgeMessagesCallOf;
use relay_substrate_client::calls::{SudoCall, XcmCall};

// The indirect pallet call used to sync `Millau` GRANDPA finality to `RialtoParachain`.
pub type BridgeMillauGrandpaCall = BridgeGrandpaCallOf<bp_millau::Millau>;
// The indirect pallet call used to sync `Millau` messages to `RialtoParachain`.
pub type BridgeMillauMessagesCall = BridgeMessagesCallOf<bp_millau::Millau>;

/// `RialtoParachain` Runtime `Call` enum.
///
/// The enum represents a subset of possible `Call`s we can send to `RialtoParachain` chain.
///
/// All entries here (like pretty much in the entire file) must be kept in sync with
/// `RialtoParachain` `construct_runtime`, so that we maintain SCALE-compatibility.
#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	/// `Sudo` pallet.
	#[codec(index = 2)]
	Sudo(SudoCall<Call>),

	/// `Xcm` pallet.
	#[codec(index = 51)]
	PolkadotXcm(XcmCall),

	/// Millau GRANDPA bridge pallet.
	#[codec(index = 55)]
	BridgeMillauGrandpa(BridgeMillauGrandpaCall),
	/// Millau messages bridge pallet.
	#[codec(index = 56)]
	BridgeMillauMessages(BridgeMillauMessagesCall),
}
