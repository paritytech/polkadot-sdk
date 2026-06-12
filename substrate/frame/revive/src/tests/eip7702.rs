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
	Code, CodeInfoOf, Config, DryRunConfig, ExecConfig, HoldReason,
	evm::{
		AuthorizationListEntry, Bytes, StateOverride, StateOverrideSet, StorageOverride,
		eip7702::AuthorizationResult, fees::InfoT,
	},
	storage::{AccountInfo, AccountType},
	test_utils::builder::Contract,
	tests::{TestSigner, builder, test_utils::*, *},
	weights::WeightInfo,
};
use alloc::collections::BTreeMap;
use alloy_core::sol_types::{SolCall, SolConstructor};
use frame_support::{
	assert_ok,
	traits::fungible::{Balanced, Inspect, Mutate},
	weights::Weight,
};
use pallet_revive_fixtures::{
	Caller, Counter, FixtureType, Host, System as SystemFixture, Terminate,
	compile_module_with_type,
};
use sp_core::{H160, H256, U256};

/// Compute the expected weight refund for a given mix of new/existing/skipped accounts.
/// Mirrors the logic in `process_authorizations`: skipped tuples are billed at the
/// existing-account weight (we still ran sig recovery on them, so they aren't free).
fn expected_weight_refund_for(total: u32, new_accounts: u32, existing_accounts: u32) -> Weight {
	let invalid = total.saturating_sub(new_accounts).saturating_sub(existing_accounts);
	let worst = <Test as Config>::WeightInfo::process_new_account_authorization(total)
		.saturating_add(<Test as Config>::WeightInfo::process_existing_account_authorization(0));
	let actual = <Test as Config>::WeightInfo::process_new_account_authorization(new_accounts)
		.saturating_add(<Test as Config>::WeightInfo::process_existing_account_authorization(
			existing_accounts.saturating_add(invalid),
		));
	worst.saturating_sub(actual)
}

fn expected_weight_refund(new_accounts: u32, existing_accounts: u32) -> Weight {
	expected_weight_refund_for(new_accounts + existing_accounts, new_accounts, existing_accounts)
}

/// Common setup for delegation tests that call `process_authorizations` directly.
pub struct DelegationTestSetup {
	pub signer: TestSigner,
	pub authority_id: AccountId32,
	origin: AccountId32,
	exec_config: ExecConfig<Test>,
	chain_id: U256,
}

impl Default for DelegationTestSetup {
	fn default() -> Self {
		Self::new([1u8; 32])
	}
}

impl DelegationTestSetup {
	pub fn new(seed: [u8; 32]) -> Self {
		let setup = Self::new_unfunded(seed);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(
			&setup.authority_id,
			100_000_000,
		);
		setup
	}

	fn new_unfunded(seed: [u8; 32]) -> Self {
		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let signer = TestSigner::new(&seed);
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&signer.address);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));
		let origin = <Test as Config>::AddressMapper::to_account_id(&H160::from([0xFF; 20]));
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&origin, 10_000_000_000);
		let exec_config = ExecConfig::new_eth_tx(U256::from(1), 0, Weight::MAX);
		Self { signer, authority_id, origin, exec_config, chain_id }
	}

	pub fn process(
		&self,
		auths: &[AuthorizationListEntry],
	) -> AuthorizationResult<crate::BalanceOf<Test>> {
		crate::evm::eip7702::process_authorizations::<Test>(auths, &self.origin, &self.exec_config)
	}

	pub fn nonce(&self) -> U256 {
		U256::from(frame_system::Pallet::<Test>::account_nonce(&self.authority_id))
	}

	/// Sign an authorization for the given target using the current nonce and chain_id.
	pub fn sign_authorization(&self, target: H160) -> AuthorizationListEntry {
		self.signer.sign_authorization(self.chain_id, target, self.nonce())
	}

	/// Sign, process, and assert delegation succeeded.
	pub fn authorize(&self, target: H160) -> AuthorizationResult<crate::BalanceOf<Test>> {
		let auth = self.sign_authorization(target);
		let result = self.process(&[auth]);
		assert!(AccountInfo::<Test>::is_delegated(&self.signer.address));
		result
	}
}

#[test]
fn delegation_storage_basics() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x11; 20]);
		let target1 = H160::from([0x22; 20]);
		let target2 = H160::from([0x33; 20]);

		// Set delegation
		AccountInfo::<Test>::set_delegation(&authority, target1).unwrap();
		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target1));

		// Update to different target
		AccountInfo::<Test>::set_delegation(&authority, target2).unwrap();
		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target2));

		// Clear delegation
		AccountInfo::<Test>::clear_delegation(&authority).unwrap();
		assert!(!AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), None);
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
fn eip3607_checks() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 1_000_000_000);

		// Delegated EOAs are allowed to originate transactions
		let authority = H160::from([0x11; 20]);
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 1_000_000);
		AccountInfo::<Test>::set_delegation(&authority, H160::from([0x22; 20])).unwrap();
		assert_ok!(Contracts::ensure_non_contract_if_signed(&RuntimeOrigin::signed(authority_id)));

		// Regular contracts are rejected
		let Contract { account_id, .. } =
			builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
				.build_and_unwrap_contract();
		assert!(
			Contracts::ensure_non_contract_if_signed(&RuntimeOrigin::signed(account_id)).is_err()
		);
	});
}

#[test]
fn authorization_happy_path() {
	ExtBuilder::default().build().execute_with(|| {
		let target = H160::from([0x42; 20]);
		let existing_one = AuthorizationResult {
			existing_accounts: 1,
			new_accounts: 0,
			deposit: 0,
			weight_refund: expected_weight_refund(0, 1),
		};

		// Valid signature → delegated, nonce incremented
		let setup = DelegationTestSetup::new([1u8; 32]);
		let nonce_before = frame_system::Pallet::<Test>::account_nonce(&setup.authority_id);
		let auth = setup.sign_authorization(target);
		assert_eq!(setup.process(&[auth]), existing_one);
		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&setup.signer.address), Some(target));
		assert_eq!(
			frame_system::Pallet::<Test>::account_nonce(&setup.authority_id),
			nonce_before + 1
		);

		// chain_id = 0 (wildcard) is accepted
		let setup = DelegationTestSetup::new([2u8; 32]);
		let auth = setup.signer.sign_authorization(U256::zero(), target, setup.nonce());
		assert_eq!(setup.process(&[auth]), existing_one);
		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&setup.signer.address), Some(target));
	});
}

