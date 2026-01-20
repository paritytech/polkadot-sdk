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
	evm::{
		AuthorizationListEntry,
		eip7702::authorization_intrinsic_gas,
	},
	storage::{AccountInfo, AccountType},
	test_utils::builder::Contract,
	tests::{builder, *},
	AccountInfoOf, Code, Config, PER_AUTH_BASE_COST, PER_EMPTY_ACCOUNT_COST,
};
use frame_support::{assert_ok, traits::fungible::Mutate};
use sp_core::{H160, H256, Pair, U256, ecdsa, keccak_256};

/// Helper function to initialize an EOA account in pallet storage
fn initialize_eoa_account(address: &H160) {
	let account_info = AccountInfo::<Test> {
		account_type: AccountType::EOA,
		dust: 0,
	};
	AccountInfoOf::<Test>::insert(address, account_info);
}

/// Test keypair for signing authorizations
struct TestSigner {
	keypair: ecdsa::Pair,
	address: H160,
}

impl TestSigner {
	/// Create a new test signer from a seed
	fn new(seed: &[u8; 32]) -> Self {
		let keypair = ecdsa::Pair::from_seed(seed);
		// Derive the Ethereum address by signing a dummy message
		let dummy_message = [0u8; 32];
		let signature = keypair.sign_prehashed(&dummy_message);

		use sp_io::crypto::secp256k1_ecdsa_recover;
		let recovered_pubkey = secp256k1_ecdsa_recover(&signature.0, &dummy_message)
			.ok()
			.expect("Failed to recover public key from signature");
		let pubkey_hash = keccak_256(&recovered_pubkey);
		let address = H160::from_slice(&pubkey_hash[12..]);

		Self { keypair, address }
	}




	/// Sign an EIP-7702 authorization tuple
	fn sign_authorization(
		&self,
		chain_id: U256,
		address: H160,
		nonce: U256,
	) -> AuthorizationListEntry {
		// Construct the message: MAGIC || rlp([chain_id, address, nonce])
		let mut message = Vec::new();
		message.push(crate::EIP7702_MAGIC);

		// RLP encode [chain_id, address, nonce]
		let mut rlp_stream = crate::evm::rlp::RlpStream::new_list(3);
		rlp_stream.append(&chain_id);
		rlp_stream.append(&address);
		rlp_stream.append(&nonce);
		let rlp_encoded = rlp_stream.out();
		message.extend_from_slice(&rlp_encoded);

		// Hash the message
		let message_hash = keccak_256(&message);

		// Sign with the keypair
		let signature = self.keypair.sign_prehashed(&message_hash);
		let sig_bytes = signature.0;

		// The signature from ecdsa::Pair is 65 bytes: [r (32), s (32), recovery_id (1)]
		let mut r_bytes = [0u8; 32];
		let mut s_bytes = [0u8; 32];
		r_bytes.copy_from_slice(&sig_bytes[0..32]);
		s_bytes.copy_from_slice(&sig_bytes[32..64]);
		let recovery_id = sig_bytes[64];

		// Convert to U256
		let r = U256::from_big_endian(&r_bytes);
		let s = U256::from_big_endian(&s_bytes);
		let y_parity = U256::from(recovery_id);

		AuthorizationListEntry {
			chain_id,
			address,
			nonce,
			y_parity,
			r,
			s,
		}
	}
}


#[test]
fn delegation_indicator_format() {
	// Test that delegation indicator has correct format: 0xef0100 || address
	let target_address = H160::from([0x42; 20]);
	let mut expected_code = vec![0xef, 0x01, 0x00];
	expected_code.extend_from_slice(target_address.as_bytes());

	assert_eq!(expected_code.len(), 23, "Delegation indicator must be 23 bytes");
	assert!(
		AccountInfo::<Test>::is_delegation_indicator(&expected_code),
		"Should be recognized as delegation indicator"
	);

	let extracted = AccountInfo::<Test>::extract_delegation_target(&expected_code);
	assert_eq!(extracted, Some(target_address), "Should extract correct target address");
}

#[test]
fn delegation_indicator_detection() {
	// Valid delegation indicator
	let mut valid = vec![0xef, 0x01, 0x00];
	valid.extend_from_slice(&[0u8; 20]);
	assert!(AccountInfo::<Test>::is_delegation_indicator(&valid));

	// Wrong prefix
	let mut wrong_prefix = vec![0xef, 0x01, 0x01];
	wrong_prefix.extend_from_slice(&[0u8; 20]);
	assert!(!AccountInfo::<Test>::is_delegation_indicator(&wrong_prefix));

	// Wrong length (too short)
	let too_short = vec![0xef, 0x01, 0x00, 0x00];
	assert!(!AccountInfo::<Test>::is_delegation_indicator(&too_short));

	// Wrong length (too long)
	let mut too_long = vec![0xef, 0x01, 0x00];
	too_long.extend_from_slice(&[0u8; 21]);
	assert!(!AccountInfo::<Test>::is_delegation_indicator(&too_long));

	// Empty code
	assert!(!AccountInfo::<Test>::is_delegation_indicator(&[]));

	// Regular contract code
	let regular_code = vec![0x60, 0x80, 0x60, 0x40, 0x52];
	assert!(!AccountInfo::<Test>::is_delegation_indicator(&regular_code));
}

