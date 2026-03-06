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
	evm::{eip7702::AuthorizationResult, fees::InfoT, AuthorizationListEntry},
	storage::AccountInfo,
	test_utils::builder::Contract,
	tests::{builder, test_utils::*, TestSigner, *},
	Code, CodeInfoOf, Config, ExecConfig, HoldReason,
};
use frame_support::{
	assert_ok,
	traits::fungible::{Balanced, Inspect, Mutate},
	weights::Weight,
};
use sp_core::{H160, H256, U256};

/// Common setup for delegation tests that call `process_authorizations` directly.
struct DelegationTestSetup {
	signer: TestSigner,
	authority_id: AccountId32,
	origin: AccountId32,
	exec_config: ExecConfig<Test>,
	chain_id: U256,
}

impl DelegationTestSetup {
	fn new(seed: [u8; 32]) -> Self {
		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let signer = TestSigner::new(&seed);
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&signer.address);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));
		let origin = <Test as Config>::AddressMapper::to_account_id(&H160::from([0xFF; 20]));
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&origin, 10_000_000_000);
		let exec_config = ExecConfig::new_eth_tx(U256::from(1), 0, Weight::MAX);
		Self { signer, authority_id, origin, exec_config, chain_id }
	}

	fn process(
		&self,
		auths: &[AuthorizationListEntry],
	) -> AuthorizationResult<crate::BalanceOf<Test>> {
		crate::evm::eip7702::process_authorizations::<Test>(auths, &self.origin, &self.exec_config)
			.expect("process_authorizations failed")
	}

	fn nonce(&self) -> U256 {
		U256::from(frame_system::Pallet::<Test>::account_nonce(&self.authority_id))
	}
}

/// Helper to call process_authorizations with a funded origin and exec_config.
fn test_process_authorizations(
	auths: &[AuthorizationListEntry],
) -> AuthorizationResult<crate::BalanceOf<Test>> {
	let origin = <Test as Config>::AddressMapper::to_account_id(&H160::from([0xFF; 20]));
	let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&origin, 10_000_000_000);
	<Test as Config>::FeeInfo::deposit_txfee(<<Test as Config>::Currency as Balanced<_>>::issue(
		10_000_000_000,
	));
	let exec_config = ExecConfig::new_eth_tx(U256::from(1), 0, Weight::MAX);
	crate::evm::eip7702::process_authorizations::<Test>(auths, &origin, &exec_config)
		.expect("process_authorizations failed")
}

#[test]
fn clear_delegation_restores_eoa() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x11; 20]);
		let target = H160::from([0x22; 20]);

		AccountInfo::<Test>::set_delegation(&authority, target);
		assert!(AccountInfo::<Test>::is_delegated(&authority));

		AccountInfo::<Test>::clear_delegation(&authority);
		assert!(!AccountInfo::<Test>::is_delegated(&authority));
	});
}

#[test]
fn delegation_can_be_updated() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x11; 20]);
		let target1 = H160::from([0x22; 20]);
		let target2 = H160::from([0x33; 20]);

		AccountInfo::<Test>::set_delegation(&authority, target1);
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target1));

		AccountInfo::<Test>::set_delegation(&authority, target2);
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

		AccountInfo::<Test>::set_delegation(&authority, target);

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
fn valid_signature_is_verified_correctly() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);

		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth = signer.sign_authorization(chain_id, target, nonce);

		assert_eq!(
			test_process_authorizations(&[auth]),
			AuthorizationResult { existing_accounts: 1, new_accounts: 0, deposit: 0, weight_refund: Weight::zero() },
		);

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target));
	});
}

#[test]
fn invalid_chain_id_rejects_authorization() {
	ExtBuilder::default().build().execute_with(|| {
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
		assert_eq!(
			test_process_authorizations(&[auth]),
			AuthorizationResult { existing_accounts: 0, new_accounts: 0, deposit: 0, weight_refund: Weight::zero() }
		);

		assert!(!AccountInfo::<Test>::is_delegated(&authority));
	});
}