#[test]
fn invalid_authorization_is_skipped() {
	ExtBuilder::default().build().execute_with(|| {
		let target = H160::from([0x42; 20]);
		let skipped = AuthorizationResult {
			existing_accounts: 0,
			new_accounts: 0,
			deposit: 0,
			weight_refund: expected_weight_refund_for(1, 0, 0),
		};

		// Wrong chain_id
		let setup = DelegationTestSetup::new([1u8; 32]);
		let auth = setup.signer.sign_authorization(U256::from(999), target, setup.nonce());
		assert_eq!(setup.process(&[auth]), skipped);
		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));

		// Wrong nonce
		let setup = DelegationTestSetup::new([2u8; 32]);
		let wrong_nonce = setup.nonce().saturating_add(U256::from(1));
		let auth = setup.signer.sign_authorization(setup.chain_id, target, wrong_nonce);
		assert_eq!(setup.process(&[auth]), skipped);
		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));

		// Corrupted signature
		let setup = DelegationTestSetup::new([3u8; 32]);
		let auth = AuthorizationListEntry {
			chain_id: setup.chain_id,
			address: target,
			nonce: setup.nonce(),
			y_parity: U256::zero(),
			r: U256::from(0xdeadbeef_u64),
			s: U256::from(0xcafebabe_u64),
		};
		assert_eq!(setup.process(&[auth]), skipped);
		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));
	});
}

/// Authorization for an account that is already a contract is skipped.
/// Per EIP-7702, only EOAs can be delegated — contract accounts are ineligible.
#[test]
fn contract_account_rejects_authorization() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let setup = DelegationTestSetup::new([1u8; 32]);
		let target = H160::from([0x42; 20]);

		// Deploy a contract and mark the signer's address as a contract
		let contract = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();
		let contract_info = get_contract(&contract.addr);

		// Overwrite the signer's account info to be a Contract type
		crate::AccountInfoOf::<Test>::insert(
			setup.signer.address,
			crate::storage::AccountInfo {
				account_type: AccountType::Contract(contract_info),
				dust: 0,
			},
		);
		assert!(AccountInfo::<Test>::is_contract(&setup.signer.address));

		let auth = setup.sign_authorization(target);

		// Authorization should be skipped because the authority is a contract
		assert_eq!(
			setup.process(&[auth]),
			AuthorizationResult {
				existing_accounts: 0,
				new_accounts: 0,
				deposit: 0,
				weight_refund: expected_weight_refund_for(1, 0, 0)
			}
		);

		// Account should still be a contract, not delegated
		assert!(AccountInfo::<Test>::is_contract(&setup.signer.address));
		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));
	});
}

#[test]
fn multiple_authorizations_with_same_nonce_first_wins() {
	ExtBuilder::default().build().execute_with(|| {
		let setup = DelegationTestSetup::new([1u8; 32]);
		let target1 = H160::from([0x11; 20]);
		let target2 = H160::from([0x22; 20]);
		let target3 = H160::from([0x33; 20]);

		let nonce = setup.nonce();

		// Duplicate-nonce case: the first auth bumps the nonce, so the rest fail
		// the nonce check and are skipped. With properly incremented nonces the
		// spec rule is the opposite — later auths overwrite earlier ones.
		let auth1 = setup.signer.sign_authorization(setup.chain_id, target1, nonce);
		let auth2 = setup.signer.sign_authorization(setup.chain_id, target2, nonce);
		let auth3 = setup.signer.sign_authorization(setup.chain_id, target3, nonce);

		assert_eq!(
			setup.process(&[auth1, auth2, auth3]),
			AuthorizationResult {
				existing_accounts: 1,
				new_accounts: 0,
				deposit: 0,
				weight_refund: expected_weight_refund_for(3, 0, 1)
			},
		);

		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&setup.signer.address),
			Some(target1)
		);
	});
}

#[test]
fn new_account_sets_delegation() {
	ExtBuilder::default().build().execute_with(|| {
		let setup = DelegationTestSetup::new_unfunded([1u8; 32]);
		let target = H160::from([0x42; 20]);

		let auth = setup.sign_authorization(target);

		assert_eq!(
			setup.process(&[auth]),
			AuthorizationResult {
				existing_accounts: 0,
				new_accounts: 1,
				deposit: 1,
				weight_refund: expected_weight_refund(1, 0)
			},
		);

		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&setup.signer.address), Some(target));
		let balance = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.authority_id);
		assert_eq!(balance, Pallet::<Test>::min_balance());
	});
}

#[test]
fn clearing_delegation_with_zero_address() {
	ExtBuilder::default().build().execute_with(|| {
		let setup = DelegationTestSetup::new([1u8; 32]);
		let target = H160::from([0x42; 20]);

		let auth1 = setup.sign_authorization(target);

		assert_eq!(
			setup.process(&[auth1]),
			AuthorizationResult {
				existing_accounts: 1,
				new_accounts: 0,
				deposit: 0,
				weight_refund: expected_weight_refund(0, 1)
			},
		);

		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));

		let auth2 = setup.sign_authorization(H160::zero());
		assert_eq!(
			setup.process(&[auth2]),
			AuthorizationResult {
				existing_accounts: 1,
				new_accounts: 0,
				deposit: 0,
				weight_refund: expected_weight_refund(0, 1)
			},
		);

		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&setup.signer.address), None);
	});
}

/// EIP-7702 spec: an authorization with `address == 0x00…00` on a non-existent authority
/// satisfies step 5 (code is empty), so the auth must still execute — creating the account,
/// bumping its nonce, and leaving it as a plain EOA with no delegation.
#[test]
fn clear_delegation_on_non_existent_account_bumps_nonce() {
	ExtBuilder::default().build().execute_with(|| {
		let setup = DelegationTestSetup::new_unfunded([0xCC; 32]);
		let authority_id = setup.authority_id.clone();

		// Sanity: authority does not exist in frame_system yet.
		assert!(!frame_system::Account::<Test>::contains_key(&authority_id));
		assert_eq!(frame_system::Pallet::<Test>::account_nonce(&authority_id), 0);

		let auth = setup.sign_authorization(H160::zero());
		let result = setup.process(&[auth]);

		// Step 9 ran: nonce was bumped from 0 → 1, and that materialized the account.
		assert!(frame_system::Account::<Test>::contains_key(&authority_id));
		assert_eq!(frame_system::Pallet::<Test>::account_nonce(&authority_id), 1);
		// Step 8's zero-address branch: no delegation indicator written.
		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&setup.signer.address), None);
		// Counter accounting: a new account was created (the authority itself).
		assert_eq!(result.new_accounts, 1);
		assert_eq!(result.existing_accounts, 0);
	});
}

