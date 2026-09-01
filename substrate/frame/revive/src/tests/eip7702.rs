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
	Code, CodeInfoOf, Config, ExecConfig, HoldReason,
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

/// Compute the expected weight refund for a given mix of new/existing/invalid accounts.
/// Mirrors the logic in `process_authorizations`: invalid tuples are billed for the
/// signature recovery they incurred via `process_invalid_authorization`.
fn expected_weight_refund_for(total: u32, new_accounts: u32, existing_accounts: u32) -> Weight {
	let invalid = total.saturating_sub(new_accounts).saturating_sub(existing_accounts);
	let worst = crate::evm::eip7702::worst_case_authorization_weight::<Test>(total);
	let actual = <Test as Config>::WeightInfo::process_new_account_authorization(new_accounts)
		.saturating_add(<Test as Config>::WeightInfo::process_existing_account_authorization(
			existing_accounts,
		))
		.saturating_add(<Test as Config>::WeightInfo::process_invalid_authorization(invalid));
	worst.saturating_sub(actual)
}

fn expected_weight_refund(new_accounts: u32, existing_accounts: u32) -> Weight {
	expected_weight_refund_for(new_accounts + existing_accounts, new_accounts, existing_accounts)
}

/// Common setup for delegation tests that call `process_authorizations` directly.
pub struct DelegationTestSetup {
	pub signer: TestSigner,
	pub authority_id: AccountId32,
	pub origin: AccountId32,
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
			deposit: crate::StorageDeposit::Charge(0),
			weight_refund: expected_weight_refund(0, 1),
		};

		// Valid signature → delegated, nonce incremented
		let setup = DelegationTestSetup::new([1u8; 32]);
		let nonce_before = frame_system::Pallet::<Test>::account_nonce(&setup.authority_id);
		let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);
		let auth = setup.sign_authorization(target);
		assert_eq!(setup.process(&[auth]), existing_one);
		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&setup.signer.address), Some(target));
		assert_eq!(
			frame_system::Pallet::<Test>::account_nonce(&setup.authority_id),
			nonce_before + 1
		);
		// Target is non-contract → no storage-deposit hold, origin's free balance untouched.
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			0,
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin),
			origin_before,
		);

		// chain_id = 0 (wildcard) is accepted
		let setup = DelegationTestSetup::new([2u8; 32]);
		let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);
		let auth = setup.signer.sign_authorization(U256::zero(), target, setup.nonce());
		assert_eq!(setup.process(&[auth]), existing_one);
		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&setup.signer.address), Some(target));
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			0,
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin),
			origin_before,
		);
	});
}

#[test]
fn invalid_authorization_is_skipped() {
	ExtBuilder::default().build().execute_with(|| {
		let target = H160::from([0x42; 20]);
		let skipped = AuthorizationResult {
			existing_accounts: 0,
			new_accounts: 0,
			deposit: crate::StorageDeposit::Charge(0),
			weight_refund: expected_weight_refund_for(1, 0, 0),
		};

		// A skipped authorization must leave both origin and authority financially untouched —
		// no hold placed, no balance moved. Bundling this with the skip-path assertions
		// rather than relying on the `Charge(0)` return alone.
		let assert_no_money_moved = |setup: &DelegationTestSetup, origin_before: u128| {
			assert_eq!(
				get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id,),
				0,
				"skipped auth must not leave a hold on the authority",
			);
			assert_eq!(
				<<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin),
				origin_before,
				"skipped auth must not move origin's free balance",
			);
		};

		// Wrong chain_id
		let setup = DelegationTestSetup::new([1u8; 32]);
		let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);
		let auth = setup.signer.sign_authorization(U256::from(999), target, setup.nonce());
		assert_eq!(setup.process(&[auth]), skipped);
		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_no_money_moved(&setup, origin_before);

		// Wrong nonce
		let setup = DelegationTestSetup::new([2u8; 32]);
		let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);
		let wrong_nonce = setup.nonce().saturating_add(U256::from(1));
		let auth = setup.signer.sign_authorization(setup.chain_id, target, wrong_nonce);
		assert_eq!(setup.process(&[auth]), skipped);
		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_no_money_moved(&setup, origin_before);

		// Corrupted signature
		let setup = DelegationTestSetup::new([3u8; 32]);
		let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);
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
		assert_no_money_moved(&setup, origin_before);

		// `y_parity` outside `{0, 1}` must be skipped per EIP-7702. In particular `27`/`28`
		// (the legacy Bitcoin/pre-EIP-155 v convention) would silently normalise to `0`/`1`
		// inside `sp_io::crypto::secp256k1_ecdsa_recover` if we let them through, so the
		// per-tuple skip has to happen *before* recovery.
		for bad in [U256::from(2u32), U256::from(27u32), U256::from(28u32)] {
			let setup = DelegationTestSetup::new([0x11; 32]);
			let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);
			let mut auth = setup.sign_authorization(target);
			auth.y_parity = bad;
			assert_eq!(setup.process(&[auth]), skipped, "y_parity={bad:?} should be skipped");
			assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));
			assert_no_money_moved(&setup, origin_before);
		}
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

		let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);

		// Authorization should be skipped because the authority is a contract
		assert_eq!(
			setup.process(&[auth]),
			AuthorizationResult {
				existing_accounts: 0,
				new_accounts: 0,
				deposit: crate::StorageDeposit::Charge(0),
				weight_refund: expected_weight_refund_for(1, 0, 0)
			}
		);

		// Account should still be a contract, not delegated
		assert!(AccountInfo::<Test>::is_contract(&setup.signer.address));
		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));

		// Skipping must not place a hold or move the origin's free balance.
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			0,
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin),
			origin_before,
		);
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

		let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);

		assert_eq!(
			setup.process(&[auth1, auth2, auth3]),
			AuthorizationResult {
				existing_accounts: 1,
				new_accounts: 0,
				deposit: crate::StorageDeposit::Charge(0),
				weight_refund: expected_weight_refund_for(3, 0, 1)
			},
		);

		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&setup.signer.address),
			Some(target1)
		);

		// Only one delegation committed (to `target1`, a non-contract) so no storage hold,
		// and the two skipped tuples must not have moved origin's free balance.
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			0,
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin),
			origin_before,
		);
	});
}