#[test]
fn authorization_gas_calculation() {

	// One authorization
	assert_eq!(authorization_intrinsic_gas(1), PER_EMPTY_ACCOUNT_COST);

	// Multiple authorizations
	assert_eq!(authorization_intrinsic_gas(5), PER_EMPTY_ACCOUNT_COST * 5);

	// Check the cost constants are as per EIP-7702 spec
	assert_eq!(PER_AUTH_BASE_COST, 12500);
	assert_eq!(PER_EMPTY_ACCOUNT_COST, 25000);
}

#[test]
fn set_delegation_creates_indicator() {
	ExtBuilder::default().build().execute_with(|| {
		let eoa = H160::from([0x11; 20]);
		let target = H160::from([0x22; 20]);
		let nonce = 0u32.into();

		// Set delegation
		assert_ok!(AccountInfo::<Test>::set_delegation(&eoa, target, nonce));

		// Verify delegation is set
		assert!(AccountInfo::<Test>::is_delegated(&eoa));

		// Verify we can retrieve the target
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&eoa), Some(target));

		// Verify the account is now a "contract" (has code)
		assert!(AccountInfo::<Test>::is_contract(&eoa));
	});
}

#[test]
fn clear_delegation_restores_eoa() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x11; 20]);
		let target = H160::from([0x22; 20]);
		let nonce = 0u32.into();

		// Set delegation
		assert_ok!(AccountInfo::<Test>::set_delegation(&authority, target, nonce));
		assert!(AccountInfo::<Test>::is_delegated(&authority));

		// Clear delegation
		assert_ok!(AccountInfo::<Test>::clear_delegation(&authority));

		// Should no longer be delegated
		assert!(!AccountInfo::<Test>::is_delegated(&authority));
	});
}

#[test]
fn delegation_can_be_updated() {
	ExtBuilder::default().build().execute_with(|| {
		let authority = H160::from([0x11; 20]);
		let target1 = H160::from([0x22; 20]);
		let target2 = H160::from([0x33; 20]);
		let nonce = 0u32.into();

		// Set first delegation
		assert_ok!(AccountInfo::<Test>::set_delegation(&authority, target1, nonce));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target1));

		// Update to second delegation
		assert_ok!(AccountInfo::<Test>::set_delegation(&authority, target2, nonce));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target2));

		// Still delegated
		assert!(AccountInfo::<Test>::is_delegated(&authority));
	});
}

#[test]
fn regular_contract_is_not_delegation() {
	ExtBuilder::default().build().execute_with(|| {
		// Deploy a regular contract
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 1_000_000_000);
		let (binary, _) = compile_module("dummy").unwrap();

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(binary)).build_and_unwrap_contract();

		// Regular contract should not be considered a delegation
		assert!(AccountInfo::<Test>::is_contract(&addr));
		assert!(!AccountInfo::<Test>::is_delegated(&addr));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&addr), None);
	});
}

#[test]
fn eip3607_allows_delegated_accounts_to_originate_transactions() {
	ExtBuilder::default().build().execute_with(|| {
		// Per EIP-7702: accounts with delegation indicators ARE allowed to
		// originate transactions (modification to EIP-3607)
		let authority = H160::from([0x11; 20]);
		let target = H160::from([0x22; 20]);
		let nonce = 0u32.into();

		// Create the account
		let account_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&account_id, 1_000_000);

		// Set delegation
		assert_ok!(AccountInfo::<Test>::set_delegation(&authority, target, nonce));

		// Should be allowed to originate transactions (EIP-7702 modification to EIP-3607)
		let origin = RuntimeOrigin::signed(account_id.clone());
		assert_ok!(Contracts::ensure_non_contract_if_signed(&origin));
	});
}

#[test]
fn eip3607_rejects_regular_contract_originating_transactions() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&ALICE, 1_000_000_000);
		let (binary, _) = compile_module("dummy").unwrap();

		let Contract { account_id, .. } =
			builder::bare_instantiate(Code::Upload(binary)).build_and_unwrap_contract();

		// Regular contracts should NOT be allowed to originate transactions (EIP-3607)
		let origin = RuntimeOrigin::signed(account_id);
		assert!(Contracts::ensure_non_contract_if_signed(&origin).is_err());
	});
}