#[test]
fn process_multiple_authorizations_from_different_signers() {
	ExtBuilder::default().build().execute_with(|| {
		let setup1 = DelegationTestSetup::new([1u8; 32]);
		let setup2 = DelegationTestSetup::new([2u8; 32]);
		let setup3 = DelegationTestSetup::new_unfunded([3u8; 32]);
		let target = H160::from([0x42; 20]);

		let auth1 = setup1.sign_authorization(target);
		let auth2 = setup2.sign_authorization(target);
		let auth3 = setup3.sign_authorization(target);

		assert_eq!(
			setup1.process(&[auth1, auth2, auth3]),
			AuthorizationResult {
				existing_accounts: 2,
				new_accounts: 1,
				deposit: 1,
				weight_refund: expected_weight_refund(1, 2)
			},
		);

		assert!(AccountInfo::<Test>::is_delegated(&setup1.signer.address));
		assert!(AccountInfo::<Test>::is_delegated(&setup2.signer.address));
		assert!(AccountInfo::<Test>::is_delegated(&setup3.signer.address));
	});
}

/// Per EIP-7702: "If any step above fails, immediately stop processing the tuple and continue
/// to the next tuple." Step 8 (set code) is one such step. We engineer a post-validation
/// failure in `set_delegation` (by deleting the target's `CodeInfoOf` entry, so the refcount
/// bump fails with `CodeNotFound`) and verify the rest of the authorization list is still
/// processed and the transaction itself does not revert.
#[test]
fn auth_failing_post_validation_skips_without_aborting_list() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();
		let target_bad = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();
		let target_good =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();

		// Stage a post-validation failure: `target_bad`'s account still claims to be a
		// contract with its original code_hash, but the corresponding `CodeInfoOf` entry
		// is removed. `set_delegation` reads the snapshot hash from `AccountInfoOf`, then
		// `increment_refcount` fails with `CodeNotFound` — exactly the kind of post-step-8
		// failure the spec says we must skip past.
		let bad_code_hash = get_contract(&target_bad.addr).code_hash;
		CodeInfoOf::<Test>::remove(bad_code_hash);

		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let setup = DelegationTestSetup::new([0xAA; 32]);
		let good_signer = TestSigner::new(&[0xBB; 32]);
		let good_id = <Test as Config>::AddressMapper::to_account_id(&good_signer.address);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&good_id, 100_000_000);

		let auth_bad = setup.sign_authorization(target_bad.addr);
		let auth_good = good_signer.sign_authorization(chain_id, target_good.addr, U256::zero());

		// Without the per-auth `with_transaction` skip, the bad auth's error would propagate
		// out and abort the whole list. With the skip, this returns normally and we can
		// inspect the per-auth outcome via the returned counts.
		let result = setup.process(&[auth_bad, auth_good]);

		// Bad auth: silently dropped — no delegation, no nonce bump for its authority.
		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(frame_system::Pallet::<Test>::account_nonce(&setup.authority_id), 0);

		// Good auth: applied.
		assert!(AccountInfo::<Test>::is_delegated(&good_signer.address));
		assert_eq!(frame_system::Pallet::<Test>::account_nonce(&good_id), 1);

		// Counters only reflect the auth that committed.
		assert_eq!(result.new_accounts, 0);
		assert_eq!(result.existing_accounts, 1);
	});
}

/// Runtime test: Set and clear authorization via eth_call.
/// Verifies delegation state, nonce increment, and deposit lifecycle.
#[test]
fn test_runtime_set_and_clear_authorization() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());

		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));

		let seed = H256::from([1u8; 32]);
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let target_contract = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		// Set delegation
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth1 = signer.sign_authorization(chain_id, target_contract.addr, nonce);
		assert_ok!(
			builder::eth_call(target_contract.addr)
				.authorization_list(vec![auth1])
				.eth_gas_limit(crate::test_utils::ETH_GAS_LIMIT.into())
				.build()
		);
		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&authority),
			Some(target_contract.addr)
		);
		assert_eq!(frame_system::Pallet::<Test>::account_nonce(&authority_id), 1);
		let hold_after_set =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id);
		assert!(hold_after_set > 0, "deposit should be held after delegation");

		// Clear delegation via zero address
		let new_nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth2 = signer.sign_authorization(chain_id, H160::zero(), new_nonce);
		assert_ok!(
			builder::eth_call(target_contract.addr)
				.authorization_list(vec![auth2])
				.eth_gas_limit(crate::test_utils::ETH_GAS_LIMIT.into())
				.build()
		);
		assert!(!AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), None);
		assert!(!AccountInfo::<Test>::is_contract(&authority));
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id),
			0,
			"deposit should be fully released"
		);
	});
}

/// Delegation set via authorization list allows calling the delegated address
/// in the same eth_call. Authorizations are processed before execution, so the
/// call body finds the delegation and executes the target contract's code.
#[test]
fn test_runtime_delegation_resolution() {
	let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(<Test as Config>::ChainId::get());

		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));

		let counter =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();

		let seed = H256::from([1u8; 32]);
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth = signer.sign_authorization(chain_id, counter.addr, nonce);
		let result = builder::eth_call(authority)
			.authorization_list(vec![auth])
			.eth_gas_limit(crate::test_utils::ETH_GAS_LIMIT.into())
			.data(Counter::setNumberCall { newNumber: 42u64 }.abi_encode())
			.build();
		assert_ok!(result);

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(counter.addr));

		// Overwrite with a distinct value via bare_call so the final read can't be
		// confused with the eth_call's setNumber(42) write above.
		let write_result = builder::bare_call(authority)
			.data(Counter::setNumberCall { newNumber: 1337u64 }.abi_encode())
			.build_and_unwrap_result();
		assert!(!write_result.did_revert());

		let read_result = builder::bare_call(authority)
			.data(Counter::numberCall {}.abi_encode())
			.build_and_unwrap_result();
		assert!(!read_result.did_revert());
		assert_eq!(Counter::numberCall::abi_decode_returns(&read_result.data).unwrap(), 1337u64);
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
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, counter_a.addr).unwrap();

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
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, counter_b.addr).unwrap();

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
	let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let counter =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();

		// Delegate ALICE → Counter and write storage
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, counter.addr).unwrap();

		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::setNumberCall { newNumber: 42u64 }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::numberCall {}.abi_encode())
			.build_and_unwrap_result();
		assert_eq!(Counter::numberCall::abi_decode_returns(&result.data).unwrap(), 42u64);

		// Clear delegation
		AccountInfo::<Test>::clear_delegation(&ALICE_ADDR).unwrap();
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

		// The gas estimate with auth should be strictly greater since it includes ED cost
		let baseline_gas = baseline.unwrap().eth_gas;
		let auth_gas = with_auth.unwrap().eth_gas;
		assert!(
			auth_gas > baseline_gas,
			"Auth gas ({auth_gas}) should be > baseline gas ({baseline_gas})"
		);

		// The delegation should have been applied during dry run
		assert!(AccountInfo::<Test>::is_delegated(&signer.address));
	});
}