#[test]
fn new_account_sets_delegation() {
	ExtBuilder::default().build().execute_with(|| {
		let setup = DelegationTestSetup::new_unfunded([1u8; 32]);
		let target = H160::from([0x42; 20]);
		let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);

		let auth = setup.sign_authorization(target);

		assert_eq!(
			setup.process(&[auth]),
			AuthorizationResult {
				existing_accounts: 0,
				new_accounts: 1,
				deposit: crate::StorageDeposit::Charge(1),
				weight_refund: expected_weight_refund(1, 0)
			},
		);

		assert!(AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&setup.signer.address), Some(target));
		let balance = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.authority_id);
		assert_eq!(balance, Pallet::<Test>::min_balance());
		// Non-contract target → no storage-deposit hold (only the ED transfer).
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			0,
			"non-contract target must not produce a storage-deposit hold",
		);
		// Under `new_eth_tx`, deposits/EDs flow through the tx-fee pot, never origin's free
		// balance.
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin),
			origin_before,
			"origin's free balance must be unchanged",
		);
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
				deposit: crate::StorageDeposit::Charge(0),
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
				deposit: crate::StorageDeposit::Charge(0),
				weight_refund: expected_weight_refund(0, 1)
			},
		);

		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&setup.signer.address), None);

		// Target was a non-contract → set+clear must produce no net hold and no movement of
		// origin's free balance. A misrouted refund of the zero deposit would still surface
		// as a balance delta on `origin` if any phantom charge leaked through.
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			0,
		);
	});
}

/// The deposit refund on clear must go to the account that paid when the delegation was set,
/// not to whichever relayer happens to submit the clear. Authorizations are signed by the
/// authority and may be relayed by anyone, so the set and clear submitters can differ.
///
/// The refund is routed via `exec_config.funds(old_payer)` to the recorded payer rather than to
/// `origin` (the clear submitter), so the original payer is made whole and the clearer gains
/// nothing.
#[test]
fn refund_routes_to_set_payer_not_clear_submitter() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));

		// Deploy a real contract so the delegation carries a non-zero base deposit.
		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		// Two distinct relayers, both funded.
		let set_payer = <Test as Config>::AddressMapper::to_account_id(&H160::from([0xAA; 20]));
		let clear_submitter =
			<Test as Config>::AddressMapper::to_account_id(&H160::from([0xBB; 20]));
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&set_payer, 100_000_000);
		let _ =
			<<Test as Config>::Currency as Mutate<_>>::set_balance(&clear_submitter, 100_000_000);

		// Authority is pre-funded so the ED path doesn't perturb relayer balances.
		let signer = TestSigner::new(&[1u8; 32]);
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&signer.address);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		let chain_id = U256::from(<Test as Config>::ChainId::get());
		// `Funds::Balance` mode: deposit flows directly between relayer balances, so the
		// routing on refund is observable. (eth-tx dispatch uses `Funds::TxFee`, which hides
		// the per-account leak under the native backend by routing through the fee pool.)
		let exec_config = ExecConfig::new_substrate_tx();

		let set_payer_before = <<Test as Config>::Currency as Inspect<_>>::balance(&set_payer);
		let clear_submitter_before =
			<<Test as Config>::Currency as Inspect<_>>::balance(&clear_submitter);

		// Step 1: `set_payer` relays the authority's set-authorization. The deposit is
		// transferred from `set_payer` to the authority's held balance.
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth_set = signer.sign_authorization(chain_id, target.addr, nonce);
		let _ = crate::evm::eip7702::process_authorizations::<Test>(
			&[auth_set],
			&set_payer,
			&exec_config,
		);
		assert!(AccountInfo::<Test>::is_delegated(&signer.address));
		let hold = get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id);
		assert!(
			hold > 0,
			"delegation must carry a non-zero deposit for this test to mean anything"
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&set_payer),
			set_payer_before - hold,
			"set_payer should have paid the deposit",
		);

		// Step 2: a *different* relayer submits the authority's signed clear. The payer-change
		// path refunds the held deposit to `set_payer` (the recorded payer) rather than to
		// `clear_submitter`, so under the mock runtime's `PGasDeposit` backend the
		// `NativeDepositOf[(authority, set_payer)]` lookup resolves and the hold is fully
		// released to the correct account.
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth_clear = signer.sign_authorization(chain_id, H160::zero(), nonce);
		let clear_result = crate::evm::eip7702::process_authorizations::<Test>(
			&[auth_clear],
			&clear_submitter,
			&exec_config,
		);
		assert!(!AccountInfo::<Test>::is_delegated(&signer.address));

		// `clear_submitter` paid nothing and the refund went to a *different* account
		// (`set_payer`), so the deposit reported to `clear_submitter`'s metering budget must be a
		// net-zero charge — not `Refund(hold)`. A cross-account refund here would credit
		// `clear_submitter`'s EVM deposit budget with money it never paid.
		assert_eq!(
			clear_result.deposit,
			crate::StorageDeposit::Charge(0),
			"payer-change clear must not credit the clear submitter's deposit budget",
		);
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id),
			0,
			"hold must be fully released after clear",
		);

		// The correctness invariant: refund must restore the original payer, and the
		// unrelated clear-submitter must not gain anything.
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&set_payer),
			set_payer_before,
			"set_payer should be made whole on clear",
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&clear_submitter),
			clear_submitter_before,
			"clear_submitter should not pocket the deposit",
		);
	});
}

