// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
	extension::{AsPerson, AsPersonInfo},
	*,
};
use frame_support::{
	assert_ok, derive_impl, dispatch::DispatchErrorWithPostInfo, match_types, parameter_types,
	storage::with_transaction, weights::RuntimeDbWeight,
};

use frame_system::{offchain::CreateTransactionBase, ChainContext};
use sp_core::{ConstU16, ConstU32, ConstU64, H256};
use sp_runtime::{
	testing::UintAuthorityId,
	traits::{Applyable, BlakeTwo256, Checkable, IdentityLookup},
	transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidityError},
	BuildStorage, DispatchError, Weight,
};
use verifiable::demo_impls::Simple;

// First ring, used in testing.
pub const RI_ZERO: RingIndex = 0;

const EXTENSION_VERSION: u8 = 0;
pub type TransactionExtension = (AsPerson<Test>, frame_system::CheckNonce<Test>);
pub type Header = sp_runtime::generic::Header<u64, sp_runtime::traits::BlakeTwo256>;
pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<
	u64,
	RuntimeCall,
	sp_runtime::testing::UintAuthorityId,
	TransactionExtension,
>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		PeoplePallet: crate,
	}
);

parameter_types! {
	pub const MockDbWeight: RuntimeDbWeight = RuntimeDbWeight {
		read: 10,
		write: 20,
	};
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = MockDbWeight;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ConstU16<42>;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

pub type Extrinsic = sp_runtime::testing::TestXt<RuntimeCall, ()>;

impl CreateTransactionBase<Call<Self>> for Test {
	type Extrinsic = Extrinsic;
	type RuntimeCall = RuntimeCall;
}

parameter_types! {
	pub static MaxRingSize: u32 = 10;
}

pub const MOCK_CONTEXT: Context = *b"pop:polkadot.network/mock       ";
match_types! {
	pub type TestAccountContexts: impl Contains<Context> = {
		&MOCK_CONTEXT
	};
}

pub struct MockWeights;
impl crate::WeightInfo for MockWeights {
	fn under_alias() -> sp_runtime::Weight {
		Weight::from_parts(3, 3)
	}

	fn set_alias_account() -> sp_runtime::Weight {
		Weight::from_parts(4, 4)
	}

	fn unset_alias_account() -> sp_runtime::Weight {
		Weight::from_parts(5, 5)
	}

	fn reset_root() -> sp_runtime::Weight {
		Weight::from_parts(6, 6)
	}

	fn force_recognize_personhood() -> sp_runtime::Weight {
		Weight::from_parts(7, 7)
	}

	fn set_personal_id_account() -> sp_runtime::Weight {
		Weight::from_parts(8, 8)
	}

	fn unset_personal_id_account() -> sp_runtime::Weight {
		Weight::from_parts(9, 9)
	}

	fn set_onboarding_size() -> sp_runtime::Weight {
		Weight::from_parts(10, 10)
	}

	fn merge_rings() -> sp_runtime::Weight {
		Weight::from_parts(11, 11)
	}

	fn migrate_included_key() -> sp_runtime::Weight {
		Weight::from_parts(12, 12)
	}

	fn migrate_onboarding_key() -> sp_runtime::Weight {
		Weight::from_parts(13, 13)
	}

	fn should_build_ring(n: u32) -> sp_runtime::Weight {
		Weight::from_parts(n as u64 * 14, n as u64 * 14)
	}

	fn build_ring(n: u32) -> sp_runtime::Weight {
		Weight::from_parts(n as u64 * 14, n as u64 * 14)
	}

	fn onboard_people() -> sp_runtime::Weight {
		Weight::from_parts(15, 15)
	}

	fn remove_suspended_people(n: u32) -> sp_runtime::Weight {
		Weight::from_parts(n as u64 * 16, n as u64 * 16)
	}

	fn pending_suspensions_iteration() -> Weight {
		Weight::from_parts(1, 1)
	}

	fn migrate_keys_single_included_key() -> sp_runtime::Weight {
		Weight::from_parts(17, 17)
	}

	fn merge_queue_pages() -> sp_runtime::Weight {
		Weight::from_parts(18, 18)
	}

	fn on_poll_base() -> sp_runtime::Weight {
		Weight::from_parts(19, 19)
	}

	fn on_idle_base() -> sp_runtime::Weight {
		Weight::from_parts(20, 20)
	}

	fn as_person_alias_with_account() -> Weight {
		Weight::from_parts(20, 20)
	}

	fn as_person_identity_with_account() -> Weight {
		Weight::from_parts(21, 21)
	}

	fn as_person_alias_with_proof() -> Weight {
		Weight::from_parts(22, 22)
	}

	fn as_person_identity_with_proof() -> Weight {
		Weight::from_parts(23, 23)
	}
}

impl crate::Config for Test {
	type WeightInfo = MockWeights;
	type RuntimeEvent = RuntimeEvent;
	type Crypto = verifiable::demo_impls::Simple;
	type AccountContexts = TestAccountContexts;
	type ChunkPageSize = ConstU32<5>;
	type MaxRingSize = MaxRingSize;
	type OnboardingQueuePageSize = ConstU32<40>;

	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = BenchHelper;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct BenchHelper {}

#[cfg(feature = "runtime-benchmarks")]
impl<Chunk> BenchmarkHelper<Chunk> for BenchHelper
where
	Chunk: From<<verifiable::demo_impls::Simple as verifiable::GenerateVerifiable>::StaticChunk>,
{
	fn valid_account_context() -> Context {
		MOCK_CONTEXT
	}
	fn initialize_chunks() -> Vec<Chunk> {
		vec![]
	}
}

#[allow(dead_code)]
pub fn advance_to(b: u64) {
	while System::block_number() < b {
		System::set_block_number(System::block_number() + 1);
	}
}

pub struct ConfigRecord;

pub fn new_config() -> ConfigRecord {
	ConfigRecord
}

pub struct TestExt(ConfigRecord);
#[allow(dead_code)]
impl TestExt {
	pub(crate) fn max_ring_size(self, size: u32) -> Self {
		MaxRingSize::set(size);
		self
	}

	pub fn new() -> Self {
		Self(new_config())
	}

	pub fn execute_with<R>(self, f: impl Fn() -> R) -> R {
		new_test_ext().execute_with(f)
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let chunks: Vec<<verifiable::demo_impls::Simple as GenerateVerifiable>::StaticChunk> =
		[(); 512].to_vec();
	let encoded_chunks = chunks.encode();

	RuntimeGenesisConfig {
		system: Default::default(),
		people_pallet: crate::GenesisConfig::<Test> {
			encoded_chunks: encoded_chunks.clone(),
			..Default::default()
		},
	}
	.build_storage()
	.unwrap()
	.into()
}

/// We gather both error into a single type in order to do `assert_ok` and `assert_err` safely.
/// Otherwise, we can easily miss the inner error in a `Resut<Resut<_, _>, _>`.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum TransactionExecutionError {
	Validity(TransactionValidityError),
	// This ignores the post info.
	Dispatch(DispatchErrorWithPostInfo),
}

impl From<DispatchErrorWithPostInfo> for TransactionExecutionError {
	fn from(e: DispatchErrorWithPostInfo) -> Self {
		TransactionExecutionError::Dispatch(e)
	}
}

impl From<TransactionValidityError> for TransactionExecutionError {
	fn from(e: TransactionValidityError) -> Self {
		TransactionExecutionError::Validity(e)
	}
}

impl From<DispatchError> for TransactionExecutionError {
	fn from(e: DispatchError) -> Self {
		TransactionExecutionError::Dispatch(e.into())
	}
}

impl From<InvalidTransaction> for TransactionExecutionError {
	fn from(e: InvalidTransaction) -> Self {
		TransactionExecutionError::Validity(e.into())
	}
}

/// Execute a transaction with the given origin, call and transaction extension.
pub fn exec_tx(
	who: Option<u64>,
	tx_ext: TransactionExtension,
	call: impl Into<RuntimeCall>,
) -> Result<(), TransactionExecutionError> {
	let tx = match who {
		Some(who) => UncheckedExtrinsic::new_signed(call.into(), who, UintAuthorityId(who), tx_ext),
		None => UncheckedExtrinsic::new_transaction(call.into(), tx_ext),
	};

	let info = tx.get_dispatch_info();
	let len = tx.encoded_size();

	// Check and validate the extrinsic.
	let checked = Checkable::check(tx, &ChainContext::<Test>::default())?;
	with_transaction(|| {
		let valid = checked.validate::<Test>(TransactionSource::External, &info, len);
		sp_runtime::TransactionOutcome::Rollback(Result::<_, DispatchError>::Ok(valid))
	})
	.unwrap()?;
	// Finally, apply the extrinsic.
	checked.apply::<Test>(&info, len)??;

	Ok(())
}

pub fn exec_as_alias_tx(
	who: u64,
	call: impl Into<RuntimeCall>,
) -> Result<(), TransactionExecutionError> {
	let nonce = frame_system::Account::<Test>::get(who).nonce;
	let tx_ext = (
		AsPerson::new(Some(AsPersonInfo::AsPersonalAliasWithAccount(nonce))),
		frame_system::CheckNonce::from(nonce),
	);

	exec_tx(Some(who), tx_ext, call)
}

/// Call `set_alias_account` for the given personal id and account.
pub fn setup_alias_account(
	key: &<Simple as GenerateVerifiable>::Member,
	secret: &<Simple as GenerateVerifiable>::Secret,
	context: Context,
	account: u64,
) {
	let id = crate::Keys::<Test>::get(key).expect("id not found");
	let record = crate::People::<Test>::get(id).expect("record not found");
	let ring_index = record.position.ring_index().expect("person not included in a ring");
	let commitment = {
		let all_keys = crate::RingKeys::<Test>::get(ring_index);
		Simple::open(key, all_keys.into_iter()).unwrap()
	};
	let call = RuntimeCall::PeoplePallet(crate::Call::set_alias_account {
		account,
		call_valid_at: frame_system::Pallet::<Test>::block_number(),
	});
	let other_tx_ext = (frame_system::CheckNonce::<Test>::from(0),);
	// Here we simply ignore implicit as they are null.
	let msg = (&EXTENSION_VERSION, &call, &other_tx_ext).using_encoded(sp_io::hashing::blake2_256);
	let (proof, _alias) =
		Simple::create(commitment, secret, &context, &msg).expect("proof creation failed");
	let tx_ext = (
		AsPerson::<Test>::new(Some(AsPersonInfo::AsPersonalAliasWithProof(
			proof, ring_index, context,
		))),
		other_tx_ext.0,
	);
	assert_ok!(exec_tx(None, tx_ext.clone(), call.clone()));
}