/// State-override path: a `code` override of `0xef0100 || target` must install the account as
/// a `DelegatedEOA` pointing at `target`, not as raw bytecode.
///
/// We seed `Eve`'s slot 0 with `42` via a storage override and dry-run `Counter::number()` on
/// `Eve`. The call only returns `42` if `Counter`'s code executes in `Eve`'s storage namespace —
/// i.e., the indicator was actually interpreted as a delegation. Without that handling, the
/// override would attempt to install the 23 indicator bytes as raw code, which fails (the first
/// byte `0xEF` is a reserved/invalid EVM opcode), and the dry-run returns empty/errors.
#[test]
fn state_override_delegation_indicator_routes_to_target() {
	let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000_000);
		let counter =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();

		let eve = H160::from([0xEE; 20]);

		// 0xef0100 || counter.addr (23 bytes total)
		let mut indicator = vec![0xefu8, 0x01, 0x00];
		indicator.extend_from_slice(counter.addr.as_bytes());

		// Counter::number is uint64 at slot 0.
		let mut slot_diff = BTreeMap::new();
		slot_diff.insert(H256::zero(), H256::from_low_u64_be(42));

		let mut overrides_map = BTreeMap::new();
		overrides_map.insert(
			eve,
			StateOverride {
				code: Some(Bytes(indicator)),
				storage: Some(StorageOverride::StateDiff(slot_diff)),
				..Default::default()
			},
		);
		let overrides = StateOverrideSet(overrides_map);

		let result = crate::Pallet::<Test>::dry_run_eth_transact(
			crate::GenericTransaction {
				from: Some(ALICE_ADDR),
				to: Some(eve),
				input: Counter::numberCall {}.abi_encode().into(),
				..Default::default()
			},
			DryRunConfig::default()
				.with_perform_balance_checks(false)
				.with_state_overrides(overrides),
		);

		let info = result.expect("dry-run with delegation indicator override should succeed");
		let returned = Counter::numberCall::abi_decode_returns(&info.data)
			.expect("call should return a valid uint64");
		assert_eq!(
			returned, 42u64,
			"delegation indicator override should route the call to Counter at Eve's storage"
		);
	});
}

/// EIP-7702 delegation chains are followed exactly one hop, after which the
/// retrieved bytes are executed as raw EVM bytecode.
///
/// Per spec: "clients must retrieve only the first code and then stop following
/// the delegation chain." The "first code" is the target's bytecode — which, if
/// the target is itself a delegated EOA, is the indicator `0xef0100||...`.
/// `0xef` is a designated invalid opcode (EIP-3541), so executing it traps and
/// the call reverts.
///
/// Reference: ethereum/execution-spec-tests
/// `test_set_code_address_and_authority_warm_state_call_types` covers this pointer-cycle case
/// explicitly.
///
/// This test verifies:
/// 1. Calling Alice (delegated to Counter) executes the Counter code.
/// 2. A contract can delegatecall to Alice and execute the Counter code.
/// 3. Calling Bob (delegated to Alice, who is herself delegated) reverts — Alice's "code" is the
///    indicator bytes and `0xef` traps as invalid opcode.
#[test]
fn delegation_chain_does_not_execute() {
	let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();
	let (caller_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&BOB, 100_000_000);

		// Deploy Counter contract
		let counter =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();

		// Alice delegates to the Counter contract
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, counter.addr).unwrap();

		// Helper to read Alice's number storage slot
		let read_number = || {
			let result = builder::bare_call(ALICE_ADDR)
				.data(Counter::numberCall {}.abi_encode())
				.build_and_unwrap_result();
			assert!(!result.did_revert());
			Counter::numberCall::abi_decode_returns(&result.data).unwrap()
		};

		// Distinct write values per case so each read assertion is unambiguous:
		// if any case accidentally rewrites Alice's slot, the final read won't
		// silently match the original value.
		const ALICE_INITIAL: u64 = 100;
		const DELEGATECALL_VALUE: u64 = 50;
		const CHAINED_ATTEMPT: u64 = 7;

		// Case 1: calling Alice executes Counter; the write lands in Alice's storage.
		let result = builder::bare_call(ALICE_ADDR)
			.data(Counter::setNumberCall { newNumber: ALICE_INITIAL }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "calling Alice should execute Counter code");
		assert_eq!(read_number(), ALICE_INITIAL);

		// Case 2: delegatecall from caller_contract into Alice — the write lands
		// in caller_contract's storage, not Alice's.
		let caller_contract =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let result = builder::bare_call(caller_contract.addr)
			.data(
				Caller::delegateCall {
					_callee: ALICE_ADDR.0.into(),
					_data: Counter::setNumberCall { newNumber: DELEGATECALL_VALUE }
						.abi_encode()
						.into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "delegatecall to Alice should work");
		let decoded = Caller::delegateCall::abi_decode_returns(&result.data).unwrap();
		assert!(decoded.success, "delegatecall to Alice should succeed");

		// DELEGATECALL_VALUE landed in caller_contract's storage (different slot
		// space than Alice's). Asserting Alice is still ALICE_INITIAL — not
		// DELEGATECALL_VALUE — proves the write didn't bleed into Alice's storage.
		assert_eq!(
			read_number(),
			ALICE_INITIAL,
			"Alice's storage must be untouched by delegatecall"
		);

		// Case 3: Bob delegates to Alice (chain: Bob -> Alice -> Counter). Per
		// spec, the call resolves one hop to Alice, retrieves Alice's "code"
		// (the indicator `0xef0100||counter`), and executes it as raw bytecode.
		// `0xef` is a designated invalid opcode, so the call reverts.
		AccountInfo::<Test>::set_delegation(&BOB_ADDR, ALICE_ADDR).unwrap();

		let result = builder::bare_call(BOB_ADDR)
			.data(Counter::setNumberCall { newNumber: CHAINED_ATTEMPT }.abi_encode())
			.build_and_unwrap_result();
		assert!(
			result.did_revert(),
			"call to chained delegation should revert (0xef invalid opcode)"
		);
		assert_eq!(
			read_number(),
			ALICE_INITIAL,
			"Alice's storage must be unchanged after chain attempt"
		);
	});
}