/// Under eth-tx, a payer-change refund must reach the recorded payer, not the fee pot: the
/// `Funds::TxFee` rail drops the recipient and returns the deposit to the pot (→ the current
/// submitter), so the refund must go out as a `Funds::Balance` transfer to `old_payer`.
/// Exercises both payer-change refund legs — re-delegation (`current > 0`) and clear
/// (`current == 0`) — and the moving recorded-payer across them, asserting the refund lands on
/// the right account and the pot never recovers it. (Full cross-tx loss needs two separate gas
/// pools, out of scope here.)
#[test]
fn eth_tx_payer_change_refund_reaches_recorded_payer() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		// Seed the fee pot so eth-tx charges have somewhere to draw from.
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));

		// Two targets sharing the same code → same base deposit, so re-delegating between them
		// keeps `previous == current` and isolates the payer routing.
		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();
		let target_b = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.salt(Some([1; 32]))
			.build_and_unwrap_contract();
		let deposit = get_contract(&target.addr).storage_base_deposit();
		assert!(deposit > 0, "delegation target must carry a non-zero base deposit");

		// Three distinct relayers and a pre-funded authority (ED path must not perturb the pot).
		let set_payer = <Test as Config>::AddressMapper::to_account_id(&H160::from([0xAA; 20]));
		let redelegate_payer =
			<Test as Config>::AddressMapper::to_account_id(&H160::from([0xCC; 20]));
		let clear_submitter =
			<Test as Config>::AddressMapper::to_account_id(&H160::from([0xBB; 20]));
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&set_payer, 100_000_000);
		let _ =
			<<Test as Config>::Currency as Mutate<_>>::set_balance(&redelegate_payer, 100_000_000);
		let _ =
			<<Test as Config>::Currency as Mutate<_>>::set_balance(&clear_submitter, 100_000_000);
		let signer = TestSigner::new(&[1u8; 32]);
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&signer.address);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		let chain_id = U256::from(<Test as Config>::ChainId::get());
		// Production config: deposits flow through the tx-fee pot (`Funds::TxFee`).
		let eth_tx = ExecConfig::new_eth_tx(U256::from(1), 0, Weight::MAX);

		let set_payer_before = <<Test as Config>::Currency as Inspect<_>>::balance(&set_payer);
		let redelegate_payer_before =
			<<Test as Config>::Currency as Inspect<_>>::balance(&redelegate_payer);
		let clear_submitter_before =
			<<Test as Config>::Currency as Inspect<_>>::balance(&clear_submitter);

		// Step 1: `set_payer` relays the set. Deposit drawn from the pot, payer recorded.
		let pot_before = <Test as Config>::FeeInfo::remaining_txfee();
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth_set = signer.sign_authorization(chain_id, target.addr, nonce);
		let _ =
			crate::evm::eip7702::process_authorizations::<Test>(&[auth_set], &set_payer, &eth_tx);
		assert!(AccountInfo::<Test>::is_delegated(&signer.address));
		let pot_after_set = <Test as Config>::FeeInfo::remaining_txfee();
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&set_payer),
			set_payer_before,
			"eth-tx charge is drawn from the fee pot, not the payer's free balance",
		);
		assert_eq!(
			pot_before.saturating_sub(deposit),
			pot_after_set,
			"the deposit must have been drawn from the fee pot",
		);
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id),
			deposit,
			"hold must equal the deposit after delegation",
		);

		// Step 2: a *different* relayer re-delegates to `target_b` (`current > 0`). The
		// payer-change refund of `previous` must reach `set_payer`'s balance — not the pot —
		// while the new charge for `current` is drawn from the pot against `redelegate_payer`.
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth_redelegate = signer.sign_authorization(chain_id, target_b.addr, nonce);
		let _ = crate::evm::eip7702::process_authorizations::<Test>(
			&[auth_redelegate],
			&redelegate_payer,
			&eth_tx,
		);
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&signer.address),
			Some(target_b.addr),
		);
		let pot_after_redelegate = <Test as Config>::FeeInfo::remaining_txfee();
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&set_payer),
			set_payer_before + deposit,
			"re-delegation must refund `previous` to set_payer's balance, not the pot",
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&redelegate_payer),
			redelegate_payer_before,
			"the new charge is drawn from the pot, not redelegate_payer's free balance",
		);
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id),
			deposit,
			"hold stays one deposit after same-size re-delegation",
		);
		assert_eq!(
			pot_after_redelegate,
			pot_after_set.saturating_sub(deposit),
			"only the new charge hit the pot; the refund of `previous` bypassed it",
		);

		// Step 3: a *third* relayer clears (`current == 0`). The refund must now reach the most
		// recent payer (`redelegate_payer`), again via balance, never the pot or the submitter.
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth_clear = signer.sign_authorization(chain_id, H160::zero(), nonce);
		let _ = crate::evm::eip7702::process_authorizations::<Test>(
			&[auth_clear],
			&clear_submitter,
			&eth_tx,
		);
		assert!(!AccountInfo::<Test>::is_delegated(&signer.address));
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id),
			0,
			"hold must be fully released after clear",
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&redelegate_payer),
			redelegate_payer_before + deposit,
			"clear must refund the current recorded payer (redelegate_payer) to its balance",
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&set_payer),
			set_payer_before + deposit,
			"set_payer must not be refunded twice — it was made whole at re-delegation",
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&clear_submitter),
			clear_submitter_before,
			"the clear submitter must not pocket the refund",
		);
		assert_eq!(
			<Test as Config>::FeeInfo::remaining_txfee(),
			pot_after_redelegate,
			"the clear refund must bypass the pot (it goes to redelegate_payer's balance)",
		);
	});
}