#[test]
fn nonce_mismatch_rejects_authorization() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());
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
		assert_eq!(
			test_process_authorizations(&[auth]),
			AuthorizationResult { existing_accounts: 0, new_accounts: 0, deposit: 0, weight_refund: Weight::zero() }
		);

		assert!(!AccountInfo::<Test>::is_delegated(&signer.address));
	});
}

#[test]
fn multiple_authorizations_from_same_authority_first_wins() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());
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

		assert_eq!(
			test_process_authorizations(&[auth1, auth2, auth3]),
			AuthorizationResult { existing_accounts: 1, new_accounts: 0, deposit: 0, weight_refund: Weight::zero() },
		);

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		// First authorization wins since we process blindly
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target1));
	});
}

#[test]
fn authorization_increments_nonce() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		let nonce_before = frame_system::Pallet::<Test>::account_nonce(&authority_id);
		let auth = signer.sign_authorization(chain_id, target, U256::from(nonce_before));

		assert_eq!(
			test_process_authorizations(&[auth]),
			AuthorizationResult { existing_accounts: 1, new_accounts: 0, deposit: 0, weight_refund: Weight::zero() },
		);

		let nonce_after = frame_system::Pallet::<Test>::account_nonce(&authority_id);
		assert_eq!(nonce_after, nonce_before + 1);
	});
}

#[test]
fn chain_id_zero_accepts_any_chain() {
	ExtBuilder::default().build().execute_with(|| {
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth = signer.sign_authorization(U256::zero(), target, nonce);

		assert_eq!(
			test_process_authorizations(&[auth]),
			AuthorizationResult { existing_accounts: 1, new_accounts: 0, deposit: 0, weight_refund: Weight::zero() },
		);

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target));
	});
}

#[test]
fn new_account_sets_delegation() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth = signer.sign_authorization(chain_id, target, nonce);

		assert_eq!(
			test_process_authorizations(&[auth]),
			AuthorizationResult { existing_accounts: 0, new_accounts: 1, deposit: 1, weight_refund: Weight::zero() },
		);

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target));
		let balance = <<Test as Config>::Currency as Inspect<_>>::balance(&authority_id);
		assert!(balance >= Pallet::<Test>::min_balance());
	});
}

#[test]
fn clearing_delegation_with_zero_address() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let target = H160::from([0x42; 20]);

		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth1 = signer.sign_authorization(chain_id, target, nonce);

		assert_eq!(
			test_process_authorizations(&[auth1]),
			AuthorizationResult { existing_accounts: 1, new_accounts: 0, deposit: 0, weight_refund: Weight::zero() },
		);

		assert!(AccountInfo::<Test>::is_delegated(&authority));

		let new_nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth2 = signer.sign_authorization(chain_id, H160::zero(), new_nonce);
		assert_eq!(
			test_process_authorizations(&[auth2]),
			AuthorizationResult { existing_accounts: 1, new_accounts: 0, deposit: 0, weight_refund: Weight::zero() },
		);

		assert!(!AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), None);
	});
}

#[test]
fn process_multiple_authorizations_from_different_signers() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());
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

		assert_eq!(
			test_process_authorizations(&[auth1, auth2, auth3]),
			AuthorizationResult { existing_accounts: 2, new_accounts: 1, deposit: 1, weight_refund: Weight::zero() },
		);

		assert!(AccountInfo::<Test>::is_delegated(&authority1));
		assert!(AccountInfo::<Test>::is_delegated(&authority2));
		assert!(AccountInfo::<Test>::is_delegated(&authority3));
	});
}