/// Delegation to a precompile address: the indicator is set and surfaces via
/// `eth_getCode`, but calls into the authority currently no-op rather than
/// dispatching the precompile.
///
/// Spec-correct behavior is to execute the precompile (in the authority's
/// context, with the call's input/value). We don't do that today because
/// `set_delegation` snapshots the target's `code_hash` — precompiles have no
/// stored code, so the snapshot is zero and call dispatch falls through to the
/// EOA-transfer path. Same root cause as the other snapshot-vs-call-time
/// deviations; fixed wholesale by call-time resolution (tracked as follow-up).
///
/// When call-time resolution lands, this test should flip to assert the
/// identity precompile actually echoes the input.
#[test]
fn delegation_to_precompile_is_noop() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		// 0x0000...0004 is the identity precompile (ECHOes input bytes).
		let identity_precompile = H160::from_low_u64_be(0x04);

		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, identity_precompile).unwrap();
		assert!(AccountInfo::<Test>::is_delegated(&ALICE_ADDR));
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&ALICE_ADDR),
			Some(identity_precompile),
		);

		// eth_getCode returns the indicator pointing at the precompile address.
		let mut expected_code = vec![0xef, 0x01, 0x00];
		expected_code.extend_from_slice(identity_precompile.as_bytes());
		assert_eq!(crate::Pallet::<Test>::code(&ALICE_ADDR), expected_code);

		// Current (deviant) behavior: the call is a no-op. Spec-correct behavior
		// would echo the input bytes via the identity precompile.
		let input = vec![0xDE, 0xAD, 0xBE, 0xEF];
		let result = builder::bare_call(ALICE_ADDR).data(input.clone()).build_and_unwrap_result();
		assert!(!result.did_revert(), "call should not revert under current impl");
		assert!(
			result.data.is_empty(),
			"under current impl no precompile dispatch happens; data is empty. \
			 With call-time resolution this assertion should flip to: \
			 assert_eq!(result.data, input)"
		);
	});
}

/// Delegation to a nonexistent address (no deployed code) results in a no-op call.
/// The authority is treated as an EOA with no executable code.
#[test]
fn delegation_to_nonexistent_address_is_noop() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		// Delegate Alice to an address that has no deployed contract
		let nonexistent = H160::from([0xDE; 20]);
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, nonexistent).unwrap();
		assert!(AccountInfo::<Test>::is_delegated(&ALICE_ADDR));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&ALICE_ADDR), Some(nonexistent));

		// Calling Alice should succeed but execute no code (empty return data)
		let result = builder::bare_call(ALICE_ADDR)
			.data(vec![0xDE, 0xAD, 0xBE, 0xEF]) // arbitrary calldata
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "call should not revert");
		assert!(
			result.data.is_empty(),
			"no code should execute for delegation to nonexistent address"
		);

		// eth_getCode should still return the delegation indicator
		let mut expected_code = vec![0xef, 0x01, 0x00];
		expected_code.extend_from_slice(nonexistent.as_bytes());
		assert_eq!(crate::Pallet::<Test>::code(&ALICE_ADDR), expected_code);
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
		let alice_balance = 5_000_000u128;
		let alice_id = <Test as Config>::AddressMapper::to_account_id(&ALICE_ADDR);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&alice_id, alice_balance);
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, contract.addr).unwrap();
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
		let alice_balance_after = <Test as Config>::Currency::free_balance(&alice_id);
		let beneficiary_balance_after = <Test as Config>::Currency::free_balance(&beneficiary_id);
		assert_eq!(
			beneficiary_balance_after,
			min_balance + alice_balance - alice_balance_after,
			"beneficiary should have received Alice's transferable balance"
		);

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

/// Delegating to a contract charges a storage deposit; clearing refunds it.
#[test]
fn delegation_deposit_lifecycle() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let setup = DelegationTestSetup::new([0xCC; 32]);
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			0
		);

		// Set delegation → deposit charged
		setup.authorize(target.addr);
		let hold =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id);
		assert!(hold > 0, "should have a storage deposit hold after delegation");
		assert_eq!(hold, get_contract(&setup.signer.address).storage_base_deposit());

		// Clear delegation → deposit refunded
		let auth = setup.sign_authorization(H160::zero());
		setup.process(&[auth]);
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			0,
			"hold should be fully released after clearing delegation"
		);
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
		setup.authorize(target_a.addr);

		let hold_a =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id);
		assert!(hold_a > 0);

		// Re-delegate to target B (same code size → same deposit)
		let auth = setup.sign_authorization(target_b.addr);
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
/// Re-delegation to the same target does not change the refcount.
#[test]
fn delegation_manages_code_refcount() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let code_hash = get_contract(&target.addr).code_hash;
		let refcount_before = CodeInfoOf::<Test>::get(code_hash).unwrap().refcount();

		let authority = H160::from([0x11; 20]);

		// Set delegation → refcount++
		AccountInfo::<Test>::set_delegation(&authority, target.addr).unwrap();
		assert_eq!(CodeInfoOf::<Test>::get(code_hash).unwrap().refcount(), refcount_before + 1);

		// Re-delegate to same target → refcount unchanged
		AccountInfo::<Test>::set_delegation(&authority, target.addr).unwrap();
		assert_eq!(CodeInfoOf::<Test>::get(code_hash).unwrap().refcount(), refcount_before + 1);

		// Clear delegation → refcount--
		AccountInfo::<Test>::clear_delegation(&authority).unwrap();
		assert_eq!(CodeInfoOf::<Test>::get(code_hash).unwrap().refcount(), refcount_before);
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
		AccountInfo::<Test>::set_delegation(&authority, target_a.addr).unwrap();
		assert_eq!(CodeInfoOf::<Test>::get(hash_a).unwrap().refcount(), refcount_a_before + 1);

		// Re-delegate to B
		AccountInfo::<Test>::set_delegation(&authority, target_b.addr).unwrap();
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

