// Copyright 2020 Parity Technologies (UK) Ltd.
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

//! Mock Runtime for Substrate Pallet Testing.
//!
//! Includes some useful testing utilities in the `helpers` module.

#![cfg(test)]

use crate::Config;
use bp_runtime::Chain;
use frame_support::{impl_outer_origin, parameter_types, weights::Weight};
use sp_runtime::{
	testing::{Header, H256},
	traits::{BlakeTwo256, IdentityLookup},
	Perbill,
};

pub type AccountId = u64;

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct TestRuntime;

impl_outer_origin! {
	pub enum Origin for TestRuntime where system = frame_system {}
}

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const MaximumBlockWeight: Weight = 1024;
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
}

impl frame_system::Config for TestRuntime {
	type Origin = Origin;
	type Index = u64;
	type Call = ();
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = ();
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = ();
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type BaseCallFilter = ();
	type SystemWeightInfo = ();
	type DbWeight = ();
	type BlockWeights = ();
	type BlockLength = ();
}

impl Config for TestRuntime {
	type BridgedChain = TestBridgedChain;
}

#[derive(Debug)]
pub struct TestBridgedChain;

impl Chain for TestBridgedChain {
	type BlockNumber = <TestRuntime as frame_system::Config>::BlockNumber;
	type Hash = <TestRuntime as frame_system::Config>::Hash;
	type Hasher = <TestRuntime as frame_system::Config>::Hashing;
	type Header = <TestRuntime as frame_system::Config>::Header;
}

pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	sp_io::TestExternalities::new(Default::default()).execute_with(test)
}

pub mod helpers {
	use super::*;
	use crate::storage::ImportedHeader;
	use crate::{BridgedBlockHash, BridgedBlockNumber, BridgedHeader};
	use finality_grandpa::voter_set::VoterSet;
	use sp_finality_grandpa::{AuthorityId, AuthorityList};
	use sp_keyring::Ed25519Keyring;

	pub type TestHeader = BridgedHeader<TestRuntime>;
	pub type TestNumber = BridgedBlockNumber<TestRuntime>;
	pub type TestHash = BridgedBlockHash<TestRuntime>;
	pub type HeaderId = (TestHash, TestNumber);

	pub fn test_header(num: TestNumber) -> TestHeader {
		let mut header = TestHeader::new_from_number(num);
		header.parent_hash = if num == 0 {
			Default::default()
		} else {
			test_header(num - 1).hash()
		};

		header
	}

	pub fn unfinalized_header(num: u64) -> ImportedHeader<TestHeader> {
		ImportedHeader {
			header: test_header(num),
			requires_justification: false,
			is_finalized: false,
			signal_hash: None,
		}
	}

	pub fn header_id(index: u8) -> HeaderId {
		(test_header(index.into()).hash(), index as _)
	}

	pub fn extract_keyring(id: &AuthorityId) -> Ed25519Keyring {
		let mut raw_public = [0; 32];
		raw_public.copy_from_slice(id.as_ref());
		Ed25519Keyring::from_raw_public(raw_public).unwrap()
	}

	pub fn voter_set() -> VoterSet<AuthorityId> {
		VoterSet::new(authority_list()).unwrap()
	}

	pub fn authority_list() -> AuthorityList {
		vec![(alice(), 1), (bob(), 1), (charlie(), 1)]
	}

	pub fn alice() -> AuthorityId {
		Ed25519Keyring::Alice.public().into()
	}

	pub fn bob() -> AuthorityId {
		Ed25519Keyring::Bob.public().into()
	}

	pub fn charlie() -> AuthorityId {
		Ed25519Keyring::Charlie.public().into()
	}
}