/// Runtime test: Set and clear authorization via eth_call
///
/// This test verifies that an EOA can delegate to a contract via EIP-7702
/// authorization list, and then clear its delegation by setting the
/// authorization address to 0x0 (zero address).
#[test]
fn test_runtime_set_and_clear_authorization() {
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

		// Set delegation
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth1 = signer.sign_authorization(chain_id, target_contract.addr, nonce);
		let result1 = builder::eth_call_with_authorization_list(target_contract.addr)
			.authorization_list(vec![auth1])
			.eth_gas_limit(crate::test_utils::ETH_GAS_LIMIT.into())
			.build();
		assert_ok!(result1);
		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&authority),
			Some(target_contract.addr)
		);
		assert_eq!(frame_system::Pallet::<Test>::account_nonce(&authority_id), 1);

		// Clear delegation via zero address
		let new_nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth2 = signer.sign_authorization(chain_id, H160::zero(), new_nonce);
		let result2 = builder::eth_call_with_authorization_list(target_contract.addr)
			.authorization_list(vec![auth2])
			.eth_gas_limit(crate::test_utils::ETH_GAS_LIMIT.into())
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
		let result = builder::eth_call_with_authorization_list(target_contract.addr)
			.authorization_list(vec![auth])
			.eth_gas_limit(crate::test_utils::ETH_GAS_LIMIT.into())
			.build();
		assert_ok!(result);

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&authority),
			Some(target_contract.addr)
		);

		let call_result = builder::eth_call(authority)
			.eth_gas_limit(crate::test_utils::ETH_GAS_LIMIT.into())
			.build();

		assert_ok!(&call_result);
	});
}

/// Re-delegation to a different target preserves the same trie_id (storage persists).
///
/// Per EIP-7702, storage is keyed by the delegated address, not the target.
/// This means switching from target A to target B retains target A's storage
/// in the same child trie. The spec recommends ERC-7201 namespaced storage to
/// avoid layout collisions.
#[test]
fn redelegation_preserves_storage() {
	use alloy_core::sol_types::SolCall;
	use pallet_revive_fixtures::{compile_module_with_type, Counter, FixtureType};

	let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		// Deploy two Counter instances as delegation targets
		let counter_a = builder::bare_instantiate(Code::Upload(counter_code.clone()))
			.build_and_unwrap_contract();
		let counter_b = builder::bare_instantiate(Code::Upload(counter_code))
			.salt(Some([1; 32]))
			.build_and_unwrap_contract();

		// Alice delegates to Counter A and writes storage
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, counter_a.addr);

		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::setNumberCall { newNumber: 42u64 }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		// Verify storage was written
		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::numberCall {}.abi_encode())
			.build_and_unwrap_result();
		assert_eq!(Counter::numberCall::abi_decode_returns(&result.data).unwrap(), 42u64);

		// Re-delegate to Counter B (same ABI, same storage layout)
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, counter_b.addr);

		// Storage from Counter A should still be accessible since the trie_id is
		// derived from the delegated address, not the target
		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::numberCall {}.abi_encode())
			.build_and_unwrap_result();
		assert_eq!(
			Counter::numberCall::abi_decode_returns(&result.data).unwrap(),
			42u64,
			"Storage should persist across re-delegation"
		);

		// Counter B's increment should work on the same storage
		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::incrementCall {}.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::numberCall {}.abi_encode())
			.build_and_unwrap_result();
		assert_eq!(
			Counter::numberCall::abi_decode_returns(&result.data).unwrap(),
			43u64,
			"Increment via new target should work on persisted storage"
		);
	});
}

/// After clearing a delegation, calling the address should not execute code.
///
/// Even though contract_info (trie_id, deposit accounting) is preserved for
/// re-delegation, bare_call must not resolve code for a cleared delegation.
#[test]
fn cleared_delegation_does_not_execute_code() {
	use alloy_core::sol_types::SolCall;
	use pallet_revive_fixtures::{compile_module_with_type, Counter, FixtureType};

	let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let counter =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();

		// Delegate ALICE → Counter and write storage
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, counter.addr);

		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::setNumberCall { newNumber: 42u64 }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::numberCall {}.abi_encode())
			.build_and_unwrap_result();
		assert_eq!(Counter::numberCall::abi_decode_returns(&result.data).unwrap(), 42u64);

		// Clear delegation
		AccountInfo::<Test>::clear_delegation(&ALICE_ADDR);
		assert!(!AccountInfo::<Test>::is_delegated(&ALICE_ADDR));

		// Calling number() should no longer execute Counter code
		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::numberCall {}.abi_encode())
			.build_and_unwrap_result();
		assert!(result.data.is_empty(), "cleared delegation should not execute code");
	});
}