/// Payer-change *re-delegation* (both deposit legs non-zero): when a second relayer re-points an
/// existing delegation, the old payer must be fully refunded, the new submitter must be charged
/// the new deposit in full, and the net deposit reported to the new submitter's metering budget
/// must be exactly `Charge(current)` — never the cross-account `current - previous` diff.
///
/// With `previous == current`, a cross-account `current - previous` net would report
/// `Charge(0)`, handing the new submitter a full deposit's worth of free EVM budget. The
/// clear-path test above only exercises `current == 0`.
#[test]
fn payer_change_redelegation_charges_new_payer_in_full() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));

		// Two distinct contracts sharing the same code, hence the same base deposit. Re-pointing
		// between them keeps `previous == current`, isolating the metering net from any deposit
		// delta so `Charge(current)` (correct) is distinguishable from `Charge(0)`.
		let target_a = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();
		let target_b = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.salt(Some([1; 32]))
			.build_and_unwrap_contract();
		let deposit = get_contract(&target_a.addr).storage_base_deposit();
		assert!(deposit > 0, "delegation target must carry a non-zero base deposit");

		// Two distinct relayers, both funded.
		let set_payer = <Test as Config>::AddressMapper::to_account_id(&H160::from([0xAA; 20]));
		let redelegate_payer =
			<Test as Config>::AddressMapper::to_account_id(&H160::from([0xBB; 20]));
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&set_payer, 100_000_000);
		let _ =
			<<Test as Config>::Currency as Mutate<_>>::set_balance(&redelegate_payer, 100_000_000);

		// Authority is pre-funded so the ED path doesn't perturb relayer balances.
		let signer = TestSigner::new(&[2u8; 32]);
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&signer.address);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		let chain_id = U256::from(<Test as Config>::ChainId::get());
		// `Funds::Balance` mode so the deposit moves directly between relayer balances and the
		// routing is observable.
		let exec_config = ExecConfig::new_substrate_tx();

		let set_payer_before = <<Test as Config>::Currency as Inspect<_>>::balance(&set_payer);
		let redelegate_payer_before =
			<<Test as Config>::Currency as Inspect<_>>::balance(&redelegate_payer);

		// Step 1: `set_payer` relays the initial delegation to target A.
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth_a = signer.sign_authorization(chain_id, target_a.addr, nonce);
		let _ = crate::evm::eip7702::process_authorizations::<Test>(
			&[auth_a],
			&set_payer,
			&exec_config,
		);
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&signer.address),
			Some(target_a.addr)
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&set_payer),
			set_payer_before - deposit,
			"set_payer should have paid the full deposit",
		);

		// Step 2: a *different* relayer re-points the delegation to target B.
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth_b = signer.sign_authorization(chain_id, target_b.addr, nonce);
		let redelegate_result = crate::evm::eip7702::process_authorizations::<Test>(
			&[auth_b],
			&redelegate_payer,
			&exec_config,
		);
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&signer.address),
			Some(target_b.addr)
		);

		// Core invariant: the new submitter is metered for exactly what it paid, not the
		// cross-account `current - previous` (which would be `Charge(0)` here).
		assert_eq!(
			redelegate_result.deposit,
			crate::StorageDeposit::Charge(deposit),
			"re-delegation must charge the new payer's budget in full, not a cross-account net",
		);
		// The hold on the authority is unchanged in size (same deposit, new payer).
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id),
			deposit,
			"hold must still equal one base deposit after re-delegation",
		);
		// Old payer is made whole; new payer pays exactly one deposit; neither over- nor
		// under-charged.
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&set_payer),
			set_payer_before,
			"set_payer should be fully refunded on re-delegation",
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&redelegate_payer),
			redelegate_payer_before - deposit,
			"redelegate_payer should be charged exactly one deposit",
		);
	});
}

