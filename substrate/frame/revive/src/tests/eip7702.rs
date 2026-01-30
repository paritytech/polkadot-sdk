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

//! Tests for EIP-7702: Set EOA Account Code

use crate::{
	evm::fees::InfoT,
	storage::AccountInfo,
	test_utils::builder::Contract,
	tests::{builder, dummy_evm_contract, TestSigner, *},
	Code, Config,
};
use frame_support::{
	assert_ok,
	traits::fungible::{Balanced, Mutate},
	weights::WeightMeter,
};
use sp_core::{H160, H256, U256};

#[test]
fn set_delegation_creates_indicator() {
	ExtBuilder::default().build().execute_with(|| {
		let eoa = H160::from([0x11; 20]);
		let target = H160::from([0x22; 20]);

		assert_ok!(AccountInfo::<Test>::set_delegation(&eoa, target));

		assert!(AccountInfo::<Test>::is_delegated(&eoa));

		assert_eq!(AccountInfo::<Test>::get_delegation_target(&eoa), Some(target));
	});
}

#[test]
fn clear_delegation_restores_eoa() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x11; 20]);
		let target = H160::from([0x22; 20]);

		assert_ok!(AccountInfo::<Test>::set_delegation(&authority, target));
		assert!(AccountInfo::<Test>::is_delegated(&authority));

		assert_ok!(AccountInfo::<Test>::clear_delegation(&authority));

		assert!(!AccountInfo::<Test>::is_delegated(&authority));
	});
}

#[test]
fn delegation_can_be_updated() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x11; 20]);
		let target1 = H160::from([0x22; 20]);
		let target2 = H160::from([0x33; 20]);

		assert_ok!(AccountInfo::<Test>::set_delegation(&authority, target1));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target1));

		assert_ok!(AccountInfo::<Test>::set_delegation(&authority, target2));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target2));

		assert!(AccountInfo::<Test>::is_delegated(&authority));
	});
}

#[test]
fn regular_contract_is_not_delegation() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 1_000_000_000);
		let bytecode = dummy_evm_contract();

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(bytecode)).build_and_unwrap_contract();

		assert!(AccountInfo::<Test>::is_contract(&addr));
		assert!(!AccountInfo::<Test>::is_delegated(&addr));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&addr), None);
	});
}

#[test]
fn eip3607_allows_delegated_accounts_to_originate_transactions() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x11; 20]);
		let target = H160::from([0x22; 20]);

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		assert_ok!(AccountInfo::<Test>::set_delegation(&authority, target));

		let origin = RuntimeOrigin::signed(authority_id.clone());
		assert_ok!(Contracts::ensure_non_contract_if_signed(&origin));
	});
}

#[test]
fn eip3607_rejects_regular_contract_originating_transactions() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 1_000_000_000);
		let bytecode = dummy_evm_contract();

		let Contract { account_id, .. } =
			builder::bare_instantiate(Code::Upload(bytecode)).build_and_unwrap_contract();

		let origin = RuntimeOrigin::signed(account_id);
		assert!(Contracts::ensure_non_contract_if_signed(&origin).is_err());
	});
}

#[test]
fn multiple_delegations_last_one_wins() {
	ExtBuilder::default().build().execute_with(|| {
		let eoa = H160::from([0x11; 20]);
		let target1 = H160::from([0x22; 20]);
		let target2 = H160::from([0x33; 20]);
		let target3 = H160::from([0x44; 20]);

		assert_ok!(AccountInfo::<Test>::set_delegation(&eoa, target1));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&eoa), Some(target1));

		assert_ok!(AccountInfo::<Test>::set_delegation(&eoa, target2));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&eoa), Some(target2));

		assert_ok!(AccountInfo::<Test>::set_delegation(&eoa, target3));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&eoa), Some(target3));
	});
}

#[test]
fn valid_signature_is_verified_correctly() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));

		let auth = signer.sign_authorization(chain_id, target, nonce);

		assert_ok!(crate::evm::eip7702::process_authorizations::<Test>(
			&[auth],
			chain_id,
			&mut WeightMeter::new(),
		));

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target));
	});
}