#[test]
fn delegation_indicator_size_is_23_bytes() {
	ExtBuilder::default().build().execute_with(|| {
		let eoa = H160::from([0x11; 20]);
		let target = H160::from([0x22; 20]);
		let nonce = 0u32.into();

		assert_ok!(AccountInfo::<Test>::set_delegation(&eoa, target, nonce));

		// Get the contract info
		let contract_info = AccountInfo::<Test>::load_contract(&eoa).unwrap();

		// Get the code
		let code = crate::PristineCode::<Test>::get(contract_info.code_hash).unwrap();

		// Verify size
		assert_eq!(code.len(), 23, "Delegation indicator must be exactly 23 bytes");

		// Verify format
		assert_eq!(&code[0..3], &[0xef, 0x01, 0x00]);
		assert_eq!(&code[3..23], target.as_bytes());
	});
}

#[test]
fn multiple_delegations_last_one_wins() {
	// Per EIP-7702: "When multiple tuples from the same authority are present,
	// set the code using the address in the last valid occurrence."
	ExtBuilder::default().build().execute_with(|| {
		let eoa = H160::from([0x11; 20]);
		let target1 = H160::from([0x22; 20]);
		let target2 = H160::from([0x33; 20]);
		let target3 = H160::from([0x44; 20]);

		// Set first delegation
		assert_ok!(AccountInfo::<Test>::set_delegation(&eoa, target1, 0u32.into()));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&eoa), Some(target1));

		// Set second delegation (should override)
		assert_ok!(AccountInfo::<Test>::set_delegation(&eoa, target2, 0u32.into()));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&eoa), Some(target2));

		// Set third delegation (should override again)
		assert_ok!(AccountInfo::<Test>::set_delegation(&eoa, target3, 0u32.into()));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&eoa), Some(target3));
	});
}

#[test]
fn valid_signature_is_verified_correctly() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		// Create a signer
		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		// Fund the account
		let account_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&account_id, 1_000_000);

		// Initialize the account in pallet storage
		initialize_eoa_account(&authority);

		// Get current nonce
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&account_id));

		// Sign authorization with correct nonce
		let auth = signer.sign_authorization(chain_id, target, nonce);

		// Process authorizations
		let mut accessed = alloc::collections::BTreeSet::new();
		let refund = crate::evm::eip7702::process_authorizations::<Test>(
			vec![auth],
			chain_id,
			&mut accessed,
		);

		// Should succeed and set delegation
		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target));

		// Existing account should get refund
		assert_eq!(refund, PER_EMPTY_ACCOUNT_COST - PER_AUTH_BASE_COST);

		// Authority should be in accessed addresses
		assert!(accessed.contains(&authority));
	});
}

#[test]
fn invalid_chain_id_rejects_authorization() {
	ExtBuilder::default().build().execute_with(|| {
		let correct_chain_id = U256::from(1);
		let wrong_chain_id = U256::from(999);
		let target = H160::from([0x42; 20]);

		// Create a signer
		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		// Fund the account
		let account_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&account_id, 1_000_000);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&account_id));

		// Sign with wrong chain ID
		let auth = signer.sign_authorization(wrong_chain_id, target, nonce);

		// Process with correct chain ID - should reject
		let mut accessed = alloc::collections::BTreeSet::new();
		let refund = crate::evm::eip7702::process_authorizations::<Test>(
			vec![auth],
			correct_chain_id,
			&mut accessed,
		);

		// Should not set delegation
		assert!(!AccountInfo::<Test>::is_delegated(&authority));

		// No refund for failed authorization
		assert_eq!(refund, 0);
	});
}

#[test]
fn nonce_mismatch_rejects_authorization() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		// Create a signer
		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		// Fund the account
		let account_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&account_id, 1_000_000);

		let current_nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&account_id));
		let wrong_nonce = current_nonce.saturating_add(U256::from(1));

		// Sign with wrong nonce
		let auth = signer.sign_authorization(chain_id, target, wrong_nonce);

		// Process - should reject due to nonce mismatch
		let mut accessed = alloc::collections::BTreeSet::new();
		let refund = crate::evm::eip7702::process_authorizations::<Test>(
			vec![auth],
			chain_id,
			&mut accessed,
		);

		// Should not set delegation
		assert!(!AccountInfo::<Test>::is_delegated(&signer.address));
		assert_eq!(refund, 0);
	});
}