/// dry_run_eth_transact with authorization list processes delegations and
/// includes the ED cost for new accounts in the gas estimate.
#[test]
fn dry_run_with_authorization_list() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000_000);

		let target_contract = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let seed = H256::from([0xAA; 32]);
		let signer = TestSigner::new(&seed.0);

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&signer.address);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth = signer.sign_authorization(chain_id, target_contract.addr, nonce);

		// Dry run without authorization list
		let baseline = crate::Pallet::<Test>::dry_run_eth_transact(
			crate::GenericTransaction {
				from: Some(ALICE_ADDR),
				to: Some(target_contract.addr),
				..Default::default()
			},
			Default::default(),
		);
		assert_ok!(&baseline);

		// Dry run with authorization list
		let with_auth = crate::Pallet::<Test>::dry_run_eth_transact(
			crate::GenericTransaction {
				from: Some(ALICE_ADDR),
				to: Some(target_contract.addr),
				authorization_list: vec![auth],
				..Default::default()
			},
			Default::default(),
		);
		assert_ok!(&with_auth);

		// The gas estimate with auth should be >= baseline since it includes ED cost
		let baseline_gas = baseline.unwrap().eth_gas;
		let auth_gas = with_auth.unwrap().eth_gas;
		assert!(
			auth_gas >= baseline_gas,
			"Auth gas ({auth_gas}) should be >= baseline gas ({baseline_gas})"
		);

		// The delegation should have been applied during dry run
		assert!(AccountInfo::<Test>::is_delegated(&signer.address));
	});
}

/// Dry-run a transaction with an authorization list, then submit with the returned gas estimate.
/// The submitted transaction must pass validation and execute successfully.
#[test]
fn dry_run_gas_estimate_produces_valid_transaction() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));

		let target_contract = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let seed = H256::from([0xCC; 32]);
		let signer = TestSigner::new(&seed.0);

		// New account — no balance set, so ED will be charged
		let nonce = U256::zero();
		let auth = signer.sign_authorization(chain_id, target_contract.addr, nonce);

		// Dry run to get gas estimate
		let dry_run = crate::Pallet::<Test>::dry_run_eth_transact(
			crate::GenericTransaction {
				from: Some(ALICE_ADDR),
				to: Some(target_contract.addr),
				authorization_list: vec![auth.clone()],
				..Default::default()
			},
			Default::default(),
		);
		let info = dry_run.expect("dry run should succeed");

		// Submit with the estimated gas — should succeed
		let result = builder::eth_call_with_authorization_list(target_contract.addr)
			.authorization_list(vec![auth])
			.eth_gas_limit(info.eth_gas)
			.build();
		assert_ok!(result);

		assert!(AccountInfo::<Test>::is_delegated(&signer.address));
	});
}

/// Verify that a transaction with insufficient gas for authorization ED costs fails.
///
/// The pre-validation in `into_call` checks that the gas budget can cover the
/// existential deposit for worst-case new accounts. A tiny gas limit with
/// authorizations should be rejected.
#[test]
fn authorization_ed_gas_check() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000_000);

		let target_contract = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let seed = H256::from([0xBB; 32]);
		let signer = TestSigner::new(&seed.0);
		let nonce = U256::zero();
		let auth = signer.sign_authorization(chain_id, target_contract.addr, nonce);

		// Dry run with a gas limit too small to cover ED for the authorization
		let result = crate::Pallet::<Test>::dry_run_eth_transact(
			crate::GenericTransaction {
				from: Some(ALICE_ADDR),
				to: Some(target_contract.addr),
				gas: Some(U256::from(21_000u64)),
				authorization_list: vec![auth],
				..Default::default()
			},
			Default::default(),
		);

		assert!(result.is_err(), "Should reject when gas can't cover ED for auth accounts");
	});
}