#[test]
fn invalid_chain_id_rejects_authorization() {
	ExtBuilder::default().build().execute_with(|| {
		let correct_chain_id = U256::from(1);
		let wrong_chain_id = U256::from(999);
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));

		let auth = signer.sign_authorization(wrong_chain_id, target, nonce);

		// Authorization with wrong chain_id should be skipped (not error)
		assert_ok!(crate::evm::eip7702::process_authorizations::<Test>(
			&[auth],
			correct_chain_id,
			&mut WeightMeter::new(),
		));

		assert!(!AccountInfo::<Test>::is_delegated(&authority));
	});
}

#[test]
fn nonce_mismatch_rejects_authorization() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		let current_nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let wrong_nonce = current_nonce.saturating_add(U256::from(1));

		let auth = signer.sign_authorization(chain_id, target, wrong_nonce);

		// Authorization with wrong nonce should be skipped (not error)
		assert_ok!(crate::evm::eip7702::process_authorizations::<Test>(
			&[auth],
			chain_id,
			&mut WeightMeter::new(),
		));

		assert!(!AccountInfo::<Test>::is_delegated(&signer.address));
	});
}

#[test]
fn multiple_authorizations_from_same_authority_first_wins() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target1 = H160::from([0x11; 20]);
		let target2 = H160::from([0x22; 20]);
		let target3 = H160::from([0x33; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));

		// All have the same nonce, but only the first will succeed
		// (subsequent ones will fail due to nonce mismatch after first increments it)
		let auth1 = signer.sign_authorization(chain_id, target1, nonce);
		let auth2 = signer.sign_authorization(chain_id, target2, nonce);
		let auth3 = signer.sign_authorization(chain_id, target3, nonce);

		assert_ok!(crate::evm::eip7702::process_authorizations::<Test>(
			&[auth1, auth2, auth3],
			chain_id,
			&mut WeightMeter::new(),
		));

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		// First authorization wins since we process blindly
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target1));
	});
}

#[test]
fn authorization_increments_nonce() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		let nonce_before = frame_system::Pallet::<Test>::account_nonce(&authority_id);

		let auth = signer.sign_authorization(chain_id, target, U256::from(nonce_before));

		assert_ok!(crate::evm::eip7702::process_authorizations::<Test>(
			&[auth],
			chain_id,
			&mut WeightMeter::new(),
		));

		let nonce_after = frame_system::Pallet::<Test>::account_nonce(&authority_id);
		assert_eq!(nonce_after, nonce_before + 1);
	});
}

#[test]
fn chain_id_zero_accepts_any_chain() {
	ExtBuilder::default().build().execute_with(|| {
		let current_chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));

		let auth = signer.sign_authorization(U256::zero(), target, nonce);

		assert_ok!(crate::evm::eip7702::process_authorizations::<Test>(
			&[auth],
			current_chain_id,
			&mut WeightMeter::new(),
		));

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target));
	});
}

#[test]
fn new_account_sets_delegation() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));

		let auth = signer.sign_authorization(chain_id, target, nonce);

		assert_ok!(crate::evm::eip7702::process_authorizations::<Test>(
			&[auth],
			chain_id,
			&mut WeightMeter::new(),
		));

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target));
	});
}

#[test]
fn clearing_delegation_with_zero_address() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));

		let auth1 = signer.sign_authorization(chain_id, target, nonce);
		assert_ok!(crate::evm::eip7702::process_authorizations::<Test>(
			&[auth1],
			chain_id,
			&mut WeightMeter::new(),
		));

		assert!(AccountInfo::<Test>::is_delegated(&authority));

		let new_nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));

		let auth2 = signer.sign_authorization(chain_id, H160::zero(), new_nonce);
		assert_ok!(crate::evm::eip7702::process_authorizations::<Test>(
			&[auth2],
			chain_id,
			&mut WeightMeter::new(),
		));

		assert!(!AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), None);
	});
}

