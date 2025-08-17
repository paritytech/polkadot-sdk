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

//! Pallet provides a set of guard functions that are running in background threads
//! and are aborting process if some condition fails.

//! Test chain implementation to use in tests.

#![cfg(any(feature = "test-helpers", test))]

use crate::{
	Chain, ChainWithBalances, ChainWithMessages, ChainWithRewards, ChainWithTransactions,
	Error as SubstrateError, SignParam, UnsignedTransaction,
};
use bp_messages::{ChainWithMessages as ChainWithMessagesBase, MessageNonce};
use bp_runtime::ChainId;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{sp_runtime::StateVersion, weights::Weight};
use scale_info::TypeInfo;
use std::time::Duration;

/// Chain that may be used in tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestChain;

impl bp_runtime::Chain for TestChain {
	const ID: ChainId = *b"test";

	type BlockNumber = u32;
	type Hash = sp_core::H256;
	type Hasher = sp_runtime::traits::BlakeTwo256;
	type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;

	type AccountId = u32;
	type Balance = u32;
	type Nonce = u32;
	type Signature = sp_runtime::testing::TestSignature;

	const STATE_VERSION: StateVersion = StateVersion::V1;

	fn max_extrinsic_size() -> u32 {
		100000
	}

	fn max_extrinsic_weight() -> Weight {
		unreachable!()
	}
}

impl Chain for TestChain {
	const NAME: &'static str = "Test";
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str = "TestMethod";
	const FREE_HEADERS_INTERVAL_METHOD: &'static str = "TestMethod";
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_millis(0);

	type SignedBlock = sp_runtime::generic::SignedBlock<
		sp_runtime::generic::Block<Self::Header, sp_runtime::OpaqueExtrinsic>,
	>;
	type Call = TestRuntimeCall;
}

impl ChainWithBalances for TestChain {
	fn account_info_storage_key(_account_id: &u32) -> sp_core::storage::StorageKey {
		unreachable!()
	}
}

/// Reward type for the test chain.
#[derive(
	Clone,
	Copy,
	Debug,
	Decode,
	DecodeWithMemTracking,
	Encode,
	Eq,
	MaxEncodedLen,
	PartialEq,
	TypeInfo,
)]
pub enum ChainReward {
	/// Reward 1 type.
	Reward1,
}

impl ChainWithRewards for TestChain {
	const WITH_CHAIN_RELAYERS_PALLET_NAME: Option<&'static str> = None;
	type RewardBalance = u128;
	type Reward = ChainReward;

	fn account_reward_storage_key(
		_account_id: &Self::AccountId,
		_reward: impl Into<Self::Reward>,
	) -> sp_core::storage::StorageKey {
		unreachable!()
	}
}

impl ChainWithMessagesBase for TestChain {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str = "Test";
	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 0;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 0;
}

impl ChainWithMessages for TestChain {
	const TO_CHAIN_MESSAGE_DETAILS_METHOD: &'static str = "TestMessagesDetailsMethod";
	const FROM_CHAIN_MESSAGE_DETAILS_METHOD: &'static str = "TestFromMessagesDetailsMethod";
}

impl ChainWithTransactions for TestChain {
	type AccountKeyPair = sp_core::sr25519::Pair;
	type SignedTransaction = bp_polkadot_core::UncheckedExtrinsic<
		TestRuntimeCall,
		bp_polkadot_core::SuffixedCommonTransactionExtension<(
			bp_runtime::extensions::BridgeRejectObsoleteHeadersAndMessages,
			bp_runtime::extensions::RefundBridgedParachainMessagesSchema,
		)>,
	>;

	fn sign_transaction(
		_param: SignParam<Self>,
		_unsigned: UnsignedTransaction<Self>,
	) -> Result<Self::SignedTransaction, SubstrateError> {
		unreachable!()
	}
}

/// Dummy runtime call.
#[derive(Decode, Encode, Clone, Debug, PartialEq)]
pub enum TestRuntimeCall {
	/// Dummy call.
	#[codec(index = 0)]
	Dummy,
}

/// Primitives-level parachain that may be used in tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestParachainBase;

impl bp_runtime::Chain for TestParachainBase {
	const ID: ChainId = *b"tstp";

	type BlockNumber = u32;
	type Hash = sp_core::H256;
	type Hasher = sp_runtime::traits::BlakeTwo256;
	type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;

	type AccountId = u32;
	type Balance = u32;
	type Nonce = u32;
	type Signature = sp_runtime::testing::TestSignature;

	const STATE_VERSION: StateVersion = StateVersion::V1;

	fn max_extrinsic_size() -> u32 {
		unreachable!()
	}

	fn max_extrinsic_weight() -> Weight {
		unreachable!()
	}
}

impl bp_runtime::Parachain for TestParachainBase {
	const PARACHAIN_ID: u32 = 1000;
	const MAX_HEADER_SIZE: u32 = 1_024;
}

/// Parachain that may be used in tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestParachain;

impl bp_runtime::UnderlyingChainProvider for TestParachain {
	type Chain = TestParachainBase;
}

impl Chain for TestParachain {
	const NAME: &'static str = "TestParachain";
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str = "TestParachainMethod";
	const FREE_HEADERS_INTERVAL_METHOD: &'static str = "TestParachainMethod";
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_millis(0);

	type SignedBlock = sp_runtime::generic::SignedBlock<
		sp_runtime::generic::Block<Self::Header, sp_runtime::OpaqueExtrinsic>,
	>;
	type Call = TestRuntimeCall;
}
