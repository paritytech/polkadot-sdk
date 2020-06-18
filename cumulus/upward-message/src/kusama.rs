// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <http://www.gnu.org/licenses/>.

//! Kusama upward message

use crate::*;
use polkadot_core_primitives::{Balance, AccountId};
use kusama_runtime::{BalancesCall, ParachainsCall};
use sp_std::vec::Vec;

/// The Kusama upward message.
pub type UpwardMessage = kusama_runtime::Call;

impl BalancesMessage<AccountId, Balance> for UpwardMessage {
	fn transfer(dest: AccountId, amount: Balance) -> Self {
		BalancesCall::transfer(dest, amount).into()
	}
}

impl XCMPMessage for UpwardMessage {
	fn send_message(dest: ParaId, msg: Vec<u8>) -> Self {
		ParachainsCall::send_xcmp_message(dest, msg).into()
	}
}