#[test]
fn process_multiple_authorizations_from_different_signers() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		let seed1 = H256::from([1u8; 32]);
		let seed2 = H256::from([2u8; 32]);
		let seed3 = H256::from([3u8; 32]);

		let signer1 = TestSigner::new(&seed1.0);
		let signer2 = TestSigner::new(&seed2.0);
		let signer3 = TestSigner::new(&seed3.0);

		let authority1 = signer1.address;
		let authority2 = signer2.address;
		let authority3 = signer3.address;

		let authority1_id = <Test as Config>::AddressMapper::to_account_id(&authority1);
		let authority2_id = <Test as Config>::AddressMapper::to_account_id(&authority2);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority1_id, 1_000_000);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority2_id, 1_000_000);

		let nonce1 = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority1_id));
		let nonce2 = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority2_id));
		let nonce3 = U256::zero();

		let auth1 = signer1.sign_authorization(chain_id, target, nonce1);
		let auth2 = signer2.sign_authorization(chain_id, target, nonce2);
		let auth3 = signer3.sign_authorization(chain_id, target, nonce3);

		assert_ok!(crate::evm::eip7702::process_authorizations::<Test>(
			&[auth1, auth2, auth3],
			chain_id,
			&mut WeightMeter::new(),
		));

		assert!(AccountInfo::<Test>::is_delegated(&authority1));
		assert!(AccountInfo::<Test>::is_delegated(&authority2));
		assert!(AccountInfo::<Test>::is_delegated(&authority3));
	});
}

/// Runtime test: Set authorization via eth_call
///
/// This test verifies that an EOA can successfully delegate to a contract
/// by creating an EIP-7702 transaction and dispatching it through eth_call.
///
/// Test flow:
/// 1. Create a test signer (EOA) and a target contract
/// 2. Fund the EOA and initialize it in storage
/// 3. Sign an authorization tuple delegating to the target contract
/// 4. Create an eth_call with the authorization list
/// 5. Dispatch the call and verify delegation is set
#[test]
fn test_runtime_set_authorization() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());

		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let target_contract = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));

		let auth = signer.sign_authorization(chain_id, target_contract.addr, nonce);

		let result = builder::eth_call(target_contract.addr)
			.authorization_list(vec![auth])
			.eth_gas_limit(1_000_000u64.into())
			.build();

		assert_ok!(result);

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&authority),
			Some(target_contract.addr)
		);

		let new_nonce = frame_system::Pallet::<Test>::account_nonce(&authority_id);
		assert_eq!(new_nonce, 1);
	});
}

/// Runtime test: Clear authorization via eth_call
///
/// This test verifies that an EOA can clear its delegation by setting
/// the authorization address to 0x0 (zero address).
///
/// Test flow:
/// 1. Set up an EOA with delegation to a contract (same as test above)
/// 2. Create a new authorization with address = 0x0
/// 3. Dispatch eth_call with the clearing authorization
/// 4. Verify delegation is cleared and account is back to EOA state
#[test]
fn test_runtime_clear_authorization() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());

		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let target_contract = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));

		let auth1 = signer.sign_authorization(chain_id, target_contract.addr, nonce);
		let result1 = builder::eth_call(target_contract.addr)
			.authorization_list(vec![auth1])
			.eth_gas_limit(1_000_000u64.into())
			.build();
		assert_ok!(result1);

		assert!(AccountInfo::<Test>::is_delegated(&authority));

		let new_nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));

		let auth2 = signer.sign_authorization(chain_id, H160::zero(), new_nonce);
		let result2 = builder::eth_call(target_contract.addr)
			.authorization_list(vec![auth2])
			.eth_gas_limit(1_000_000u64.into())
			.build();
		assert_ok!(result2);

		assert!(!AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), None);

		assert!(!AccountInfo::<Test>::is_contract(&authority));
	});
}

/// Runtime test: Delegation authorization can be set via eth_call
///
/// This test verifies that an EOA can be set up with delegation to a target
/// contract, and that subsequent calls to the delegated EOA succeed through
/// the EVM execution path.
///
/// Test flow:
/// 1. Create an EOA and a simple target contract
/// 2. Set delegation from EOA to target contract via authorization list
/// 3. Verify the delegation indicator is stored correctly
/// 4. Call the delegated EOA address using eth_call
/// 5. Verify the call succeeds (delegation is recognized in EVM context)
///
/// Note: This test validates the authorization processing and storage of
/// delegation indicators. Full execution semantics of delegated code are
/// handled by the VM layer during actual contract execution.
#[test]
fn test_runtime_delegation_resolution() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());

		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let target_contract = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));

		let auth = signer.sign_authorization(chain_id, target_contract.addr, nonce);
		let result = builder::eth_call(target_contract.addr)
			.authorization_list(vec![auth])
			.eth_gas_limit(1_000_000u64.into())
			.build();
		assert_ok!(result);

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&authority),
			Some(target_contract.addr)
		);

		let call_result = builder::eth_call(authority).eth_gas_limit(1_000_000u64.into()).build();

		assert_ok!(&call_result);
	});
}
