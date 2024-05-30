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

use crate::{Chain, ChainWithBalances, ChainWithMessages};
use bp_messages::{ChainWithMessages as ChainWithMessagesBase, MessageNonce};
use bp_runtime::ChainId;
use frame_support::weights::Weight;
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
	type Call = ();
}

impl ChainWithBalances for TestChain {
	fn account_info_storage_key(_account_id: &u32) -> sp_core::storage::StorageKey {
		unreachable!()
	}
}

impl ChainWithMessagesBase for TestChain {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str = "Test";
	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 0;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 0;
}

impl ChainWithMessages for TestChain {
	const WITH_CHAIN_RELAYERS_PALLET_NAME: Option<&'static str> = None;
	const TO_CHAIN_MESSAGE_DETAILS_METHOD: &'static str = "TestMessagesDetailsMethod";
	const FROM_CHAIN_MESSAGE_DETAILS_METHOD: &'static str = "TestFromMessagesDetailsMethod";
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
	type Call = ();
}