/// Payer-change re-delegation where the two targets carry *different* deposits:
/// `previous != current`, so a cross-account `current - previous` net would be a non-zero (and
/// wrong) charge/refund, while the correct net is exactly `Charge(current)` charged to the new
/// payer. The old payer must be refunded its full `previous`, independent of the new deposit.
#[test]
fn payer_change_redelegation_different_deposits_charges_current() {
	let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(10_000_000_000));

		// Two targets with distinct code → distinct base deposits.
		let target_a = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();
		let target_b =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();
		let deposit_a = get_contract(&target_a.addr).storage_base_deposit();
		let deposit_b = get_contract(&target_b.addr).storage_base_deposit();
		assert!(deposit_a > 0 && deposit_b > 0, "both targets must carry a deposit");
		assert_ne!(deposit_a, deposit_b, "targets must have different deposits for this test");

		let set_payer = <Test as Config>::AddressMapper::to_account_id(&H160::from([0xAA; 20]));
		let redelegate_payer =
			<Test as Config>::AddressMapper::to_account_id(&H160::from([0xBB; 20]));
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&set_payer, 100_000_000);
		let _ =
			<<Test as Config>::Currency as Mutate<_>>::set_balance(&redelegate_payer, 100_000_000);

		let signer = TestSigner::new(&[3u8; 32]);
		let authority_id = <Test as Config>::AddressMapper::to_account_id(&signer.address);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&authority_id, 100_000_000);

		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let exec_config = ExecConfig::new_substrate_tx();

		let set_payer_before = <<Test as Config>::Currency as Inspect<_>>::balance(&set_payer);
		let redelegate_payer_before =
			<<Test as Config>::Currency as Inspect<_>>::balance(&redelegate_payer);

		// Step 1: set_payer delegates to A (deposit_a).
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth_a = signer.sign_authorization(chain_id, target_a.addr, nonce);
		let _ = crate::evm::eip7702::process_authorizations::<Test>(
			&[auth_a],
			&set_payer,
			&exec_config,
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&set_payer),
			set_payer_before - deposit_a,
		);

		// Step 2: a different relayer re-delegates to B (deposit_b != deposit_a).
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
		let auth_b = signer.sign_authorization(chain_id, target_b.addr, nonce);
		let result = crate::evm::eip7702::process_authorizations::<Test>(
			&[auth_b],
			&redelegate_payer,
			&exec_config,
		);
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&signer.address),
			Some(target_b.addr)
		);

		// Net charged to the new payer's budget is the NEW deposit in full — not the
		// cross-account diff deposit_b - deposit_a.
		assert_eq!(
			result.deposit,
			crate::StorageDeposit::Charge(deposit_b),
			"new payer must be metered for the full new deposit, not the cross-account diff",
		);
		// Hold now reflects the new target's deposit.
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id),
			deposit_b,
			"hold must equal the new target's deposit after re-delegation",
		);
		// Old payer refunded its full original deposit (deposit_a), regardless of deposit_b.
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&set_payer),
			set_payer_before,
			"old payer must be refunded its full original deposit",
		);
		// New payer charged exactly the new deposit.
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&redelegate_payer),
			redelegate_payer_before - deposit_b,
			"new payer charged exactly the new deposit",
		);
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

		let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup1.origin);

		assert_eq!(
			setup1.process(&[auth1, auth2, auth3]),
			AuthorizationResult {
				existing_accounts: 2,
				new_accounts: 1,
				deposit: crate::StorageDeposit::Charge(1),
				weight_refund: expected_weight_refund(1, 2)
			},
		);

		assert!(AccountInfo::<Test>::is_delegated(&setup1.signer.address));
		assert!(AccountInfo::<Test>::is_delegated(&setup2.signer.address));
		assert!(AccountInfo::<Test>::is_delegated(&setup3.signer.address));

		// All targets are non-contracts → no storage-deposit holds anywhere.
		for setup in [&setup1, &setup2, &setup3] {
			assert_eq!(
				get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id,),
				0,
			);
		}
		// Under `new_eth_tx`, the only `Charge(1)` (ED for setup3) flowed through the
		// tx-fee pot, not origin's free balance.
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&setup1.origin),
			origin_before,
		);
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

		let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);

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

		// Bad auth must roll back cleanly: no hold leaked onto the failed authority.
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			0,
			"failed auth must not leave a storage-deposit hold behind",
		);
		// Good auth's hold equals the deposit recorded on the authority's own delegation
		// account — the account the charge and the hold both live on. Reading the expected
		// value off the *target* would only match by coincidence (when the two `ContractInfo`s
		// happen to encode to the same size); the authority's record is the authoritative source.
		let good_deposit = get_contract(&good_signer.address).storage_base_deposit();
		assert!(good_deposit > 0, "good auth must carry a non-zero delegation deposit");
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &good_id),
			good_deposit,
			"good auth's hold must match the deposit charged to the authority",
		);
		// Net deposit reported reflects only the good auth.
		assert_eq!(result.deposit, crate::StorageDeposit::Charge(good_deposit));
		// Under `new_eth_tx`, origin's free balance is never touched directly.
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin),
			origin_before,
		);
	});
}

