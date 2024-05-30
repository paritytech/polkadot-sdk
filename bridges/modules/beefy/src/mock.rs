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

use crate as beefy;
use crate::{
	utils::get_authorities_mmr_root, BridgedBeefyAuthoritySet, BridgedBeefyAuthoritySetInfo,
	BridgedBeefyCommitmentHasher, BridgedBeefyMmrLeafExtra, BridgedBeefySignedCommitment,
	BridgedMmrHash, BridgedMmrHashing, BridgedMmrProof,
};

use bp_beefy::{BeefyValidatorSignatureOf, ChainWithBeefy, Commitment, MmrDataOrHash};
use bp_runtime::{BasicOperatingMode, Chain, ChainId};
use codec::Encode;
use frame_support::{construct_runtime, derive_impl, weights::Weight};
use sp_core::{sr25519::Signature, Pair};
use sp_runtime::{
	testing::{Header, H256},
	traits::{BlakeTwo256, Hash},
};

pub use sp_consensus_beefy::ecdsa_crypto::{AuthorityId as BeefyId, Pair as BeefyPair};
use sp_core::crypto::Wraps;
use sp_runtime::traits::Keccak256;

pub type TestAccountId = u64;
pub type TestBridgedBlockNumber = u64;
pub type TestBridgedBlockHash = H256;
pub type TestBridgedHeader = Header;
pub type TestBridgedAuthoritySetInfo = BridgedBeefyAuthoritySetInfo<TestRuntime, ()>;
pub type TestBridgedValidatorSet = BridgedBeefyAuthoritySet<TestRuntime, ()>;
pub type TestBridgedCommitment = BridgedBeefySignedCommitment<TestRuntime, ()>;
pub type TestBridgedValidatorSignature = BeefyValidatorSignatureOf<TestBridgedChain>;
pub type TestBridgedCommitmentHasher = BridgedBeefyCommitmentHasher<TestRuntime, ()>;
pub type TestBridgedMmrHashing = BridgedMmrHashing<TestRuntime, ()>;
pub type TestBridgedMmrHash = BridgedMmrHash<TestRuntime, ()>;
pub type TestBridgedBeefyMmrLeafExtra = BridgedBeefyMmrLeafExtra<TestRuntime, ()>;
pub type TestBridgedMmrProof = BridgedMmrProof<TestRuntime, ()>;
pub type TestBridgedRawMmrLeaf = sp_consensus_beefy::mmr::MmrLeaf<
	TestBridgedBlockNumber,
	TestBridgedBlockHash,
	TestBridgedMmrHash,
	TestBridgedBeefyMmrLeafExtra,
>;
pub type TestBridgedMmrNode = MmrDataOrHash<Keccak256, TestBridgedRawMmrLeaf>;

type Block = frame_system::mocking::MockBlock<TestRuntime>;

construct_runtime! {
	pub enum TestRuntime
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Beefy: beefy::{Pallet},
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Block = Block;
}

impl beefy::Config for TestRuntime {
	type MaxRequests = frame_support::traits::ConstU32<16>;
	type BridgedChain = TestBridgedChain;
	type CommitmentsToKeep = frame_support::traits::ConstU32<16>;
}

#[derive(Debug)]
pub struct TestBridgedChain;

impl Chain for TestBridgedChain {
	const ID: ChainId = *b"tbch";

	type BlockNumber = TestBridgedBlockNumber;
	type Hash = H256;
	type Hasher = BlakeTwo256;
	type Header = sp_runtime::testing::Header;

	type AccountId = TestAccountId;
	type Balance = u64;
	type Nonce = u64;
	type Signature = Signature;

	fn max_extrinsic_size() -> u32 {
		unreachable!()
	}
	fn max_extrinsic_weight() -> Weight {
		unreachable!()
	}
}

impl ChainWithBeefy for TestBridgedChain {
	type CommitmentHasher = Keccak256;
	type MmrHashing = Keccak256;
	type MmrHash = <Keccak256 as Hash>::Output;
	type BeefyMmrLeafExtra = ();
	type AuthorityId = BeefyId;
	type AuthorityIdToMerkleLeaf = pallet_beefy_mmr::BeefyEcdsaToEthereum;
}

/// Run test within test runtime.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	sp_io::TestExternalities::new(Default::default()).execute_with(test)
}

/// Initialize pallet and run test.
pub fn run_test_with_initialize<T>(initial_validators_count: u32, test: impl FnOnce() -> T) -> T {
	run_test(|| {
		let validators = validator_ids(0, initial_validators_count);
		let authority_set = authority_set_info(0, &validators);

		crate::Pallet::<TestRuntime>::initialize(
			RuntimeOrigin::root(),
			bp_beefy::InitializationData {
				operating_mode: BasicOperatingMode::Normal,
				best_block_number: 0,
				authority_set,
			},
		)
		.expect("initialization data is correct");

		test()
	})
}

/// Import given commitment.
pub fn import_commitment(
	header: crate::mock_chain::HeaderAndCommitment,
) -> sp_runtime::DispatchResult {
	crate::Pallet::<TestRuntime>::submit_commitment(
		RuntimeOrigin::signed(1),
		header
			.commitment
			.expect("thou shall not call import_commitment on header without commitment"),
		header.validator_set,
		Box::new(header.leaf),
		header.leaf_proof,
	)
}

pub fn validator_pairs(index: u32, count: u32) -> Vec<BeefyPair> {
	(index..index + count)
		.map(|index| {
			let mut seed = [1u8; 32];
			seed[0..8].copy_from_slice(&(index as u64).encode());
			BeefyPair::from_seed(&seed)
		})
		.collect()
}

/// Return identifiers of validators, starting at given index.
pub fn validator_ids(index: u32, count: u32) -> Vec<BeefyId> {
	validator_pairs(index, count).into_iter().map(|pair| pair.public()).collect()
}

pub fn authority_set_info(id: u64, validators: &[BeefyId]) -> TestBridgedAuthoritySetInfo {
	let merkle_root = get_authorities_mmr_root::<TestRuntime, (), _>(validators.iter());

	TestBridgedAuthoritySetInfo { id, len: validators.len() as u32, keyset_commitment: merkle_root }
}

/// Sign BEEFY commitment.
pub fn sign_commitment(
	commitment: Commitment<TestBridgedBlockNumber>,
	validator_pairs: &[BeefyPair],
	signature_count: usize,
) -> TestBridgedCommitment {
	let total_validators = validator_pairs.len();
	let random_validators =
		rand::seq::index::sample(&mut rand::thread_rng(), total_validators, signature_count);

	let commitment_hash = TestBridgedCommitmentHasher::hash(&commitment.encode());
	let mut signatures = vec![None; total_validators];
	for validator_idx in random_validators.iter() {
		let validator = &validator_pairs[validator_idx];
		signatures[validator_idx] =
			Some(validator.as_inner_ref().sign_prehashed(commitment_hash.as_fixed_bytes()).into());
	}

	TestBridgedCommitment { commitment, signatures }
}
