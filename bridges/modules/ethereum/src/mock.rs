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

use crate::finality::FinalityVotes;
use crate::validators::{ValidatorsConfiguration, ValidatorsSource};
use crate::{AuraConfiguration, GenesisConfig, HeaderToImport, HeadersByNumber, PruningStrategy, Storage, Trait};
use frame_support::StorageMap;
use frame_support::{impl_outer_origin, parameter_types, weights::Weight};
use parity_crypto::publickey::{sign, KeyPair, Secret};
use primitives::{rlp_encode, H520};
use primitives::{Address, Header, H256, U256};
use sp_runtime::{
	testing::Header as SubstrateHeader,
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

impl frame_system::Trait for TestRuntime {
	type Origin = Origin;
	type Index = u64;
	type Call = ();
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = SubstrateHeader;
	type Event = ();
	type BlockHashCount = BlockHashCount;
	type MaximumBlockWeight = MaximumBlockWeight;
	type DbWeight = ();
	type BlockExecutionWeight = ();
	type ExtrinsicBaseWeight = ();
	type MaximumExtrinsicWeight = ();
	type AvailableBlockRatio = AvailableBlockRatio;
	type MaximumBlockLength = MaximumBlockLength;
	type Version = ();
	type ModuleToIndex = ();
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
}

parameter_types! {
	pub const TestFinalityVotesCachingInterval: Option<u64> = Some(16);
	pub TestAuraConfiguration: AuraConfiguration = test_aura_config();
	pub TestValidatorsConfiguration: ValidatorsConfiguration = test_validators_config();
}

impl Trait for TestRuntime {
	type AuraConfiguration = TestAuraConfiguration;
	type ValidatorsConfiguration = TestValidatorsConfiguration;
	type FinalityVotesCachingInterval = TestFinalityVotesCachingInterval;
	type PruningStrategy = KeepSomeHeadersBehindBest;
	type OnHeadersSubmitted = ();
}

/// Step of genesis header.
pub const GENESIS_STEP: u64 = 42;

/// Aura configuration that is used in tests by default.
pub fn test_aura_config() -> AuraConfiguration {
	AuraConfiguration {
		empty_steps_transition: u64::max_value(),
		strict_empty_steps_transition: 0,
		validate_step_transition: 0x16e360,
		validate_score_transition: 0x41a3c4,
		two_thirds_majority_transition: u64::max_value(),
		min_gas_limit: 0x1388.into(),
		max_gas_limit: U256::max_value(),
		maximum_extra_data_size: 0x20,
	}
}

/// Validators configuration that is used in tests by default.
pub fn test_validators_config() -> ValidatorsConfiguration {
	ValidatorsConfiguration::Single(ValidatorsSource::List(validators_addresses(3)))
}

/// Genesis header that is used in tests by default.
pub fn genesis() -> Header {
	Header {
		seal: vec![vec![GENESIS_STEP as _].into(), vec![].into()],
		..Default::default()
	}
}

/// Build default i-th block, using data from runtime storage.
pub fn block_i(number: u64, validators: &[KeyPair]) -> Header {
	custom_block_i(number, validators, |_| {})
}

/// Build custom i-th block, using data from runtime storage.
pub fn custom_block_i(number: u64, validators: &[KeyPair], customize: impl FnOnce(&mut Header)) -> Header {
	let validator_index: u8 = (number % (validators.len() as u64)) as _;
	let mut header = Header {
		number,
		parent_hash: HeadersByNumber::get(number - 1).unwrap()[0].clone(),
		gas_limit: 0x2000.into(),
		author: validator(validator_index).address(),
		seal: vec![vec![(number + GENESIS_STEP) as u8].into(), vec![].into()],
		difficulty: number.into(),
		..Default::default()
	};
	customize(&mut header);
	signed_header(validators, header, number + GENESIS_STEP)
}

/// Build signed header from given header.
pub fn signed_header(validators: &[KeyPair], mut header: Header, step: u64) -> Header {
	let message = header.seal_hash(false).unwrap();
	let validator_index = (step % validators.len() as u64) as usize;
	let signature = sign(validators[validator_index].secret(), &message.as_fixed_bytes().into()).unwrap();
	let signature: [u8; 65] = signature.into();
	let signature = H520::from(signature);
	header.seal[1] = rlp_encode(&signature);
	header
}

/// Return key pair of given test validator.
pub fn validator(index: u8) -> KeyPair {
	KeyPair::from_secret(Secret::from([index + 1; 32])).unwrap()
}

/// Return key pairs of all test validators.
pub fn validators(count: u8) -> Vec<KeyPair> {
	(0..count).map(validator).collect()
}

/// Return addresses of all test validators.
pub fn validators_addresses(count: u8) -> Vec<Address> {
	(0..count).map(|i| validator(i).address()).collect()
}

/// Prepare externalities to start with custom initial header.
pub fn custom_test_ext(initial_header: Header, initial_validators: Vec<Address>) -> sp_io::TestExternalities {
	let t = GenesisConfig {
		initial_header,
		initial_difficulty: 0.into(),
		initial_validators,
	}
	.build_storage::<TestRuntime>()
	.unwrap();
	sp_io::TestExternalities::new(t)
}

/// Insert header into storage.
pub fn insert_header<S: Storage>(storage: &mut S, header: Header) {
	storage.insert_header(HeaderToImport {
		context: storage.import_context(None, &header.parent_hash).unwrap(),
		is_best: true,
		id: header.compute_id(),
		header,
		total_difficulty: 0.into(),
		enacted_change: None,
		scheduled_change: None,
		finality_votes: FinalityVotes::default(),
	});
}

/// Pruning strategy that keeps 10 headers behind best block.
pub struct KeepSomeHeadersBehindBest(pub u64);

impl Default for KeepSomeHeadersBehindBest {
	fn default() -> KeepSomeHeadersBehindBest {
		KeepSomeHeadersBehindBest(10)
	}
}

impl PruningStrategy for KeepSomeHeadersBehindBest {
	fn pruning_upper_bound(&mut self, best_number: u64, _: u64) -> u64 {
		best_number.checked_sub(self.0).unwrap_or(0)
	}
}