/// Runtime test: Set and clear authorization via eth_call.
/// Verifies delegation state, nonce increment, and deposit lifecycle.
#[test]
fn runtime_set_and_clear_authorization() {
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
fn runtime_delegation_resolution() {
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
			None,
			true,
			None,
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
			None,
			true,
			None,
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

/// A dry run carrying an authorization list but no `from` must be rejected rather than fall back
/// to the zero address. The zero-address account funds nothing, so every authorization would roll
/// back post-validation and be dropped from the estimate — an under-priced estimate that looks
/// valid.
#[test]
fn dry_run_with_authorization_list_requires_from() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000_000);

		let target_contract = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let chain_id = U256::from(<Test as Config>::ChainId::get());
		let signer = TestSigner::new(&[0xAB; 32]);
		let auth = signer.sign_authorization(chain_id, target_contract.addr, U256::zero());

		let result = crate::Pallet::<Test>::dry_run_eth_transact(
			crate::GenericTransaction {
				from: None,
				to: Some(target_contract.addr),
				authorization_list: vec![auth.clone()],
				..Default::default()
			},
			None,
			true,
			None,
		);

		assert!(
			matches!(&result, Err(crate::EthTransactError::Message(msg)) if msg.contains("`from`")),
			"expected a missing-`from` rejection, got: {result:?}"
		);
		// The rejection must happen before any delegation is written.
		assert!(!AccountInfo::<Test>::is_delegated(&signer.address));

		// The same transaction with a `from` is accepted.
		assert_ok!(crate::Pallet::<Test>::dry_run_eth_transact(
			crate::GenericTransaction {
				from: Some(ALICE_ADDR),
				to: Some(target_contract.addr),
				authorization_list: vec![auth],
				..Default::default()
			},
			None,
			true,
			None,
		));
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
			None,
			false,
			Some(overrides),
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

		// Case 4: nested-call variant. A contract calling Bob (chain-delegated) must see the
		// same revert semantics. Without the chain guard mirrored on the nested
		// `PrecompileExt::call` path, the call would fall through to the value-transfer path
		// and silently succeed as a plain EOA transfer — a different observable outcome from
		// the top-level call in Case 3.
		let value_before_caller =
			<<Test as Config>::Currency as Inspect<_>>::balance(&caller_contract.account_id);
		let value_before_bob = <<Test as Config>::Currency as Inspect<_>>::balance(&BOB);

		let result = builder::bare_call(caller_contract.addr)
			.data(
				Caller::normalCall {
					_callee: BOB_ADDR.0.into(),
					_value: 0,
					_data: Counter::setNumberCall { newNumber: CHAINED_ATTEMPT }
						.abi_encode()
						.into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "outer contract call itself shouldn't trap");
		let decoded = Caller::normalCall::abi_decode_returns(&result.data).unwrap();
		assert!(
			!decoded.success,
			"nested call into chain-delegated Bob must report failure, not silently succeed as an EOA transfer",
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&BOB),
			value_before_bob,
			"value must not have moved to Bob via the nested chain call",
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&caller_contract.account_id),
			value_before_caller,
			"caller contract's balance must be unchanged after the failed nested chain call",
		);
		assert_eq!(read_number(), ALICE_INITIAL, "Alice's storage must still be untouched");

		// Case 5: DELEGATECALL variant of the chain. A contract DELEGATECALLing Bob
		// (chain-delegated to Alice, who's herself delegated) must see the same revert
		// semantics as CALL. Per spec the resolved code is Bob's indicator bytes
		// `0xef0100||alice` and execution traps on the `0xef` invalid opcode.
		//
		// The chain-revert guard is mirrored in `Stack::delegate_call` (the third call entry
		// point), so the delegatecall reverts here just like CALL rather than silently
		// succeeding as a no-op.
		let result = builder::bare_call(caller_contract.addr)
			.data(
				Caller::delegateCall {
					_callee: BOB_ADDR.0.into(),
					_data: Counter::setNumberCall { newNumber: CHAINED_ATTEMPT }
						.abi_encode()
						.into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "outer contract call itself shouldn't trap");
		let decoded = Caller::delegateCall::abi_decode_returns(&result.data).unwrap();
		assert!(
			!decoded.success,
			"nested delegatecall into chain-delegated Bob must report failure, not silently succeed",
		);
		assert_eq!(
			read_number(),
			ALICE_INITIAL,
			"Alice's storage must still be untouched after the delegatecall chain attempt",
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

/// Calling the system precompile's `terminate` on an EIP-7702 delegated EOA must revert.
/// Distinct from `selfdestruct_on_delegated_account`, which covers the EVM `SELFDESTRUCT`
/// opcode path (`terminate_if_same_tx`); this exercises the system precompile path
/// (`terminate_caller`).
///
/// Note: only the `METHOD_PRECOMPILE` (direct call) case specifically validates the new
/// `CannotTerminateDelegatedAccount` guard. `METHOD_DELEGATE_CALL` short-circuits earlier on
/// the pre-existing `PrecompileDelegateDenied` guard (the precompile-itself-being-
/// delegate-called check). Both cases revert; only one is load-bearing for this fix.
fn assert_system_terminate_on_delegated_reverts(method: u8, ctx: &str) {
	let (code, _) = compile_module_with_type("Terminate", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		// Deploy with skip=true so the constructor doesn't selfdestruct.
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

		// Fund Alice and delegate her to the Terminate contract.
		let alice_balance = 5_000_000u128;
		let alice_id = <Test as Config>::AddressMapper::to_account_id(&ALICE_ADDR);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&alice_id, alice_balance);
		AccountInfo::<Test>::set_delegation(&ALICE_ADDR, contract.addr).unwrap();
		assert!(AccountInfo::<Test>::is_delegated(&ALICE_ADDR));

		let beneficiary = DJANGO_ADDR;
		let beneficiary_id = <Test as Config>::AddressMapper::to_account_id(&beneficiary);
		let min_balance = Contracts::min_balance();
		let _ =
			<<Test as Config>::Currency as Mutate<_>>::set_balance(&beneficiary_id, min_balance);

		let alice_balance_before = <Test as Config>::Currency::free_balance(&alice_id);
		let beneficiary_balance_before = <Test as Config>::Currency::free_balance(&beneficiary_id);

		// Both METHOD_PRECOMPILE (0) and METHOD_DELEGATE_CALL (1) end up in
		// `terminate_caller` where the `is_delegated` guard fires.
		let result = builder::bare_call(ALICE_ADDR)
			.data(
				Terminate::terminateCall { method, beneficiary: beneficiary.0.into() }.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(
			result.did_revert(),
			"system-precompile terminate on a delegated EOA must revert ({ctx})",
		);
		// Decode the revert string. `try_to_revert` maps each rejected case to a specific
		// message; matching the exact string nails down *which* guard fired.
		let revert_msg = crate::evm::decode_revert_reason(&result.data)
			.unwrap_or_else(|| panic!("expected an ABI-encoded Error(string) revert ({ctx})"));
		let expected_msg = if method == 0 {
			// METHOD_PRECOMPILE: the `CannotTerminateDelegatedAccount` guard.
			"revert: cannot terminate an EIP-7702 delegated account via the terminate pre-compile"
		} else {
			// METHOD_DELEGATE_CALL: short-circuits on the `PrecompileDelegateDenied` guard
			// before reaching `CannotTerminateDelegatedAccount`.
			"revert: illegal to call this pre-compile via delegate call"
		};
		assert_eq!(revert_msg, expected_msg, "wrong revert reason ({ctx})");

		// Side effects must not have happened: no balance moved, delegation intact.
		assert_eq!(
			<Test as Config>::Currency::free_balance(&alice_id),
			alice_balance_before,
			"Alice's balance must be unchanged after the reverted terminate ({ctx})",
		);
		assert_eq!(
			<Test as Config>::Currency::free_balance(&beneficiary_id),
			beneficiary_balance_before,
			"beneficiary's balance must be unchanged after the reverted terminate ({ctx})",
		);
		assert!(
			AccountInfo::<Test>::is_delegated(&ALICE_ADDR),
			"delegation must survive the failed terminate ({ctx})",
		);
	});
}

#[test]
fn system_terminate_via_precompile_on_delegated_account_reverts() {
	assert_system_terminate_on_delegated_reverts(0, "METHOD_PRECOMPILE direct call");
}

#[test]
fn system_terminate_via_delegatecall_on_delegated_account_reverts() {
	assert_system_terminate_on_delegated_reverts(1, "METHOD_DELEGATE_CALL via delegatecall");
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

/// An EIP-7702 transaction that revokes more delegation deposit than it charges must surface a
/// `Refund(N)` from `process_authorizations` — aggregating on an unsigned balance would clamp
/// the net refund to `Charge(0)` and silently lose it.
#[test]
fn pure_revoke_authorization_yields_net_refund() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 100_000_000);

		let target = builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
			.build_and_unwrap_contract();

		let setup = DelegationTestSetup::new([0xAB; 32]);
		let origin_before = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			0,
			"no hold before delegation",
		);
		let charge_result = setup.authorize(target.addr);

		// The expected deposit is the one recorded on the authority's own delegation account —
		// the account the charge and the hold both live on — not the target's (which would only
		// match by coincidence of identical `ContractInfo` encoding).
		let expected_deposit = get_contract(&setup.signer.address).storage_base_deposit();
		assert!(
			expected_deposit > 0,
			"delegation to a real contract must carry a non-zero deposit"
		);
		assert_eq!(
			charge_result.deposit,
			crate::StorageDeposit::Charge(expected_deposit),
			"set_delegation to a real contract must produce a positive net charge",
		);
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			expected_deposit,
			"hold must match the charged deposit after delegation",
		);

		let revoke_auth = setup.sign_authorization(H160::zero());
		let revoke_result = setup.process(&[revoke_auth]);
		assert_eq!(
			revoke_result.deposit,
			crate::StorageDeposit::Refund(expected_deposit),
			"pure-revoke authorization must propagate as Refund, not be clamped to zero",
		);
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id),
			0,
			"hold must be fully released after revoke",
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin),
			origin_before,
			"origin's free balance must be unchanged (deposits flow through the tx-fee pot \
			 under new_eth_tx, never via origin's free balance)",
		);
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
		let origin_after_set = <<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin);

		// Re-delegate to target B (same code size → same deposit)
		let auth = setup.sign_authorization(target_b.addr);
		let result = setup.process(&[auth]);

		// Same payer, same deposit → the net reported to the metering budget must be zero, not a
		// spurious charge or refund.
		assert_eq!(
			result.deposit,
			crate::StorageDeposit::Charge(0),
			"same-payer same-deposit re-delegation must net to zero",
		);

		let hold_b =
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &setup.authority_id);
		assert_eq!(hold_a, hold_b, "same-code re-delegation should keep the same deposit");
		assert_eq!(
			AccountInfo::<Test>::get_delegation_target(&setup.signer.address),
			Some(target_b.addr)
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&setup.origin),
			origin_after_set,
			"same-payer net-zero re-delegation must not touch origin's free balance",
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
		let authority_balance_before =
			<<Test as Config>::Currency as Inspect<_>>::balance(&authority_id);

		// Direct `set_delegation` computes the deposit delta but does NOT place a hold or
		// move balances — that's `process_authorizations`'s job. Invariant: no hold appears
		// and the authority's free balance is untouched across all three steps.
		let assert_no_balance_movement = |step: &str| {
			assert_eq!(
				get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id),
				0,
				"set_delegation must not place a hold ({step})",
			);
			assert_eq!(
				<<Test as Config>::Currency as Inspect<_>>::balance(&authority_id),
				authority_balance_before,
				"set_delegation must not move authority's free balance ({step})",
			);
		};

		// Step 1: delegate to contract A — charges deposit, increments refcount
		let deposit_a = AccountInfo::<Test>::set_delegation(&authority, target_a.addr).unwrap();
		assert_eq!(deposit_a.previous, 0, "fresh delegation should have no previous deposit");
		let charge_a = deposit_a.current;
		assert!(charge_a > 0, "delegation to contract should charge a deposit");
		assert_eq!(CodeInfoOf::<Test>::get(hash_a).unwrap().refcount(), refcount_a_before + 1);
		assert_no_balance_movement("after step 1");

		// Step 2: re-delegate to a plain EOA — refunds the full deposit, decrements refcount
		let plain_eoa = H160::from([0x77; 20]);
		let deposit_eoa = AccountInfo::<Test>::set_delegation(&authority, plain_eoa).unwrap();
		assert_eq!(deposit_eoa.previous, charge_a, "previous deposit should match step 1's charge");
		assert_eq!(deposit_eoa.current, 0, "re-delegating to EOA should leave no new deposit");
		assert_eq!(
			CodeInfoOf::<Test>::get(hash_a).unwrap().refcount(),
			refcount_a_before,
			"A's refcount should be back to original after re-delegating to EOA"
		);
		assert_no_balance_movement("after step 2");

		// Step 3: re-delegate to contract C — charges a fresh deposit, must NOT touch A
		let deposit_c = AccountInfo::<Test>::set_delegation(&authority, target_c.addr).unwrap();
		assert_eq!(deposit_c.previous, 0, "post-EOA re-delegation should have no previous deposit");
		let charge_c = deposit_c.current;
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
		assert_no_balance_movement("after step 3");
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
		assert_eq!(deposit.previous, 0, "delegation to EOA should not surface a previous deposit");
		assert_eq!(deposit.current, 0, "delegation to EOA should not charge any deposit");
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
		assert_eq!(deposit.previous, 0, "fresh delegation should have no previous deposit");
		assert_eq!(deposit.current, 0, "zero-code target should not charge a code-lockup deposit");
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &authority_id),
			0,
			"a zero-code target must produce no storage-deposit hold",
		);
	});
}

