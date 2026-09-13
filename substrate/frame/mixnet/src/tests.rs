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

#![cfg(test)]

use super::*;
use frame::{
	deps::{
		frame_support::traits::Authorize,
		sp_core::{sr25519, Pair},
		sp_runtime::{
			self,
			testing::TestXt,
			transaction_validity::{
				InvalidTransaction, TransactionSource, TransactionValidityError,
			},
		},
	},
	testing_prelude::*,
};
use sp_mixnet::types::{AuthorityId, AuthoritySignature};

type TxExtension = frame_system::AuthorizeCall<Test>;
type Extrinsic = TestXt<RuntimeCall, TxExtension>;
type Block = sp_runtime::generic::Block<
	sp_runtime::generic::Header<u64, sp_runtime::traits::BlakeTwo256>,
	Extrinsic,
>;

construct_runtime!(
	pub enum Test {
		System: frame_system,
		Mixnet: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl frame_system::offchain::SigningTypes for Test {
	type Public = sp_runtime::testing::UintAuthorityId;
	type Signature = sp_runtime::testing::TestSignature;
}

impl<LocalCall> frame_system::offchain::CreateTransactionBase<LocalCall> for Test
where
	RuntimeCall: From<LocalCall>,
{
	type RuntimeCall = RuntimeCall;
	type Extrinsic = Extrinsic;
}

impl<LocalCall> frame_system::offchain::CreateTransaction<LocalCall> for Test
where
	RuntimeCall: From<LocalCall>,
{
	type Extension = TxExtension;
	fn create_transaction(call: RuntimeCall, extension: Self::Extension) -> Extrinsic {
		Extrinsic::new_transaction(call, extension)
	}
}

impl<LocalCall> frame_system::offchain::CreateAuthorizedTransaction<LocalCall> for Test
where
	RuntimeCall: From<LocalCall>,
{
	fn create_extension() -> Self::Extension {
		TxExtension::new()
	}
}

parameter_types! {
	pub const RegistrationPriority: TransactionPriority = 1 << 20;
}

impl Config for Test {
	type WeightInfo = ();
	type MaxAuthorities = ConstU32<10>;
	type MaxExternalAddressSize = ConstU32<64>;
	type MaxExternalAddressesPerMixnode = ConstU32<4>;
	type NextSessionRotation = ();
	type NumCoverToCurrentBlocks = ConstU64<3>;
	type NumRequestsToCurrentBlocks = ConstU64<3>;
	type NumCoverToPrevBlocks = ConstU64<3>;
	type NumRegisterStartSlackBlocks = ConstU64<3>;
	type NumRegisterEndSlackBlocks = ConstU64<3>;
	type RegistrationPriority = RegistrationPriority;
	type MinMixnodes = ConstU32<1>;
}

pub(crate) fn new_test_ext() -> TestState {
	use sp_keystore::{testing::MemoryKeystore, KeystoreExt};

	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = TestState::new(t);
	ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
	ext
}

fn generate_authority() -> (sr25519::Pair, AuthorityId) {
	let (pair, _) = sr25519::Pair::generate();
	let authority_id = AuthorityId::from(pair.public());
	(pair, authority_id)
}

fn test_mixnode() -> BoundedMixnodeFor<Test> {
	BoundedMixnode {
		kx_public: [1u8; 32],
		peer_id: [2u8; 32],
		external_addresses: Default::default(),
	}
}

fn create_registration(
	session_index: u32,
	authority_index: AuthorityIndex,
) -> RegistrationFor<Test> {
	Registration { block_number: 1u64, session_index, authority_index, mixnode: test_mixnode() }
}

fn sign_registration(
	pair: &sr25519::Pair,
	registration: &RegistrationFor<Test>,
) -> AuthoritySignature {
	AuthoritySignature::from(pair.sign(&registration.encode()))
}

fn setup_authority(authority_index: AuthorityIndex) -> (sr25519::Pair, AuthorityId) {
	let (pair, authority_id) = generate_authority();
	pallet::NextAuthorityIds::<Test>::insert(authority_index, authority_id.clone());
	(pair, authority_id)
}

fn make_register_call(
	registration: RegistrationFor<Test>,
	signature: AuthoritySignature,
) -> RuntimeCall {
	RuntimeCall::Mixnet(pallet::Call::register { registration, signature })
}

#[test]
fn authorize_accepts_valid_registration() {
	new_test_ext().execute_with(|| {
		pallet::CurrentSessionIndex::<Test>::put(0u32);
		let (pair, _) = setup_authority(0);

		let registration = create_registration(0, 0);
		let signature = sign_registration(&pair, &registration);
		let call = make_register_call(registration, signature);

		let result = call.authorize(TransactionSource::External);
		assert!(result.is_some(), "Call should have authorize logic");
		assert!(result.unwrap().is_ok(), "Valid registration should be accepted");
	});
}

#[test]
fn authorize_rejects_bad_signature() {
	new_test_ext().execute_with(|| {
		pallet::CurrentSessionIndex::<Test>::put(0u32);
		let (_pair, _) = setup_authority(0);

		let registration = create_registration(0, 0);
		// Sign with a different key
		let (wrong_pair, _) = generate_authority();
		let signature = sign_registration(&wrong_pair, &registration);
		let call = make_register_call(registration, signature);

		let result = call.authorize(TransactionSource::External);
		assert!(result.is_some());
		assert_eq!(
			result.unwrap(),
			Err(TransactionValidityError::Invalid(InvalidTransaction::BadProof))
		);
	});
}

#[test]
fn register_dispatches_with_authorized_origin() {
	new_test_ext().execute_with(|| {
		pallet::CurrentSessionIndex::<Test>::put(0u32);
		let (pair, _) = setup_authority(0);

		let registration = create_registration(0, 0);
		let signature = sign_registration(&pair, &registration);

		assert_ok!(Mixnet::register(
			RuntimeOrigin::from(frame_system::RawOrigin::Authorized),
			registration,
			signature,
		));

		// Verify the mixnode was stored for the next session
		assert!(pallet::Mixnodes::<Test>::contains_key(1u32, 0u32));
	});
}