/// Re-delegation from contract → EOA → contract must not double-decrement the original
/// code's refcount or double-refund the deposit. Verifies both refcounts and returned
/// deposit values at each step.
#[test]
fn redelegation_via_eoa_does_not_double_decrement() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let target_a = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();
		let target_c =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();

		let hash_a = get_contract(&target_a.addr).code_hash;
		let hash_c = get_contract(&target_c.addr).code_hash;
		assert_ne!(hash_a, hash_c);

		let refcount_a_before = CodeInfoOf::<Test>::get(hash_a).unwrap().refcount();
		let refcount_c_before = CodeInfoOf::<Test>::get(hash_c).unwrap().refcount();

		let authority = H160::from([0x11; 20]);
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		// Step 1: delegate to contract A — charges deposit, increments refcount
		let deposit_a = AccountInfo::<Test>::set_delegation(&authority, target_a.addr).unwrap();
		let charge_a = match deposit_a {
			crate::StorageDeposit::Charge(d) => d,
			other => panic!("expected Charge, got {other:?}"),
		};
		assert!(charge_a > 0, "delegation to contract should charge a deposit");
		assert_eq!(CodeInfoOf::<Test>::get(hash_a).unwrap().refcount(), refcount_a_before + 1);

		// Step 2: re-delegate to a plain EOA — refunds the full deposit, decrements refcount
		let plain_eoa = H160::from([0x77; 20]);
		let deposit_eoa = AccountInfo::<Test>::set_delegation(&authority, plain_eoa).unwrap();
		assert_eq!(
			deposit_eoa,
			crate::StorageDeposit::Refund(charge_a),
			"re-delegating to EOA should refund the full deposit from step 1"
		);
		assert_eq!(
			CodeInfoOf::<Test>::get(hash_a).unwrap().refcount(),
			refcount_a_before,
			"A's refcount should be back to original after re-delegating to EOA"
		);

		// Step 3: re-delegate to contract C — charges a fresh deposit, must NOT touch A
		let deposit_c = AccountInfo::<Test>::set_delegation(&authority, target_c.addr).unwrap();
		let charge_c = match deposit_c {
			crate::StorageDeposit::Charge(d) => d,
			other => panic!("expected Charge, got {other:?}"),
		};
		assert!(charge_c > 0, "delegation to contract should charge a deposit");
		assert_eq!(
			CodeInfoOf::<Test>::get(hash_a).unwrap().refcount(),
			refcount_a_before,
			"A's refcount must not be decremented again"
		);
		assert_eq!(
			CodeInfoOf::<Test>::get(hash_c).unwrap().refcount(),
			refcount_c_before + 1,
			"C's refcount should be incremented"
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

		let deposit = AccountInfo::<Test>::set_delegation(&authority, plain_eoa).unwrap();

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

/// Per EIP-7702: delegating to a precompile (which surfaces in `AccountInfoOf` as
/// `AccountType::Contract` with `code_hash == 0` and no `CodeInfo`) must succeed and
/// behave as empty code on call. Without filtering zero-hashes, `increment_refcount`
/// would fail with `CodeNotFound` and the auth would be silently skipped — a spec
/// deviation since the spec requires the delegation to apply.
#[test]
fn set_delegation_to_zero_hash_contract_succeeds() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x77; 20]);
		let precompile_like = H160::from([0x88; 20]);
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		// Seed the target as a Contract with zero code_hash (precompile-style).
		let zero_info = crate::storage::ContractInfo::<Test>::new_for_delegation(
			&precompile_like,
			H256::zero(),
		);
		crate::AccountInfoOf::<Test>::insert(
			precompile_like,
			AccountInfo { account_type: crate::storage::AccountType::Contract(zero_info), dust: 0 },
		);

		// Delegation must succeed (no `CodeNotFound` propagating out of refcount bump).
		let deposit = AccountInfo::<Test>::set_delegation(&authority, precompile_like).unwrap();

		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(precompile_like));
		assert!(deposit.is_zero(), "zero-code target should not charge a code-lockup deposit");
	});
}

/// EOA → DelegatedEOA transition must preserve the account's existing `dust` field —
/// a `set_delegation` should only touch the account_type / contract_info, not silently
/// drop sub-ratio dust the user had accumulated.
#[test]
fn set_delegation_preserves_dust_on_eoa_transition() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x55; 20]);
		let authority_id = <Test as Config>::AddressMapper::to_address(
			&<Test as Config>::AddressMapper::to_account_id(&authority),
		);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(
			&<Test as Config>::AddressMapper::to_account_id(&authority),
			100_000_000,
		);

		// Seed a non-zero `dust` on the EOA's `AccountInfoOf` entry.
		crate::AccountInfoOf::<Test>::insert(
			authority,
			AccountInfo { account_type: crate::storage::AccountType::EOA, dust: 7 },
		);
		assert_eq!(crate::AccountInfoOf::<Test>::get(&authority).unwrap().dust, 7);

		AccountInfo::<Test>::set_delegation(&authority, H160::from([0x66; 20])).unwrap();

		// Dust must survive the transition.
		let after = crate::AccountInfoOf::<Test>::get(&authority).unwrap();
		assert_eq!(after.dust, 7, "set_delegation must not zero existing dust");
		assert!(matches!(
			after.account_type,
			crate::storage::AccountType::DelegatedEOA { delegate_target: Some(_), .. }
		));
		let _ = authority_id; // suppress unused
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
		let deposit_a = AccountInfo::<Test>::set_delegation(&authority_a, target.addr).unwrap();
		let deposit_b = AccountInfo::<Test>::set_delegation(&authority_b, target.addr).unwrap();

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