#[test]
fn multiple_authorizations_from_same_authority_last_wins() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target1 = H160::from([0x11; 20]);
		let target2 = H160::from([0x22; 20]);
		let target3 = H160::from([0x33; 20]);

		// Create a signer
		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		// Fund the account
		let account_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&account_id, 1_000_000);

		// Initialize the account in pallet storage
		initialize_eoa_account(&authority);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&account_id));

		// Sign three authorizations with same nonce but different targets
		let auth1 = signer.sign_authorization(chain_id, target1, nonce);
		let auth2 = signer.sign_authorization(chain_id, target2, nonce);
		let auth3 = signer.sign_authorization(chain_id, target3, nonce);

		// Process all three - last one should win
		let mut accessed = alloc::collections::BTreeSet::new();
		let refund = crate::evm::eip7702::process_authorizations::<Test>(
			vec![auth1, auth2, auth3],
			chain_id,
			&mut accessed,
		);

		// Should set delegation to target3 (last one)
		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target3));

		// Only one refund even though three authorizations processed
		assert_eq!(refund, PER_EMPTY_ACCOUNT_COST - PER_AUTH_BASE_COST);
	});
}

#[test]
fn authorization_increments_nonce() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		// Create a signer
		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		// Fund the account
		let account_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&account_id, 1_000_000);

		let initial_nonce = frame_system::Pallet::<Test>::account_nonce(&account_id);

		// Sign authorization with current nonce
		let auth = signer.sign_authorization(chain_id, target, U256::from(initial_nonce));

		// Process authorization
		let mut accessed = alloc::collections::BTreeSet::new();
		let _refund = crate::evm::eip7702::process_authorizations::<Test>(
			vec![auth],
			chain_id,
			&mut accessed,
		);

		// Nonce should be incremented
		let new_nonce = frame_system::Pallet::<Test>::account_nonce(&account_id);
		assert_eq!(new_nonce, initial_nonce + 1);
	});
}

#[test]
fn chain_id_zero_accepts_any_chain() {
	ExtBuilder::default().build().execute_with(|| {
		let current_chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		// Create a signer
		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		// Fund the account
		let account_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&account_id, 1_000_000);

		// Initialize the account in pallet storage
		initialize_eoa_account(&authority);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&account_id));

		// Sign with chain_id = 0 (should accept any chain)
		let auth = signer.sign_authorization(U256::zero(), target, nonce);

		// Process with current chain ID
		let mut accessed = alloc::collections::BTreeSet::new();
		let refund = crate::evm::eip7702::process_authorizations::<Test>(
			vec![auth],
			current_chain_id,
			&mut accessed,
		);

		// Should succeed
		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target));
		assert_eq!(refund, PER_EMPTY_ACCOUNT_COST - PER_AUTH_BASE_COST);
	});
}

#[test]
fn new_account_gets_no_refund() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		// Create a signer but DON'T fund the account (new account)
		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		let account_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&account_id));

		// Sign authorization
		let auth = signer.sign_authorization(chain_id, target, nonce);

		// Process authorization
		let mut accessed = alloc::collections::BTreeSet::new();
		let refund = crate::evm::eip7702::process_authorizations::<Test>(
			vec![auth],
			chain_id,
			&mut accessed,
		);

		// New account should get no refund
		assert_eq!(refund, 0);

		// But delegation should still be set
		assert!(AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), Some(target));
	});
}

#[test]
fn clearing_delegation_with_zero_address() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = U256::from(1);
		let target = H160::from([0x42; 20]);

		// Create a signer
		let seed = H256::random();
		let signer = TestSigner::new(&seed.0);
		let authority = signer.address;

		// Fund the account
		let account_id = <Test as Config>::AddressMapper::to_account_id(&authority);
		let _ = <<Test as Config>::Currency as Mutate<_>>::set_balance(&account_id, 1_000_000);

		// Initialize the account in pallet storage
		initialize_eoa_account(&authority);

		let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&account_id));

		// First, set delegation
		let auth1 = signer.sign_authorization(chain_id, target, nonce);
		let mut accessed = alloc::collections::BTreeSet::new();
		let _refund1 = crate::evm::eip7702::process_authorizations::<Test>(
			vec![auth1],
			chain_id,
			&mut accessed,
		);

		// Verify delegation is set
		assert!(AccountInfo::<Test>::is_delegated(&authority));

		// Get new nonce after first authorization
		let new_nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&account_id));

		// Clear delegation with zero address
		let auth2 = signer.sign_authorization(chain_id, H160::zero(), new_nonce);
		let mut accessed2 = alloc::collections::BTreeSet::new();
		let _refund2 = crate::evm::eip7702::process_authorizations::<Test>(
			vec![auth2],
			chain_id,
			&mut accessed2,
		);

		// Delegation should be cleared
		assert!(!AccountInfo::<Test>::is_delegated(&authority));
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&authority), None);
	});
}
