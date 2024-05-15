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

use relay_substrate_client::{Chain, ChainWithUtilityPallet, UtilityPallet};

use std::marker::PhantomData;

// to avoid `finality_relay` dependency in other crates
pub use finality_relay::HeadersToRelay;

pub mod cli;
pub mod equivocation;
pub mod error;
pub mod finality;
pub mod finality_base;
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
	/// Account, used to sign message (also headers and parachains) relay transactions from given
	/// bridged chain.
	Messages {
		/// Account id.
		id: AccountId,
		/// Name of the bridged chain, which sends us messages or delivery confirmations.
		bridged_chain: String,
	},
}

impl<AccountId> TaggedAccount<AccountId> {
	/// Returns reference to the account id.
	pub fn id(&self) -> &AccountId {
		match *self {
			TaggedAccount::Messages { ref id, .. } => id,
		}
	}

	/// Returns stringified account tag.
	pub fn tag(&self) -> String {
		match *self {
			TaggedAccount::Messages { ref bridged_chain, .. } => {
				format!("{bridged_chain}Messages")
			},
		}
	}
}

/// Batch call builder.
pub trait BatchCallBuilder<Call>: Clone + Send + Sync {
	/// Create batch call from given calls vector.
	fn build_batch_call(&self, _calls: Vec<Call>) -> Call;
}

/// Batch call builder constructor.
pub trait BatchCallBuilderConstructor<Call>: Clone {
	/// Call builder, used by this constructor.
	type CallBuilder: BatchCallBuilder<Call>;
	/// Create a new instance of a batch call builder.
	fn new_builder() -> Option<Self::CallBuilder>;
}

/// Batch call builder based on `pallet-utility`.
#[derive(Clone)]
pub struct UtilityPalletBatchCallBuilder<C: Chain>(PhantomData<C>);

impl<C: Chain> BatchCallBuilder<C::Call> for UtilityPalletBatchCallBuilder<C>
where
	C: ChainWithUtilityPallet,
{
	fn build_batch_call(&self, calls: Vec<C::Call>) -> C::Call {
		C::UtilityPallet::build_batch_call(calls)
	}
}

impl<C: Chain> BatchCallBuilderConstructor<C::Call> for UtilityPalletBatchCallBuilder<C>
where
	C: ChainWithUtilityPallet,
{
	type CallBuilder = Self;

	fn new_builder() -> Option<Self::CallBuilder> {
		Some(Self(Default::default()))
	}
}

// A `BatchCallBuilderConstructor` that always returns `None`.
impl<Call> BatchCallBuilderConstructor<Call> for () {
	type CallBuilder = ();
	fn new_builder() -> Option<Self::CallBuilder> {
		None
	}
}

// Dummy `BatchCallBuilder` implementation that must never be used outside
// of the `impl BatchCallBuilderConstructor for ()` code.
impl<Call> BatchCallBuilder<Call> for () {
	fn build_batch_call(&self, _calls: Vec<Call>) -> Call {
		unreachable!("never called, because ()::new_builder() returns None; qed")
	}
}