/// Test that delegation chains are not followed during execution (EIP-7702 spec)
///
/// Per EIP-7702: "In case a delegation indicator points to another delegation,
/// creating a potential chain or loop of delegations, clients must retrieve
/// only the first code and then stop following the delegation chain."
///
/// This test verifies:
/// 1. Calling Alice (who delegates to Counter) executes the contract code
/// 2. A contract can delegatecall to Alice and execute the contract code
/// 3. Calling Bob (who delegates to Alice) does NOT execute code (chain not followed)
#[test]
fn delegation_chain_does_not_execute() {
	use alloy_core::sol_types::SolCall;
	use pallet_revive_fixtures::{compile_module_with_type, Caller, Counter, FixtureType};

	let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();
	let (caller_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&BOB, 100_000_000);

		// Deploy Counter contract
		let counter =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();

		// Alice delegates to the Counter contract
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, counter.addr);

		// Helper to read Alice's number storage slot
		let read_number = || {
			let result = builder::bare_call(ALICE_ADDR)
				.data(Counter::numberCall {}.abi_encode())
				.build_and_unwrap_result();
			assert!(!result.did_revert());
			Counter::numberCall::abi_decode_returns(&result.data).unwrap()
		};

		// Case 1: Calling Alice executes the contract - setNumber(42) should work
		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::setNumberCall { newNumber: 42u64 }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "calling Alice should execute Counter code");
		assert_eq!(read_number(), 42u64);

		// Case 2: A contract can delegatecall to Alice and execute the code
		let caller_contract =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let result = builder::bare_call(caller_contract.addr)
			.data(
				Caller::delegateCall {
					_callee: ALICE_ADDR.0.into(),
					_data: Counter::incrementCall {}.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "delegatecall to Alice should work");
		let decoded = Caller::delegateCall::abi_decode_returns(&result.data).unwrap();
		assert!(decoded.success, "delegatecall to Alice should succeed");
		// delegatecall modifies the caller's storage, not Alice's
		assert_eq!(read_number(), 42u64);

		// Case 3: Bob delegates to Alice (chain: Bob -> Alice -> Counter)
		// Calling Bob should NOT execute code because chains are not followed
		AccountInfo::<Test>::set_delegation(&BOB_ADDR, ALICE_ADDR);

		let result = builder::bare_call(BOB_ADDR)
			.data(Counter::setNumberCall { newNumber: 99u64 }.abi_encode())
			.build_and_unwrap_result();
		// Bob is treated as an EOA (no code), so the call succeeds but does nothing
		assert!(!result.did_revert(), "call to Bob should not revert (treated as EOA transfer)");
		assert!(result.data.is_empty(), "call to Bob should return empty (no code executed)");
		// Alice's number should be unchanged
		assert_eq!(read_number(), 42u64);
	});
}

