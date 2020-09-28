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

use crate::client::Client;

use frame_support::Parameter;
use jsonrpsee::common::{DeserializeOwned, Serialize};
use sp_core::Pair;
use sp_runtime::traits::{
	AtLeast32Bit, AtLeast32BitUnsigned, Bounded, CheckEqual, Dispatchable, Header as HeaderT, MaybeDisplay,
	MaybeMallocSizeOf, MaybeSerialize, MaybeSerializeDeserialize, Member, SimpleBitOps,
};
use sp_std::fmt::Debug;

/// Substrate-based chain from minimal relay-client point of view.
pub trait Chain {
	/// The block number type used by the runtime.
	type BlockNumber: Parameter
		+ Member
		+ MaybeSerializeDeserialize
		+ Debug
		+ MaybeDisplay
		+ AtLeast32BitUnsigned
		+ Default
		+ Bounded
		+ Copy
		+ sp_std::hash::Hash
		+ sp_std::str::FromStr
		+ MaybeMallocSizeOf;
	/// The output of the `Hashing` function.
	type Hash: Parameter
		+ Member
		+ MaybeSerializeDeserialize
		+ Debug
		+ MaybeDisplay
		+ SimpleBitOps
		+ Ord
		+ Default
		+ Copy
		+ CheckEqual
		+ sp_std::hash::Hash
		+ AsRef<[u8]>
		+ AsMut<[u8]>
		+ MaybeMallocSizeOf;
	/// The block header.
	type Header: Parameter + HeaderT<Number = Self::BlockNumber, Hash = Self::Hash>;
	/// The user account identifier type for the runtime.
	type AccountId: Parameter + Member + MaybeSerializeDeserialize + Debug + MaybeDisplay + Ord + Default;
	/// Account index (aka nonce) type. This stores the number of previous transactions associated
	/// with a sender account.
	type Index: Parameter + Member + MaybeSerialize + Debug + Default + MaybeDisplay + AtLeast32Bit + Copy;
	/// Block type.
	type SignedBlock: Member + Serialize + DeserializeOwned;
	/// The aggregated `Call` type.
	type Call: Dispatchable + Debug;
}

/// Substrate-based chain transactions signing scheme.
pub trait TransactionSignScheme {
	/// Chain that this scheme is to be used.
	type Chain: Chain;
	/// Type of key pairs used to sign transactions.
	type AccountKeyPair: Pair;
	/// Signed transaction.
	type SignedTransaction;

	/// Create transaction for given runtime call, signed by given account.
	fn sign_transaction(
		client: &Client<Self::Chain>,
		signer: &Self::AccountKeyPair,
		signer_nonce: <Self::Chain as Chain>::Index,
		call: <Self::Chain as Chain>::Call,
	) -> Self::SignedTransaction;
}