/// EOA → DelegatedEOA transition must preserve the account's existing `dust` field —
/// a `set_delegation` should only touch the account_type / contract_info, not silently
/// drop sub-ratio dust the user had accumulated.
#[test]
fn set_delegation_preserves_dust_on_eoa_transition() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x55; 20]);
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
		let bal_a_before = <<Test as Config>::Currency as Inspect<_>>::balance(&id_a);
		let bal_b_before = <<Test as Config>::Currency as Inspect<_>>::balance(&id_b);

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

		// `set_delegation` is the storage half — it must not place holds or move balances;
		// charging is the caller's job. Same expectations apply to both authorities.
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &id_a),
			0,
			"set_delegation must not place a hold on authority A",
		);
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &id_b),
			0,
			"set_delegation must not place a hold on authority B",
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&id_a),
			bal_a_before,
			"authority A's free balance must be unchanged by set_delegation",
		);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&id_b),
			bal_b_before,
			"authority B's free balance must be unchanged by set_delegation",
		);
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

/// EIP-7702 spec step 3 second-half: `s` must be `<= secp256k1n/2`. The shared
/// `recover_eth_address_from_message` rejects high-s signatures (EIP-2), so the high-s
/// twin of a valid authorization fails recovery and the tuple is skipped per spec —
/// no delegation, no nonce bump, and the transaction as a whole is unaffected.
#[test]
fn high_s_authorization_is_rejected() {
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

		assert!(!AccountInfo::<Test>::is_delegated(&setup.signer.address));
		assert_eq!(frame_system::Pallet::<Test>::account_nonce(&setup.authority_id), 0);
		assert_eq!(result.existing_accounts, 0);
		assert_eq!(result.new_accounts, 0);
	});
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
