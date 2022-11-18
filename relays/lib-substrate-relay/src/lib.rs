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

//! The library of substrate relay. contains some public codes to provide to substrate relay.

#![warn(missing_docs)]

pub mod error;
pub mod finality;
pub mod messages_lane;
pub mod messages_metrics;
pub mod messages_source;
pub mod messages_target;
pub mod on_demand;
pub mod parachains;

/// Transaction creation parameters.
#[derive(Clone, Debug)]
pub struct TransactionParams<TS> {
	/// Transactions author.
	pub signer: TS,
	/// Transactions mortality.
	pub mortality: Option<u32>,
}

/// Tagged relay account, which balance may be exposed as metrics by the relay.
#[derive(Clone, Debug)]
pub enum TaggedAccount<AccountId> {
	/// Account, used to sign headers relay transactions from given bridged chain.
	Headers {
		/// Account id.
		id: AccountId,
		/// Name of the bridged chain, which headers are relayed.
		bridged_chain: String,
	},
	/// Account, used to sign parachains relay transactions from given bridged relay chain.
	Parachains {
		/// Account id.
		id: AccountId,
		/// Name of the bridged relay chain with parachain heads.
		bridged_chain: String,
	},
	/// Account, used to sign message relay transactions from given bridged chain.
	Messages {
		/// Account id.
		id: AccountId,
		/// Name of the bridged chain, which sends us messages or delivery confirmations.
		bridged_chain: String,
	},
	/// Account, used to sign messages with-bridged-chain pallet parameters update transactions.
	MessagesPalletOwner {
		/// Account id.
		id: AccountId,
		/// Name of the chain, bridged using messages pallet at our chain.
		bridged_chain: String,
	},
}

impl<AccountId> TaggedAccount<AccountId> {
	/// Returns reference to the account id.
	pub fn id(&self) -> &AccountId {
		match *self {
			TaggedAccount::Headers { ref id, .. } => id,
			TaggedAccount::Parachains { ref id, .. } => id,
			TaggedAccount::Messages { ref id, .. } => id,
			TaggedAccount::MessagesPalletOwner { ref id, .. } => id,
		}
	}

	/// Returns stringified account tag.
	pub fn tag(&self) -> String {
		match *self {
			TaggedAccount::Headers { ref bridged_chain, .. } => format!("{bridged_chain}Headers"),
			TaggedAccount::Parachains { ref bridged_chain, .. } => {
				format!("{bridged_chain}Parachains")
			},
			TaggedAccount::Messages { ref bridged_chain, .. } => {
				format!("{bridged_chain}Messages")
			},
			TaggedAccount::MessagesPalletOwner { ref bridged_chain, .. } => {
				format!("{bridged_chain}MessagesPalletOwner")
			},
		}
	}
}