/// SELFDESTRUCT on a delegated account transfers the account's balance to the
/// beneficiary but does NOT remove the delegation indicator or affect the
/// original contract. Per EIP-6780, selfdestruct only clears the account when
/// called in the same transaction as creation, which is not the case here.
///
/// Test flow:
/// 1. Deploy Terminate contract
/// 2. Fund Alice and delegate her to the Terminate contract
/// 3. Call destroy(beneficiary) on Alice — runs in Alice's context
/// 4. Verify: Alice balance → 0, beneficiary received funds
/// 5. Verify: delegation indicator survives (eth_getCode still returns 0xef0100||addr)
/// 6. Verify: original contract code unaffected
/// 7. Verify: delegation still functional (echo(42) returns 42)
#[test]
fn selfdestruct_on_delegated_account() {
	use alloy_core::sol_types::{SolCall, SolConstructor};
	use pallet_revive_fixtures::{compile_module_with_type, FixtureType, Terminate};

	let (code, _) = compile_module_with_type("Terminate", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		// Deploy the Terminate contract (skip=true to not selfdestruct in constructor)
		let contract = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(
				Terminate::constructorCall {
					skip: true,
					method: 0,
					beneficiary: H160::zero().0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_contract();

		// Fund Alice and delegate her to the Terminate contract
		let alice_balance = 5_000_000u64;
		let alice_id = <Test as Config>::AddressMapper::to_account_id(&ALICE_ADDR);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&alice_id, alice_balance);
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, contract.addr);
		assert!(AccountInfo::<Test>::is_delegated(&ALICE_ADDR));

		// Beneficiary must exist (has at least ED) so the selfdestruct balance
		// transfer doesn't need to charge ED from the origin (which is Alice
		// herself in the delegated case).
		let beneficiary = DJANGO_ADDR;
		let beneficiary_id = <Test as Config>::AddressMapper::to_account_id(&beneficiary);
		let min_balance = Contracts::min_balance();
		let _ =
			<<Test as Config>::Currency as Mutate<_>>::set_balance(&beneficiary_id, min_balance);

		// Save contract code before selfdestruct for later comparison
		let contract_code_before = crate::Pallet::<Test>::code(&contract.addr);
		assert!(!contract_code_before.is_empty());

		// Step 2: Call destroy(beneficiary) on Alice — selfdestruct runs in Alice's context
		let result = builder::bare_call(ALICE_ADDR)
			.data(
				Terminate::terminateCall {
					method: 2, // METHOD_SYSCALL = selfdestruct opcode
					beneficiary: beneficiary.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "selfdestruct should succeed");

		// Check balances — Alice's balance transferred to beneficiary.
		// EIP-6780: selfdestruct only sends balance, doesn't delete account
		// (account was not created in this transaction).
		let beneficiary_balance_after = <Test as Config>::Currency::free_balance(&beneficiary_id);
		assert!(beneficiary_balance_after > min_balance, "beneficiary should have received funds");

		// Step 4: Delegation indicator survives selfdestruct
		assert!(
			AccountInfo::<Test>::is_delegated(&ALICE_ADDR),
			"delegation should survive selfdestruct"
		);
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&ALICE_ADDR),
			Some(contract.addr),
			"delegation target should be unchanged"
		);

		// eth_getCode(alice) should still return the delegation indicator
		let mut expected_code = vec![0xef, 0x01, 0x00];
		expected_code.extend_from_slice(contract.addr.as_bytes());
		assert_eq!(crate::Pallet::<Test>::code(&ALICE_ADDR), expected_code);

		// Step 5: Original contract is completely unaffected
		let contract_code_after = crate::Pallet::<Test>::code(&contract.addr);
		assert_eq!(
			contract_code_before, contract_code_after,
			"original contract code should be unchanged"
		);

		// Step 6: Delegation still functional — echo(42) returns 42
		// Fund Alice again so we can make a call
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&alice_id, 1_000_000);
		let expected = alloy_core::primitives::U256::from(42u64);
		let result = builder::bare_call(ALICE_ADDR)
			.data(Terminate::echoCall { value: expected }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "delegation should still be functional after selfdestruct");
		let returned = Terminate::echoCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(returned, expected, "echo should return 42");
	});
}

/// Delegating to a contract charges a storage base deposit (held on the authority).
#[test]
fn delegation_charges_storage_deposit() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let setup = DelegationTestSetup::new([0xCC; 32]);
		let hold_before =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id);
		assert_eq!(hold_before, 0);

		let auth = setup.signer.sign_authorization(setup.chain_id, target.addr, setup.nonce());
		setup.process(&[auth]);

		let hold_after =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id);
		assert!(hold_after > 0, "should have a storage deposit hold after delegation");

		let contract_info = get_contract(&setup.signer.address);
		assert_eq!(hold_after, contract_info.storage_base_deposit());
	});
}

/// Clearing a delegation refunds the storage base deposit.
#[test]
fn clear_delegation_refunds_storage_deposit() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let setup = DelegationTestSetup::new([0xDD; 32]);

		// Set delegation
		let auth = setup.signer.sign_authorization(setup.chain_id, target.addr, setup.nonce());
		setup.process(&[auth]);

		let hold_after_set =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id);
		assert!(hold_after_set > 0);

		// Clear delegation (address = zero)
		let auth = setup.signer.sign_authorization(setup.chain_id, H160::zero(), setup.nonce());
		setup.process(&[auth]);

		let hold_after_clear =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id);
		assert_eq!(hold_after_clear, 0, "hold should be fully released after clearing delegation");

		assert!(get_contract_checked(&setup.signer.address).is_none());
	});
}