/// Self-sponsored authorization: tx signer and auth signer are the same account.
///
/// Per EIP-7702 spec: the tx-level nonce bump happens before authorization processing,
/// so when authority == tx_signer the auth's nonce field must already account for that
/// bump (i.e. `auth.nonce = pre_tx_nonce + 1`).
#[test]
fn self_sponsored_authorization_works() {
	ExtBuilder::default().build().execute_with(|| {
		let setup = DelegationTestSetup::new([0xAB; 32]);
		let target = H160::from([0x42; 20]);

		// Simulate the tx nonce bump that would happen before auth processing
		// when the auth signer is also the tx signer.
		frame_system::Pallet::<Test>::inc_account_nonce(&setup.authority_id);
		let bumped_nonce = setup.nonce();
		assert_eq!(bumped_nonce, U256::one());

		// Sign with the bumped nonce.
		let auth = setup.signer.sign_authorization(setup.chain_id, target, bumped_nonce);

		// Process using the authority itself as the origin (self-sponsored).
		let result = crate::evm::eip7702::process_authorizations::<Test>(
			&[auth],
			&setup.authority_id,
			&setup.exec_config,
		);

		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&setup.signer.address), Some(target));
		// Auth processing bumps the nonce again: pre-tx 0 → after-tx-bump 1 → after-auth 2.
		assert_eq!(frame_system::Pallet::<Test>::account_nonce(&setup.authority_id), 2);
		assert_eq!(result.existing_accounts, 1);
	});
}

/// Refcount overflow in `increment_refcount` is a post-validation failure: per spec the
/// individual auth must be skipped while the rest of the list (and the transaction
/// itself) continues. Engineered by force-setting the target's `CodeInfo::refcount`
/// to `u64::MAX` so the next bump overflows.
#[test]
fn refcount_overflow_skips_auth_without_aborting() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let bad_target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();
		let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();
		let good_target =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();

		// Force the bad target's code refcount to u64::MAX so increment_refcount overflows.
		let bad_hash = get_contract(&bad_target.addr).code_hash;
		CodeInfoOf::<Test>::mutate(bad_hash, |maybe| {
			maybe.as_mut().unwrap().set_refcount(u64::MAX);
		});

		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let setup_bad = DelegationTestSetup::new([0xA1; 32]);
		let setup_good = DelegationTestSetup::new([0xA2; 32]);
		let auth_bad = setup_bad.sign_authorization(bad_target.addr);
		let auth_good =
			setup_good.signer.sign_authorization(chain_id, good_target.addr, U256::zero());

		// Mustn't propagate the refcount error — it's a per-auth skip.
		let result = setup_bad.process(&[auth_bad, auth_good]);

		// Bad auth: not applied, nonce not bumped (rolled back by per-auth with_transaction).
		assert!(!AccountInfo::<Test>::is_delegated(&setup_bad.signer.address));
		assert_eq!(frame_system::Pallet::<Test>::account_nonce(&setup_bad.authority_id), 0);

		// Good auth: applied.
		assert!(AccountInfo::<Test>::is_delegated(&setup_good.signer.address));
		assert_eq!(frame_system::Pallet::<Test>::account_nonce(&setup_good.authority_id), 1);

		assert_eq!(result.existing_accounts, 1);
		assert_eq!(result.new_accounts, 0);
	});
}

/// EIP-7702 spec step 3 second-half (low-s enforcement) is intentionally NOT enforced
/// on revive.
/// This test asserts the current deviation: an authorization with high-s recovers to
/// the same address and is processed normally (delegation applied, nonce bumped),
/// instead of being skipped or invalidating the whole transaction.
#[test]
fn high_s_authorization_is_not_rejected() {
	ExtBuilder::default().build().execute_with(|| {
		let setup = DelegationTestSetup::new([0xCC; 32]);
		let target = H160::from([0x42; 20]);

		let auth = setup.sign_authorization(target);

		// secp256k1 group order N.
		let secp256k1_n = U256::from_str_radix(
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141",
			16,
		)
		.unwrap();

		// Map (r, s, y_parity) → (r, N - s, !y_parity) — the high-s twin that recovers
		// to the same address. The library produces low-s by convention, so this is the
		// non-canonical form a spec-compliant client would reject.
		let high_s_auth = AuthorizationListEntry {
			s: secp256k1_n - auth.s,
			y_parity: if auth.y_parity.is_zero() { U256::one() } else { U256::zero() },
			..auth
		};
		assert!(high_s_auth.s > secp256k1_n / 2, "manipulated s must actually be high-s");

		let result = setup.process(&[high_s_auth]);

		// Current behavior: high-s sig is accepted, delegation applied.
		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&setup.signer.address), Some(target));
		assert_eq!(result.existing_accounts, 1);
	});
}

/// Spec-compliant behavior for an EIP-7702 transaction whose destination is the
/// `RUNTIME_PALLETS_ADDR` substrate-dispatch routing address: the auths must still be
/// processed before the substrate call dispatches.
///
/// Currently we reject this combination at validation time (see
/// `check_eth_transact_7702_rejects_runtime_pallets_addr`) because `eth_substrate_call`
/// has no `authorization_list` field. Activating this test requires:
///  1. Dropping that rejection check.
///  2. Plumbing the auth list through `eth_substrate_call` and processing it before dispatching the
///     inner pallet call.
#[test]
#[ignore = "spec-compliant EIP-7702 + RUNTIME_PALLETS_ADDR requires plumbing the auth list through eth_substrate_call"]
fn runtime_pallet_call_still_processes_authorizations() {
	// Placeholder for the spec-compliant test:
	// 1. Build an EIP-7702 tx with `to = RUNTIME_PALLETS_ADDR`, data = encoded
	//    `frame_system::Call::remark { remark }`, and a valid authorization_list.
	// 2. Submit via eth_transact.
	// 3. Assert the auth signer ends up delegated to the auth's target.
	// 4. Assert the remark event was emitted (substrate call also dispatched).
}

/// Snapshot-vs-call-time deviation: if code is deployed at the delegation target *after*
/// the authorization is processed, spec-compliant clients resolve the new code at call
/// time. Our snapshot-at-delegation model misses it. Same root cause as the chained-
/// delegation and precompile-target deviations; the proper fix is call-time resolution.
#[test]
#[ignore = "snapshot-vs-call-time deviation; fix is call-time resolution"]
fn late_deploy_to_delegation_target_resolves_code() {
	let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		// Pre-compute the CREATE2 address where Counter *will* be deployed.
		let setup = DelegationTestSetup::new([0xDD; 32]);
		let salt = [0xEE; 32];
		let target = crate::address::create2(&ALICE_ADDR, &counter_code, &[], &salt);

		// Alice delegates to `target` while it still has no code.
		setup.authorize(target);
		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));

		// Deploy Counter at the pre-computed address.
		let deployed = builder::bare_instantiate(Code::Upload(counter_code))
			.salt(Some(salt))
			.build_and_unwrap_contract();
		assert_eq!(deployed.addr, target);

		// Spec-correct: calling the authority should now execute Counter's code.
		let result = builder::bare_call(setup.signer.address)
			.data(Counter::setNumberCall { newNumber: 99u64 }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "spec-correct: late-deployed target should execute");

		let read = builder::bare_call(setup.signer.address)
			.data(Counter::numberCall {}.abi_encode())
			.build_and_unwrap_result();
		assert_eq!(Counter::numberCall::abi_decode_returns(&read.data).unwrap(), 99u64);
	});
}

