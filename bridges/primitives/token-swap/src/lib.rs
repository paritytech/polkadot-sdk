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

use codec::{Decode, Encode};
use frame_support::RuntimeDebug;
use sp_core::U256;

/// Token swap type.
///
/// Different swap types give a different guarantees regarding possible swap
/// replay protection.
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum TokenSwapType<ThisBlockNumber> {
	/// The `target_account_at_bridged_chain` is temporary and only have funds for single swap.
	///
	/// ***WARNING**: if `target_account_at_bridged_chain` still exists after the swap has been
	/// completed (either by claiming or cancelling), the `source_account_at_this_chain` will be able
	/// to restart the swap again and repeat the swap until `target_account_at_bridged_chain` depletes.
	TemporaryTargetAccountAtBridgedChain,
	/// This swap type prevents `source_account_at_this_chain` from restarting the swap after it has
	/// been completed. There are two consequences:
	///
	/// 1) the `source_account_at_this_chain` won't be able to call `start_swap` after given <ThisBlockNumber>;
	/// 2) the `target_account_at_bridged_chain` won't be able to call `claim_swap` (over the bridge) before
	///    block `<ThisBlockNumber + 1>`.
	///
	/// The second element is the nonce of the swap. You must care about its uniqueness if you're
	/// planning to perform another swap with exactly the same parameters (i.e. same amount, same accounts,
	/// same `ThisBlockNumber`) to avoid collisions.
	LockClaimUntilBlock(ThisBlockNumber, U256),
}

/// An intention to swap `source_balance_at_this_chain` owned by `source_account_at_this_chain`
/// to `target_balance_at_bridged_chain` owned by `target_account_at_bridged_chain`.
///
/// **IMPORTANT NOTE**: this structure is always the same during single token swap. So even
/// when chain changes, the meaning of This and Bridged are still used to point to the same chains.
/// This chain is always the chain where swap has been started. And the Bridged chain is the other chain.
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct TokenSwap<ThisBlockNumber, ThisBalance, ThisAccountId, BridgedBalance, BridgedAccountId> {
	/// The type of the swap.
	pub swap_type: TokenSwapType<ThisBlockNumber>,
	/// This chain balance to be swapped with `target_balance_at_bridged_chain`.
	pub source_balance_at_this_chain: ThisBalance,
	/// Account id of the party acting at This chain and owning the `source_account_at_this_chain`.
	pub source_account_at_this_chain: ThisAccountId,
	/// Bridged chain balance to be swapped with `source_balance_at_this_chain`.
	pub target_balance_at_bridged_chain: BridgedBalance,
	/// Account id of the party acting at the Bridged chain and owning the `target_balance_at_bridged_chain`.
	pub target_account_at_bridged_chain: BridgedAccountId,
}