/// Re-delegating to a different contract adjusts the deposit correctly.
#[test]
fn redelegation_adjusts_deposit() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		// Both targets use the same code, so deposits should be equal
		let target_a = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();
		let target_b = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.salt(Some([1; 32]))
			.build_and_unwrap_contract();

		let setup = DelegationTestSetup::new([0xEE; 32]);

		// Delegate to target A
		let auth = setup.signer.sign_authorization(setup.chain_id, target_a.addr, setup.nonce());
		setup.process(&[auth]);

		let hold_a =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id);
		assert!(hold_a > 0);

		// Re-delegate to target B (same code size → same deposit)
		let auth = setup.signer.sign_authorization(setup.chain_id, target_b.addr, setup.nonce());
		setup.process(&[auth]);

		let hold_b =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id);
		assert_eq!(hold_a, hold_b, "same-code re-delegation should keep the same deposit");
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&setup.signer.address),
			Some(target_b.addr)
		);
	});
}

/// Delegation to a contract increments its code refcount; clearing decrements it.
#[test]
fn delegation_manages_code_refcount() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let code_hash = get_contract(&target.addr).code_hash;
		let refcount_before = CodeInfoOf::<Test>::get(code_hash).unwrap().refcount();

		// Delegate authority → target
		let authority = H160::from([0x11; 20]);
		AccountInfo::<Test>::set_delegation(&authority, target.addr);

		let refcount_after_set = CodeInfoOf::<Test>::get(code_hash).unwrap().refcount();
		assert_eq!(refcount_after_set, refcount_before + 1, "delegation should increment refcount");

		// Clear delegation
		AccountInfo::<Test>::clear_delegation(&authority);

		let refcount_after_clear = CodeInfoOf::<Test>::get(code_hash).unwrap().refcount();
		assert_eq!(
			refcount_after_clear, refcount_before,
			"clearing delegation should decrement refcount"
		);
	});
}

/// Re-delegation from contract A to contract B decrements A's refcount and increments B's.
#[test]
fn redelegation_updates_refcounts() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let target_a = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		// Deploy a different contract so it has a different code hash
		use pallet_revive_fixtures::{compile_module_with_type, FixtureType};
		let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();
		let target_b =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();

		let hash_a = get_contract(&target_a.addr).code_hash;
		let hash_b = get_contract(&target_b.addr).code_hash;
		assert_ne!(hash_a, hash_b);

		let refcount_a_before = CodeInfoOf::<Test>::get(hash_a).unwrap().refcount();
		let refcount_b_before = CodeInfoOf::<Test>::get(hash_b).unwrap().refcount();

		let authority = H160::from([0x11; 20]);

		// Delegate to A
		AccountInfo::<Test>::set_delegation(&authority, target_a.addr);
		assert_eq!(CodeInfoOf::<Test>::get(hash_a).unwrap().refcount(), refcount_a_before + 1);

		// Re-delegate to B
		AccountInfo::<Test>::set_delegation(&authority, target_b.addr);
		assert_eq!(
			CodeInfoOf::<Test>::get(hash_a).unwrap().refcount(),
			refcount_a_before,
			"old code refcount should be decremented"
		);
		assert_eq!(
			CodeInfoOf::<Test>::get(hash_b).unwrap().refcount(),
			refcount_b_before + 1,
			"new code refcount should be incremented"
		);
	});
}

/// Delegating to a non-contract (plain EOA) does not create a contract_info or charge a deposit.
#[test]
fn delegation_to_eoa_has_no_deposit() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x11; 20]);
		let plain_eoa = H160::from([0x22; 20]);
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		let deposit = AccountInfo::<Test>::set_delegation(&authority, plain_eoa);

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert!(deposit.is_zero(), "delegation to EOA should not charge any deposit");
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id),
			0
		);
		// No contract info created
		assert!(get_contract_checked(&authority).is_none());
	});
}