/// EIP-7702 op-code coverage on a delegated EOA, Solc execution path.
///
/// Two families:
/// - **EXT*** (external introspection of `delegated_eoa.code`): must surface the 23-byte indicator
///   `0xef0100 || target` — length 23, hash `keccak256(indicator)`.
/// - **CODE*** (self introspection from inside the delegated EOA's execution): must surface the
///   *target's* code, not the indicator.
///
/// The Resolc/PVM analogue of this test for the CODE* family is `delegated_eoa_pvm_*`
/// below, which is `#[ignore]`'d because resolc currently lowers both opcode families
/// through the same address-keyed host functions.
#[test]
fn delegated_eoa_opcodes_return_correct_data_solc() {
	let (host_code, _) = compile_module_with_type("Host", FixtureType::Solc).unwrap();
	let (system_code, _) = compile_module_with_type("System", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000_000);

		// Host is the external probe (EXTCODESIZE / EXTCODEHASH). System is the
		// delegation target (its CODESIZE returns the executing code's size).
		let host = builder::bare_instantiate(Code::Upload(host_code)).build_and_unwrap_contract();
		let system = builder::bare_instantiate(Code::Upload(system_code))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, system.addr).unwrap();

		// ---- EXT* family ----

		// EXTCODESIZE(alice) == 23 (length of `0xef0100 || target`).
		let result = builder::bare_call(host.addr)
			.data(Host::extcodesizeOpCall { account: ALICE_ADDR.0.into() }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());
		assert_eq!(Host::extcodesizeOpCall::abi_decode_returns(&result.data).unwrap(), 23);

		// EXTCODEHASH(alice) == keccak256(indicator).
		let mut indicator = vec![0xefu8, 0x01, 0x00];
		indicator.extend_from_slice(system.addr.as_bytes());
		let expected_hash = sp_io::hashing::keccak_256(&indicator);
		let result = builder::bare_call(host.addr)
			.data(Host::extcodehashOpCall { account: ALICE_ADDR.0.into() }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());
		assert_eq!(
			Host::extcodehashOpCall::abi_decode_returns(&result.data).unwrap().0,
			expected_hash,
		);

		// ---- CODE* family inside the delegated EOA's execution context ----

		// CODESIZE inside Alice's execution must equal the target's CODESIZE.
		// (EVM's native CODESIZE sees the executing bytecode, which is System's.)
		let alice_codesize = {
			let r = builder::bare_call(ALICE_ADDR)
				.data(SystemFixture::codesizeCall {}.abi_encode())
				.build_and_unwrap_result();
			assert!(!r.did_revert());
			SystemFixture::codesizeCall::abi_decode_returns(&r.data).unwrap()
		};
		let system_codesize = {
			let r = builder::bare_call(system.addr)
				.data(SystemFixture::codesizeCall {}.abi_encode())
				.build_and_unwrap_result();
			assert!(!r.did_revert());
			SystemFixture::codesizeCall::abi_decode_returns(&r.data).unwrap()
		};
		assert_eq!(
			alice_codesize, system_codesize,
			"CODESIZE inside delegated EOA must match the target's CODESIZE",
		);
		assert_ne!(alice_codesize, 23, "CODESIZE must not be the indicator length");
	});
}

/// Resolc/PVM analogue of `delegated_eoa_opcodes_return_correct_data_solc` for the
/// CODE* family. Currently `#[ignore]`'d: resolc lowers `CODESIZE` (and `CODECOPY`)
/// through the address-keyed `code_size`/`copy_code_slice` host functions, which
/// surface the indicator for delegated EOAs instead of the target's bytecode.
/// Will pass once `own_code_size`/`own_code_copy` host fns land and resolc routes
/// CODESIZE/CODECOPY through them.
#[test]
#[ignore = "PVM CODE* family currently conflates self/external code paths"]
fn delegated_eoa_codesize_inside_execution_resolc() {
	let (system_code, _) = compile_module_with_type("System", FixtureType::Resolc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000_000);
		let system = builder::bare_instantiate(Code::Upload(system_code))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, system.addr).unwrap();

		let alice_codesize = {
			let r = builder::bare_call(ALICE_ADDR)
				.data(SystemFixture::codesizeCall {}.abi_encode())
				.build_and_unwrap_result();
			SystemFixture::codesizeCall::abi_decode_returns(&r.data).unwrap()
		};
		let system_codesize = {
			let r = builder::bare_call(system.addr)
				.data(SystemFixture::codesizeCall {}.abi_encode())
				.build_and_unwrap_result();
			SystemFixture::codesizeCall::abi_decode_returns(&r.data).unwrap()
		};
		assert_eq!(alice_codesize, system_codesize, "spec-correct: CODESIZE returns target size");
		assert_ne!(alice_codesize, 23, "CODESIZE must not be the indicator length");
	});
}

/// An EOA that delegates to itself forms a one-element cycle. The same chain-detection
/// logic that catches `bob -> alice -> alice` catches this: the authority's delegation
/// target is itself a delegated EOA, so the call reverts on the indicator `0xef` byte
/// rather than executing endlessly or no-op'ing.
#[test]
fn self_delegation_reverts_on_call() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, ALICE_ADDR).unwrap();
		assert!(AccountInfo::<Test>::is_delegated(&ALICE_ADDR));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&ALICE_ADDR), Some(ALICE_ADDR));

		let result = builder::bare_call(ALICE_ADDR)
			.data(vec![0xDE, 0xAD, 0xBE, 0xEF])
			.build_and_unwrap_result();
		assert!(result.did_revert(), "self-delegation should revert (chain cycle, 0xef trap)");
	});
}