/// Delegating to a contract and then to a plain EOA refunds the deposit.
#[test]
fn redelegation_from_contract_to_eoa_refunds() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let setup = DelegationTestSetup::new([0xAA; 32]);

		// Delegate to contract
		let auth = setup.signer.sign_authorization(setup.chain_id, target.addr, setup.nonce());
		setup.process(&[auth]);

		let hold_after_contract =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id);
		assert!(hold_after_contract > 0);

		// Re-delegate to a plain EOA (no code)
		let plain_eoa = H160::from([0x77; 20]);
		let auth = setup.signer.sign_authorization(setup.chain_id, plain_eoa, setup.nonce());
		setup.process(&[auth]);

		let hold_after_eoa =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id);
		assert_eq!(hold_after_eoa, 0, "deposit should be refunded when re-delegating to EOA");
	});
}

/// Multiple delegations from different authorities to the same contract each get their own deposit.
#[test]
fn multiple_delegations_each_have_own_deposit() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let authority_a = H160::from([0x11; 20]);
		let authority_b = H160::from([0x22; 20]);
		let id_a = <Test as Config>::AddressMapper::to_account_id(&authority_a);
		let id_b = <Test as Config>::AddressMapper::to_account_id(&authority_b);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&id_a, 100_000_000);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&id_b, 100_000_000);

		// Delegate both to the same target
		let deposit_a = AccountInfo::<Test>::set_delegation(&authority_a, target.addr);
		let deposit_b = AccountInfo::<Test>::set_delegation(&authority_b, target.addr);

		// Both should get the same charge since they delegate to the same code
		assert_eq!(deposit_a, deposit_b);

		// Each authority has independent contract info
		let ci_a = get_contract(&authority_a);
		let ci_b = get_contract(&authority_b);
		assert_eq!(ci_a.storage_base_deposit(), ci_b.storage_base_deposit());
		// But different trie_ids (storage is per-delegator)
		assert_ne!(ci_a.child_trie_info(), ci_b.child_trie_info());
	});
}

/// Re-delegation to the same target does not change the refcount.
#[test]
fn redelegation_to_same_target_keeps_refcount() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();
		let code_hash = get_contract(&target.addr).code_hash;

		let authority = H160::from([0x11; 20]);

		AccountInfo::<Test>::set_delegation(&authority, target.addr);
		let refcount_after_first = CodeInfoOf::<Test>::get(code_hash).unwrap().refcount();

		// Re-delegate to the same target
		AccountInfo::<Test>::set_delegation(&authority, target.addr);
		let refcount_after_second = CodeInfoOf::<Test>::get(code_hash).unwrap().refcount();

		assert_eq!(
			refcount_after_second, refcount_after_first,
			"re-delegation to same target should not change refcount"
		);
	});
}

/// Full round-trip via runtime: set delegation → verify deposit → clear → verify refund.
#[test]
fn runtime_delegation_deposit_roundtrip() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));

		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let seed = H256::from([0xBB; 32]);
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		// Set delegation via eth_call
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth = signer.sign_authorization(chain_id, target.addr, nonce);
		assert_ok!(builder::eth_call_with_authorization_list(target.addr)
			.authorization_list(vec![auth])
			.eth_gas_limit(crate::test_utils::ETH_GAS_LIMIT.into())
			.build());

		let hold_after_set =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id);
		assert!(hold_after_set > 0, "deposit should be held after delegation");

		// Clear delegation via eth_call (zero address)
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth = signer.sign_authorization(chain_id, H160::zero(), nonce);
		assert_ok!(builder::eth_call_with_authorization_list(target.addr)
			.authorization_list(vec![auth])
			.eth_gas_limit(crate::test_utils::ETH_GAS_LIMIT.into())
			.build());

		let hold_after_clear =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id);
		assert_eq!(hold_after_clear, 0, "deposit should be fully released");
	});
}
