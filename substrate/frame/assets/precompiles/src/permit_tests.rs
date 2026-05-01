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

use super::permit;
use crate::mock::{new_test_ext, Test};
use pallet_revive::precompiles::H160;
use sp_core::{H256, U256};

// =============================================================================
// Test Helpers and Constants
// =============================================================================

/// Helper to create a verifying contract address for tests.
fn test_verifying_contract() -> H160 {
	H160::from_low_u64_be(0x1234)
}

/// Helper to create a test token name for EIP-712 domain separator.
fn test_token_name() -> &'static [u8] {
	b"Test Token"
}

/// Helper to create a future deadline (far in the future).
/// EIP-2612 specifies deadlines in UNIX seconds.
fn future_deadline() -> [u8; 32] {
	// Unix timestamp for year 2100 in seconds
	U256::from(4102444800u64).to_big_endian()
}

/// Helper to create a past deadline.
/// EIP-2612 specifies deadlines in UNIX seconds.
fn past_deadline() -> [u8; 32] {
	// Unix timestamp for year 2020 in seconds
	U256::from(1577836800u64).to_big_endian()
}

/// Hardhat account #0 address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
///
/// This is a well-known test address derived from the private key:
/// 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
///
/// DO NOT use in production!
const HARDHAT_ACCOUNT_0: H160 = H160([
	0xf3, 0x9F, 0xd6, 0xe5, 0x1a, 0xad, 0x88, 0xF6, 0xF4, 0xce, 0x6a, 0xB8, 0x82, 0x72, 0x79, 0xcf,
	0xfF, 0xb9, 0x22, 0x66,
]);

/// Parameters for a valid pre-computed permit signature.
///
/// Generated using Hardhat account #0 private key with these parameters:
/// - Chain ID: 31337
/// - Token Name: "Asset Permit"
/// - Owner: 0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266
/// - Verifying Contract: 0x0000000000000000000000000000000012345678
/// - Spender: 0x0000000000000000000000000000000098765432
/// - Value: 1000
/// - Nonce: 0
/// - Deadline: u64::MAX (18446744073709551615)
struct ValidPermitParams {
	verifying_contract: H160,
	name: &'static [u8],
	owner: H160,
	spender: H160,
	value: [u8; 32],
	deadline: [u8; 32],
	v: u8,
	r: [u8; 32],
	s: [u8; 32],
}

fn valid_permit_params() -> ValidPermitParams {
	ValidPermitParams {
		verifying_contract: H160::from_low_u64_be(0x1234_5678),
		name: b"Asset Permit",
		owner: HARDHAT_ACCOUNT_0,
		spender: H160::from_low_u64_be(0x9876_5432),
		value: U256::from(1000).to_big_endian(),
		deadline: U256::from(u64::MAX).to_big_endian(),
		v: 27u8,
		r: [
			175, 252, 243, 1, 254, 212, 189, 22, 49, 158, 63, 188, 243, 21, 56, 240, 124, 215, 220,
			121, 137, 153, 208, 70, 123, 109, 221, 94, 191, 131, 210, 111,
		],
		s: [
			21, 240, 201, 4, 59, 104, 154, 99, 230, 111, 29, 9, 150, 225, 57, 209, 15, 222, 27, 5,
			147, 40, 44, 246, 24, 108, 82, 129, 121, 73, 44, 234,
		],
	}
}

// =============================================================================
// Nonce Tests
// =============================================================================

#[test]
fn nonce_starts_at_zero() {
	new_test_ext().execute_with(|| {
		let verifying_contract = test_verifying_contract();
		let owner = H160::from_low_u64_be(1);

		let nonce = permit::Pallet::<Test>::nonce(&verifying_contract, &owner);
		assert_eq!(nonce, U256::zero());
	});
}

#[test]
fn nonce_increments() {
	new_test_ext().execute_with(|| {
		let verifying_contract = test_verifying_contract();
		let owner = H160::from_low_u64_be(1);

		let nonce1 = permit::Pallet::<Test>::increment_nonce(&verifying_contract, &owner).unwrap();
		assert_eq!(nonce1, U256::one());

		let nonce2 = permit::Pallet::<Test>::increment_nonce(&verifying_contract, &owner).unwrap();
		assert_eq!(nonce2, U256::from(2));

		let nonce_read = permit::Pallet::<Test>::nonce(&verifying_contract, &owner);
		assert_eq!(nonce_read, U256::from(2));
	});
}

#[test]
fn nonces_are_independent_per_verifying_contract() {
	new_test_ext().execute_with(|| {
		let owner = H160::from_low_u64_be(1);
		let contract_1 = H160::from_low_u64_be(0x1111);
		let contract_2 = H160::from_low_u64_be(0x2222);

		permit::Pallet::<Test>::increment_nonce(&contract_1, &owner).unwrap();
		permit::Pallet::<Test>::increment_nonce(&contract_1, &owner).unwrap();

		assert_eq!(permit::Pallet::<Test>::nonce(&contract_1, &owner), U256::from(2));
		assert_eq!(permit::Pallet::<Test>::nonce(&contract_2, &owner), U256::zero());
	});
}

#[test]
fn nonces_are_independent_per_owner() {
	new_test_ext().execute_with(|| {
		let verifying_contract = test_verifying_contract();
		let owner1 = H160::from_low_u64_be(1);
		let owner2 = H160::from_low_u64_be(2);

		permit::Pallet::<Test>::increment_nonce(&verifying_contract, &owner1).unwrap();
		permit::Pallet::<Test>::increment_nonce(&verifying_contract, &owner1).unwrap();

		assert_eq!(permit::Pallet::<Test>::nonce(&verifying_contract, &owner1), U256::from(2));
		assert_eq!(permit::Pallet::<Test>::nonce(&verifying_contract, &owner2), U256::zero());
	});
}

// =============================================================================
// Domain Separator Tests
// =============================================================================

#[test]
fn domain_separator_is_computed() {
	new_test_ext().execute_with(|| {
		let verifying_contract = test_verifying_contract();
		let name = test_token_name();
		let separator = permit::Pallet::<Test>::compute_domain_separator(&verifying_contract, name);
		// Should be a non-zero hash
		assert_ne!(separator, H256::zero());
	});
}

#[test]
fn domain_separator_is_deterministic() {
	new_test_ext().execute_with(|| {
		let verifying_contract = test_verifying_contract();
		let name = test_token_name();
		let separator1 =
			permit::Pallet::<Test>::compute_domain_separator(&verifying_contract, name);
		let separator2 =
			permit::Pallet::<Test>::compute_domain_separator(&verifying_contract, name);
		// Should return the same value for same inputs
		assert_eq!(separator1, separator2);
	});
}

#[test]
fn domain_separators_differ_per_verifying_contract() {
	new_test_ext().execute_with(|| {
		let contract_1 = H160::from_low_u64_be(0x1111);
		let contract_2 = H160::from_low_u64_be(0x2222);
		let name = test_token_name();

		let separator1 = permit::Pallet::<Test>::compute_domain_separator(&contract_1, name);
		let separator2 = permit::Pallet::<Test>::compute_domain_separator(&contract_2, name);

		// Domain separators should be different for different verifying contracts
		assert_ne!(separator1, separator2);
	});
}

#[test]
fn domain_separators_differ_per_token_name() {
	new_test_ext().execute_with(|| {
		let verifying_contract = test_verifying_contract();

		let separator1 =
			permit::Pallet::<Test>::compute_domain_separator(&verifying_contract, b"Token A");
		let separator2 =
			permit::Pallet::<Test>::compute_domain_separator(&verifying_contract, b"Token B");

		// Domain separators should be different for different token names
		assert_ne!(separator1, separator2);
	});
}

// =============================================================================
// Permit Digest Tests
// =============================================================================

#[test]
fn permit_digest_is_deterministic() {
	new_test_ext().execute_with(|| {
		let verifying_contract = test_verifying_contract();
		let name = test_token_name();
		let owner = H160::from_low_u64_be(1);
		let spender = H160::from_low_u64_be(2);
		let value = [0u8; 32];
		let nonce = U256::zero();
		let deadline = [0u8; 32];

		let digest1 = permit::Pallet::<Test>::permit_digest(
			&verifying_contract,
			name,
			&owner,
			&spender,
			&value,
			&nonce,
			&deadline,
		);
		let digest2 = permit::Pallet::<Test>::permit_digest(
			&verifying_contract,
			name,
			&owner,
			&spender,
			&value,
			&nonce,
			&deadline,
		);

		assert_eq!(digest1, digest2);
	});
}

#[test]
fn permit_digest_changes_with_nonce() {
	new_test_ext().execute_with(|| {
		let verifying_contract = test_verifying_contract();
		let name = test_token_name();
		let owner = H160::from_low_u64_be(1);
		let spender = H160::from_low_u64_be(2);
		let value = [0u8; 32];
		let deadline = [0u8; 32];

		let digest1 = permit::Pallet::<Test>::permit_digest(
			&verifying_contract,
			name,
			&owner,
			&spender,
			&value,
			&U256::zero(),
			&deadline,
		);
		let digest2 = permit::Pallet::<Test>::permit_digest(
			&verifying_contract,
			name,
			&owner,
			&spender,
			&value,
			&U256::one(),
			&deadline,
		);

		assert_ne!(digest1, digest2);
	});
}

#[test]
fn permit_digest_changes_with_verifying_contract() {
	new_test_ext().execute_with(|| {
		let contract_1 = H160::from_low_u64_be(0x1111);
		let contract_2 = H160::from_low_u64_be(0x2222);
		let name = test_token_name();
		let owner = H160::from_low_u64_be(1);
		let spender = H160::from_low_u64_be(2);
		let value = [0u8; 32];
		let nonce = U256::zero();
		let deadline = [0u8; 32];

		let digest1 = permit::Pallet::<Test>::permit_digest(
			&contract_1,
			name,
			&owner,
			&spender,
			&value,
			&nonce,
			&deadline,
		);
		let digest2 = permit::Pallet::<Test>::permit_digest(
			&contract_2,
			name,
			&owner,
			&spender,
			&value,
			&nonce,
			&deadline,
		);

		// Digests should differ for different verifying contracts (domain separation)
		assert_ne!(digest1, digest2);
	});
}

// =============================================================================
// ECDSA Recovery Tests
// =============================================================================

#[test]
fn ecrecover_with_valid_signature() {
	new_test_ext().execute_with(|| {
		// Test vector generated with ethers.js:
		// const wallet = new
		// Wallet("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80");
		// const message = "test message";
		// const messageHash = ethers.keccak256(ethers.toUtf8Bytes(message));
		// const signature = wallet.signingKey.sign(messageHash);

		let message_hash = sp_io::hashing::keccak_256(b"test message");

		// Signature components from ethers.js signing
		let r: [u8; 32] = [
			0xbf, 0x50, 0xb8, 0x99, 0x85, 0xbd, 0x02, 0x4b, 0xd4, 0xf2, 0x5e, 0xa2, 0x1e, 0x72,
			0xe0, 0x56, 0xd4, 0x46, 0xdd, 0xe9, 0x8a, 0xac, 0x81, 0xf3, 0x10, 0x3c, 0x9e, 0x46,
			0x9e, 0x23, 0x1a, 0xad,
		];
		let s: [u8; 32] = [
			0x51, 0x91, 0x01, 0xf0, 0x2d, 0xaa, 0xbb, 0xd4, 0xaf, 0x51, 0xdf, 0x7f, 0xa2, 0x12,
			0xc1, 0x33, 0x88, 0xa9, 0x26, 0x10, 0x84, 0x2b, 0xda, 0xe8, 0x07, 0x26, 0x60, 0x99,
			0x36, 0x7c, 0xc6, 0x86,
		];
		let v = 27u8;

		let result = permit::Pallet::<Test>::ecrecover(&message_hash, v, &r, &s);

		// Should recover the correct address (Hardhat account #0)
		assert_eq!(result.unwrap(), HARDHAT_ACCOUNT_0);
	});
}

#[test]
fn ecrecover_fails_with_invalid_v() {
	new_test_ext().execute_with(|| {
		let digest = [0u8; 32];
		let r = [0u8; 32];
		let s = [0u8; 32];
		let v = 30u8; // Invalid v value (must be 27 or 28)

		let result = permit::Pallet::<Test>::ecrecover(&digest, v, &r, &s);
		assert!(matches!(result, Err(permit::pallet::Error::<Test>::InvalidVValue)));
	});
}

#[test]
fn ecrecover_fails_with_v_below_27() {
	new_test_ext().execute_with(|| {
		let digest = [0u8; 32];
		let r = [0u8; 32];
		let s = [0u8; 32];
		let v = 0u8; // Invalid v value

		let result = permit::Pallet::<Test>::ecrecover(&digest, v, &r, &s);
		assert!(matches!(result, Err(permit::pallet::Error::<Test>::InvalidVValue)));
	});
}

// =============================================================================
// Signature Malleability Tests
// =============================================================================

#[test]
fn ecrecover_rejects_high_s_value() {
	new_test_ext().execute_with(|| {
		let digest = [0u8; 32];
		let r = [0u8; 32];
		// s value greater than SECP256K1_N_DIV_2
		let s: [u8; 32] = [
			0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00,
		];
		let v = 27u8;

		let result = permit::Pallet::<Test>::ecrecover(&digest, v, &r, &s);
		assert!(matches!(result, Err(permit::pallet::Error::<Test>::SignatureSValueTooHigh)));
	});
}

#[test]
fn ecrecover_accepts_s_at_boundary() {
	new_test_ext().execute_with(|| {
		let digest = [0u8; 32];
		let r: [u8; 32] = [
			0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00,
		];
		// s value exactly at SECP256K1_N_DIV_2 (should be valid)
		let s = permit::SECP256K1_N_DIV_2;
		let v = 27u8;

		// Should not fail with SignatureSValueTooHigh
		let result = permit::Pallet::<Test>::ecrecover(&digest, v, &r, &s);
		// The signature itself might be invalid, but it should not fail due to s being too high
		assert!(!matches!(result, Err(permit::pallet::Error::<Test>::SignatureSValueTooHigh)));
	});
}

// =============================================================================
// Deadline Validation Tests
// =============================================================================

#[test]
fn verify_permit_fails_with_expired_deadline() {
	new_test_ext().execute_with(|| {
		let verifying_contract = test_verifying_contract();
		let name = test_token_name();
		let owner = H160::from_low_u64_be(1);
		let spender = H160::from_low_u64_be(2);
		let value = [0u8; 32];
		let deadline = past_deadline(); // Deadline in the past
		let r = [0u8; 32];
		let s = [0u8; 32];
		let v = 27u8;

		let result = permit::Pallet::<Test>::verify_permit(
			&verifying_contract,
			name,
			&owner,
			&spender,
			&value,
			&deadline,
			v,
			&r,
			&s,
		);

		assert!(matches!(result, Err(permit::pallet::Error::<Test>::PermitExpired)));
	});
}

// =============================================================================
// Use Permit (Replay Attack Prevention) Tests
// =============================================================================

#[test]
fn verify_permit_does_not_increment_nonce() {
	new_test_ext().execute_with(|| {
		let verifying_contract = test_verifying_contract();
		let name = test_token_name();
		let owner = H160::from_low_u64_be(1);
		let spender = H160::from_low_u64_be(2);
		let value = [0u8; 32];
		let deadline = future_deadline();
		let r = [0u8; 32];
		let s = [0u8; 32];
		let v = 27u8;

		let initial_nonce = permit::Pallet::<Test>::nonce(&verifying_contract, &owner);

		// Call verify_permit multiple times
		for _ in 0..3 {
			let _ = permit::Pallet::<Test>::verify_permit(
				&verifying_contract,
				name,
				&owner,
				&spender,
				&value,
				&deadline,
				v,
				&r,
				&s,
			);
		}

		// Nonce should remain unchanged
		let final_nonce = permit::Pallet::<Test>::nonce(&verifying_contract, &owner);
		assert_eq!(initial_nonce, final_nonce, "verify_permit must not modify nonce");
	});
}

#[test]
fn use_permit_succeeds_with_valid_signature() {
	new_test_ext().execute_with(|| {
		let p = valid_permit_params();

		// Verify initial nonce is zero
		let initial_nonce = permit::Pallet::<Test>::nonce(&p.verifying_contract, &p.owner);
		assert_eq!(initial_nonce, U256::zero(), "initial nonce should be zero");

		// First use_permit should succeed
		let result = permit::Pallet::<Test>::use_permit(
			&p.verifying_contract,
			p.name,
			&p.owner,
			&p.spender,
			&p.value,
			&p.deadline,
			p.v,
			&p.r,
			&p.s,
		);
		assert!(result.is_ok(), "use_permit should succeed with valid signature");

		// Nonce should now be 1
		let nonce_after = permit::Pallet::<Test>::nonce(&p.verifying_contract, &p.owner);
		assert_eq!(nonce_after, U256::one(), "nonce should be incremented to 1 after use_permit");
	});
}

/// This is the critical EIP-2612 security property test.
///
/// It verifies that once a permit signature has been used successfully,
/// the same signature cannot be replayed to grant additional allowances.
/// This is the fundamental protection against permit replay attacks.
#[test]
fn use_permit_rejects_replay_of_consumed_permit() {
	new_test_ext().execute_with(|| {
		let p = valid_permit_params();

		// First use: should succeed
		let first_result = permit::Pallet::<Test>::use_permit(
			&p.verifying_contract,
			p.name,
			&p.owner,
			&p.spender,
			&p.value,
			&p.deadline,
			p.v,
			&p.r,
			&p.s,
		);
		assert!(first_result.is_ok(), "first use_permit should succeed");

		// Verify nonce was incremented
		let nonce = permit::Pallet::<Test>::nonce(&p.verifying_contract, &p.owner);
		assert_eq!(nonce, U256::one(), "nonce should be 1 after first use");

		// Replay attempt: should fail because nonce is now 1, but signature was for nonce 0
		let replay_result = permit::Pallet::<Test>::use_permit(
			&p.verifying_contract,
			p.name,
			&p.owner,
			&p.spender,
			&p.value,
			&p.deadline,
			p.v,
			&p.r,
			&p.s,
		);

		// The replay should fail with SignerMismatch because the digest computed
		// with nonce=1 won't match the signature created for nonce=0
		assert!(
			replay_result.is_err(),
			"replay of consumed permit MUST fail - this is a critical security property"
		);
		assert!(
			matches!(replay_result, Err(permit::pallet::Error::<Test>::SignerMismatch)),
			"replay should fail with SignerMismatch due to nonce mismatch in digest"
		);

		// Nonce should still be 1 (failed attempt should not increment)
		let nonce_after_replay = permit::Pallet::<Test>::nonce(&p.verifying_contract, &p.owner);
		assert_eq!(
			nonce_after_replay,
			U256::one(),
			"nonce should remain 1 after failed replay attempt"
		);
	});
}

/// Test that multiple consecutive replays all fail.
/// This ensures the protection isn't a one-time check.
#[test]
fn use_permit_rejects_multiple_replay_attempts() {
	new_test_ext().execute_with(|| {
		let p = valid_permit_params();

		// First use: should succeed
		let first_result = permit::Pallet::<Test>::use_permit(
			&p.verifying_contract,
			p.name,
			&p.owner,
			&p.spender,
			&p.value,
			&p.deadline,
			p.v,
			&p.r,
			&p.s,
		);
		assert!(first_result.is_ok(), "first use_permit should succeed");

		// Multiple replay attempts: all should fail
		for attempt in 1..=5 {
			let replay_result = permit::Pallet::<Test>::use_permit(
				&p.verifying_contract,
				p.name,
				&p.owner,
				&p.spender,
				&p.value,
				&p.deadline,
				p.v,
				&p.r,
				&p.s,
			);
			assert!(replay_result.is_err(), "replay attempt {} should fail", attempt);
		}

		// Nonce should still be 1 (no failed attempts should increment)
		let final_nonce = permit::Pallet::<Test>::nonce(&p.verifying_contract, &p.owner);
		assert_eq!(
			final_nonce,
			U256::one(),
			"nonce should remain 1 after all failed replay attempts"
		);
	});
}

// =============================================================================
// PERMIT_TYPEHASH Tests
// =============================================================================

#[test]
fn permit_typehash_is_correct() {
	let computed = sp_io::hashing::keccak_256(
		b"Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)",
	);
	assert_eq!(computed, permit::PERMIT_TYPEHASH);
}

// =============================================================================
// Constants Tests
// =============================================================================

#[test]
fn secp256k1_n_div_2_is_correct() {
	// n/2 should be 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5D576E7357A4501DDFE92F46681B20A0
	let expected = [
		0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
		0xFF, 0x5D, 0x57, 0x6E, 0x73, 0x57, 0xA4, 0x50, 0x1D, 0xDF, 0xE9, 0x2F, 0x46, 0x68, 0x1B,
		0x20, 0xA0,
	];
	assert_eq!(permit::SECP256K1_N_DIV_2, expected);
}

#[test]
fn encoded_length_constants_are_correct() {
	assert_eq!(permit::DOMAIN_SEPARATOR_ENCODED_LEN, 160);
	assert_eq!(permit::PERMIT_STRUCT_ENCODED_LEN, 192);
	assert_eq!(permit::DIGEST_PREFIX_LEN, 66);
}

// =============================================================================
// Precompile integration tests
// =============================================================================
//
// The tests above exercise `permit::Pallet` directly in isolation. The tests
// in this submodule drive the same logic end-to-end through the precompile
// dispatcher via `bare_call`, signing each digest at runtime with Hardhat
// account #0's private key. They cover precompile-level concerns the pallet
// tests cannot:
//
//   * the allowance-update branches in `permit()` (fresh-approve, revoke, noop,
//     cancel-then-approve), each pinned by a dedicated test
//   * `with_transaction` rollback (nonce, allowance, deposit, contract events)
//   * Approval event emission
//   * the dispatcher's revert-reason mapping
//   * cross-prefix (verifying-contract) domain separation
//
// Wrapped in a submodule so we can import alloy's `U256` without conflicting
// with the `sp_core::U256` used by the pallet-level tests above.
mod precompile {
	use super::*;
	use crate::{
		alloy::hex,
		mock::{Assets, Balances, RuntimeEvent, RuntimeOrigin, System},
		test_helpers::{
			assert_contract_event, set_prefix_in_address, setup_asset_for_prefix, ICaller,
			PRECOMPILE_ADDRESS_PREFIX, PRECOMPILE_ADDRESS_PREFIX_FOREIGN,
		},
		IERC20::{self, IERC20Events},
	};
	use alloy::primitives::U256 as AlloyU256;
	use frame_support::{
		assert_ok,
		traits::{Currency, Get},
	};
	use pallet_revive::{
		precompiles::{alloy, alloy::sol_types::SolCall, TransactionLimits},
		AddressMapper, Code, ExecConfig,
	};
	use sp_runtime::Weight;
	use test_case::test_case;

	// HARDHAT_ACCOUNT_0 is brought in via `use super::*;` above.

	const HARDHAT_ACCOUNT_0_SEED: &[u8] =
		b"0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

	const PERMIT_TOKEN_NAME: &[u8] = b"Test Token";
	/// 2100-01-01 00:00 UTC in seconds — used as a deadline that will not
	/// expire during the lifetime of the test runtime.
	const FAR_FUTURE_DEADLINE: u64 = 4_102_444_800;
	/// Account id used as the EIP-2612 spender across permit tests. Arbitrary
	/// non-zero u64 — the value is irrelevant as long as it differs from the
	/// signer (Hardhat #0) and the relayer.
	const SPENDER_ACCOUNT: u64 = 987_654_321;
	/// Account id used as the relayer that submits the permit on behalf of
	/// the signer. Arbitrary non-zero u64.
	const SUBMITTER_ACCOUNT: u64 = 555;
	/// Free balance given to a funded account — large enough to cover the
	/// permit-call storage deposits and the `Caller` fixture's contract
	/// deposit in the STATICCALL test.
	const SUBMITTER_FUNDING: u64 = 1_000_000_000_000;
	/// Account id used to deploy the `Caller` fixture contract in the
	/// STATICCALL test. Distinct from the relayer / signer / spender so a
	/// regression that crosses roles is visible.
	const DEPLOYER_ACCOUNT: u64 = 1234;

	/// The `u64` AccountId that the runtime's `AddressMapper` derives from
	/// `HARDHAT_ACCOUNT_0`. Derived via the trait so this stays correct if
	/// the mapper's derivation ever changes.
	fn hardhat_account_id() -> u64 {
		<Test as pallet_revive::Config>::AddressMapper::to_account_id(&HARDHAT_ACCOUNT_0)
	}

	/// Sign an EIP-2612 permit digest with Hardhat #0's key. Reads the
	/// current on-chain nonce so the digest is valid for an immediate
	/// `permit()` call. Returns `(v, r, s)` in Ethereum format
	/// (v ∈ {27, 28}).
	fn sign_permit(
		asset_addr: H160,
		spender: H160,
		value: AlloyU256,
		deadline: AlloyU256,
	) -> (u8, [u8; 32], [u8; 32]) {
		let nonce = permit::Pallet::<Test>::nonce(&asset_addr, &HARDHAT_ACCOUNT_0);
		let value_bytes: [u8; 32] = value.to_be_bytes();
		let deadline_bytes: [u8; 32] = deadline.to_be_bytes();

		let digest = permit::Pallet::<Test>::permit_digest(
			&asset_addr,
			PERMIT_TOKEN_NAME,
			&HARDHAT_ACCOUNT_0,
			&spender,
			&value_bytes,
			&nonce,
			&deadline_bytes,
		);

		// Sign via the keystore — works in both native and WASM, mirroring
		// the approach used in benchmarking.rs.
		let key_type = sp_core::crypto::KeyTypeId(*b"prmt");
		let pub_key =
			sp_io::crypto::ecdsa_generate(key_type, Some(HARDHAT_ACCOUNT_0_SEED.to_vec()));
		let sig = sp_io::crypto::ecdsa_sign_prehashed(key_type, &pub_key, &digest)
			.expect("signing with Hardhat #0 must succeed; qed");
		let sig_bytes: &[u8; 65] = sig.as_ref();
		let r: [u8; 32] = sig_bytes[0..32].try_into().expect("r is 32 bytes");
		let s: [u8; 32] = sig_bytes[32..64].try_into().expect("s is 32 bytes");
		let v: u8 = sig_bytes[64] + 27;
		(v, r, s)
	}

	/// Configures an asset owned by Hardhat #0 with metadata name
	/// [`PERMIT_TOKEN_NAME`], returning the asset's precompile address.
	/// Hardhat #0 is set as the asset admin so freeze tests can drive
	/// `freeze_asset` from that account.
	fn setup_permit_asset(asset_id: u32, prefix: u16) -> H160 {
		let asset_addr = H160::from(set_prefix_in_address(prefix));
		let owner = hardhat_account_id();
		Balances::make_free_balance_be(&owner, 1_000);
		setup_asset_for_prefix(asset_id, prefix);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::force_set_metadata(
			RuntimeOrigin::root(),
			asset_id,
			PERMIT_TOKEN_NAME.to_vec(),
			b"TST".to_vec(),
			18,
			false,
		));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));
		asset_addr
	}

	/// Submits a `permit()` call via the precompile, returning the bare-call
	/// result so callers can distinguish revert paths.
	fn raw_permit(
		sender: u64,
		asset_addr: H160,
		owner: H160,
		spender: H160,
		value: AlloyU256,
		deadline: AlloyU256,
		v: u8,
		r: [u8; 32],
		s: [u8; 32],
	) -> pallet_revive::ContractResult<pallet_revive::ExecReturnValue, u64> {
		let data = IERC20::permitCall {
			owner: owner.0.into(),
			spender: spender.0.into(),
			value,
			deadline,
			v,
			r: r.into(),
			s: s.into(),
		}
		.abi_encode();
		pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(sender),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		)
	}

	/// Signs the current-nonce permit and submits it, asserting success.
	fn permit_sign_and_call(
		submitter: u64,
		asset_addr: H160,
		spender: H160,
		value: AlloyU256,
		deadline: AlloyU256,
	) {
		let (v, r, s) = sign_permit(asset_addr, spender, value, deadline);
		let result =
			raw_permit(submitter, asset_addr, HARDHAT_ACCOUNT_0, spender, value, deadline, v, r, s);
		assert!(result.result.is_ok(), "permit precompile call failed: {:?}", result);
		assert!(!result.result.unwrap().did_revert(), "permit call reverted");
	}

	/// Asserts a permit submission trapped with `Err(DispatchError::Module(_))`
	/// matching the given pallet error variant. Use for the
	/// `Error::Error(DispatchError)` trap path; for clean reverts use
	/// `assert_permit_reverted_with`.
	///
	/// Strict equality against the lifted `DispatchError` ensures unrelated
	/// failure modes (out-of-gas, panics, weight exhaustion, a different
	/// pallet error) cannot silently keep the test green if the failure
	/// surface changes.
	fn assert_permit_dispatch_err<E>(
		result: pallet_revive::ContractResult<pallet_revive::ExecReturnValue, u64>,
		expected: E,
	) where
		E: Into<sp_runtime::DispatchError>,
	{
		use sp_runtime::DispatchError;
		let expected: DispatchError = expected.into();
		let actual = match result.result {
			Err(e) => e,
			Ok(v) => {
				panic!("permit expected to trap with {:?}; call returned Ok({:?})", expected, v)
			},
		};
		assert!(
			matches!(actual, DispatchError::Module(_)),
			"expected DispatchError::Module(...), got {:?}",
			actual,
		);
		assert_eq!(actual, expected);
	}

	/// Asserts the call cleanly reverted (not trapped) and that the revert
	/// reason contains `expected_substring`.
	///
	/// **Avoid prefix collisions** — pass the *full* reason string. For
	/// example, `"Invalid signature"` is a prefix of `"Invalid signature v
	/// value"`, and matching the bare prefix would silently accept either.
	fn assert_permit_reverted_with(
		result: pallet_revive::ContractResult<pallet_revive::ExecReturnValue, u64>,
		expected_substring: &str,
	) {
		let exec = match result.result.as_ref() {
			Ok(v) => v,
			Err(e) => panic!(
				"expected revert with reason {:?}, got dispatch error: {:?}",
				expected_substring, e
			),
		};
		assert!(
			exec.did_revert(),
			"expected revert with reason {:?}, but call succeeded: {:?}",
			expected_substring,
			exec,
		);
		let needle = expected_substring.as_bytes();
		assert!(
			exec.data.windows(needle.len()).any(|w| w == needle),
			"expected revert reason to contain {:?}, got 0x{}",
			expected_substring,
			hex::encode(&exec.data),
		);
	}

	/// Asserts no `ContractEmitted` event was raised by `contract`. Used to
	/// verify event rollback when a permit fails inside `with_transaction`.
	fn assert_no_contract_event_from(contract: H160) {
		let any = System::events().iter().any(|er| {
			matches!(
				&er.event,
				RuntimeEvent::Revive(pallet_revive::Event::ContractEmitted { contract: c, .. }) if *c == contract,
			)
		});
		assert!(!any, "expected no ContractEmitted events from {:?}", contract);
	}

	fn fund_submitter(account: u64) {
		Balances::make_free_balance_be(&account, SUBMITTER_FUNDING);
	}

	/// Common setup shared by most permit tests: an asset registered behind
	/// the given precompile prefix, the signer (Hardhat #0) as owner, a
	/// fixed spender account/address, a funded relayer (`submitter`), and a
	/// far-future deadline. Tests that need a different shape (e.g.
	/// zero-address callers, or cross-prefix signing) build their state
	/// directly.
	struct PermitSetup {
		asset_id: u32,
		asset_addr: H160,
		owner_account: u64,
		spender_account: u64,
		spender_addr: H160,
		submitter: u64,
		deadline: AlloyU256,
	}

	fn permit_setup(prefix: u16) -> PermitSetup {
		let asset_id = 0u32;
		let asset_addr = setup_permit_asset(asset_id, prefix);
		let owner_account = hardhat_account_id();
		let spender_account = SPENDER_ACCOUNT;
		let spender_addr =
			<Test as pallet_revive::Config>::AddressMapper::to_address(&spender_account);
		let submitter = SUBMITTER_ACCOUNT;
		fund_submitter(submitter);
		let deadline = AlloyU256::from(FAR_FUTURE_DEADLINE);
		PermitSetup {
			asset_id,
			asset_addr,
			owner_account,
			spender_account,
			spender_addr,
			submitter,
			deadline,
		}
	}

	/// Drives `permit()` through the fresh-approve and revoke branches:
	/// 0→100 (fresh), 100→0 (revoke), 0→50 (fresh again). Verifies
	/// allowance, deposit, nonce, and the Approval event at each step. The
	/// headline permit integration test — kept parametrized over both
	/// prefixes for confidence on the cross-prefix asset_id extraction path.
	/// The non-zero→non-zero branch is covered by `permit_nonzero_to_nonzero`.
	#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
	#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
	fn permit_set_and_revoke(asset_index: u16) {
		use frame_support::traits::fungibles::approvals::Inspect;

		new_test_ext().execute_with(|| {
			let setup = permit_setup(asset_index);
			let deposit: u64 = <Test as pallet_assets::Config>::ApprovalDeposit::get();

			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::zero()
			);

			// 0 → 100: fresh approval.
			permit_sign_and_call(
				setup.submitter,
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
			);
			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				100
			);
			assert_eq!(Balances::reserved_balance(&setup.owner_account), deposit);
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::one()
			);
			assert_contract_event(
				setup.asset_addr,
				IERC20Events::Approval(IERC20::Approval {
					owner: HARDHAT_ACCOUNT_0.0.into(),
					spender: setup.spender_addr.0.into(),
					value: AlloyU256::from(100),
				}),
			);

			// 100 → 0: revoke. ERC-20 conformance: must fire Approval(_, _, 0).
			permit_sign_and_call(
				setup.submitter,
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(0),
				setup.deadline,
			);
			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				0
			);
			assert_eq!(Balances::reserved_balance(&setup.owner_account), 0);
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::from(2)
			);
			assert_contract_event(
				setup.asset_addr,
				IERC20Events::Approval(IERC20::Approval {
					owner: HARDHAT_ACCOUNT_0.0.into(),
					spender: setup.spender_addr.0.into(),
					value: AlloyU256::from(0),
				}),
			);

			// 0 → 50: fresh approval again.
			permit_sign_and_call(
				setup.submitter,
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(50),
				setup.deadline,
			);
			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				50
			);
			assert_eq!(Balances::reserved_balance(&setup.owner_account), deposit);
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::from(3)
			);
			assert_contract_event(
				setup.asset_addr,
				IERC20Events::Approval(IERC20::Approval {
					owner: HARDHAT_ACCOUNT_0.0.into(),
					spender: setup.spender_addr.0.into(),
					value: AlloyU256::from(50),
				}),
			);
		});
	}

	/// `permit(value=0)` against a non-existent allowance succeeds silently —
	/// no allowance entry, no deposit, but the nonce IS consumed and an
	/// `Approval(_, _, 0)` event IS emitted (matches ERC-20 set semantics).
	/// Pins the `new_amount.is_zero() && current.is_zero()` noop branch in
	/// the `permit` dispatcher.
	#[test]
	fn permit_zero_on_nonexistent_is_noop() {
		use frame_support::traits::fungibles::approvals::Inspect;

		new_test_ext().execute_with(|| {
			let setup = permit_setup(PRECOMPILE_ADDRESS_PREFIX);

			permit_sign_and_call(
				setup.submitter,
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(0),
				setup.deadline,
			);

			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				0
			);
			assert_eq!(Balances::reserved_balance(&setup.owner_account), 0);
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::one()
			);
			assert_contract_event(
				setup.asset_addr,
				IERC20Events::Approval(IERC20::Approval {
					owner: HARDHAT_ACCOUNT_0.0.into(),
					spender: setup.spender_addr.0.into(),
					value: AlloyU256::from(0),
				}),
			);
		});
	}

	/// Overwriting a non-zero allowance via permit must use set semantics:
	/// the allowance equals the new value (not the sum), and only one
	/// deposit is held throughout. Pins the cancel-then-approve branch in
	/// the `permit` dispatcher (the `!new_amount.is_zero() && !current.is_zero()` arm).
	#[test]
	fn permit_nonzero_to_nonzero() {
		use frame_support::traits::fungibles::approvals::Inspect;

		new_test_ext().execute_with(|| {
			let setup = permit_setup(PRECOMPILE_ADDRESS_PREFIX);
			let deposit: u64 = <Test as pallet_assets::Config>::ApprovalDeposit::get();

			permit_sign_and_call(
				setup.submitter,
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
			);
			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				100
			);
			assert_eq!(Balances::reserved_balance(&setup.owner_account), deposit);

			// 100 → 50, no zeroing in between.
			permit_sign_and_call(
				setup.submitter,
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(50),
				setup.deadline,
			);
			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				50
			);
			assert_eq!(Balances::reserved_balance(&setup.owner_account), deposit);

			// 50 → 200: confirm both directions.
			permit_sign_and_call(
				setup.submitter,
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(200),
				setup.deadline,
			);
			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				200
			);
			assert_eq!(Balances::reserved_balance(&setup.owner_account), deposit);
		});
	}

	/// If the inner allowance update fails after `use_permit` succeeded, the
	/// whole storage transaction must roll back — nonce, allowance, deposit,
	/// and (importantly) any contract event from the closure body. We
	/// trigger the inner failure by freezing the asset after signing.
	///
	/// The `assert_no_contract_event_from` here also implicitly pins that
	/// `pallet_revive`'s contract events ARE rolled back by
	/// `frame_support::storage::with_transaction`.
	#[test]
	fn permit_rollback_does_not_increment_nonce() {
		use frame_support::traits::fungibles::approvals::Inspect;

		new_test_ext().execute_with(|| {
			let setup = permit_setup(PRECOMPILE_ADDRESS_PREFIX);

			let (v, r, s) = sign_permit(
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
			);

			assert_ok!(Assets::freeze_asset(
				RuntimeOrigin::signed(setup.owner_account),
				setup.asset_id
			));

			let result = raw_permit(
				setup.submitter,
				setup.asset_addr,
				HARDHAT_ACCOUNT_0,
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
				v,
				r,
				s,
			);
			assert_permit_dispatch_err(result, pallet_assets::Error::<Test>::AssetNotLive);

			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::zero(),
				"nonce must remain 0 when the storage transaction rolls back"
			);
			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				0
			);
			assert_eq!(Balances::reserved_balance(&setup.owner_account), 0);
			assert_no_contract_event_from(setup.asset_addr);
		});
	}

	/// A failed permit must not destroy a prior allowance. Pre-approve(100),
	/// freeze, submit permit(200) — rollback must leave the prior allowance
	/// and its deposit untouched.
	///
	/// Note: an even stronger test would exercise the cancel-then-approve
	/// order directly (cancel succeeds, approve fails, rollback restores).
	/// But both pallet-assets entry points gate on `AssetStatus::Live` as
	/// their first check, so that exact sequence cannot be constructed in
	/// this mock.
	#[test]
	fn permit_rollback_preserves_prior_allowance() {
		use frame_support::traits::fungibles::approvals::Inspect;

		new_test_ext().execute_with(|| {
			let setup = permit_setup(PRECOMPILE_ADDRESS_PREFIX);
			let deposit: u64 = <Test as pallet_assets::Config>::ApprovalDeposit::get();

			assert_ok!(Assets::approve_transfer(
				RuntimeOrigin::signed(setup.owner_account),
				setup.asset_id,
				setup.spender_account,
				100,
			));
			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				100
			);
			assert_eq!(Balances::reserved_balance(&setup.owner_account), deposit);

			let (v, r, s) = sign_permit(
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(200),
				setup.deadline,
			);
			assert_ok!(Assets::freeze_asset(
				RuntimeOrigin::signed(setup.owner_account),
				setup.asset_id
			));

			let result = raw_permit(
				setup.submitter,
				setup.asset_addr,
				HARDHAT_ACCOUNT_0,
				setup.spender_addr,
				AlloyU256::from(200),
				setup.deadline,
				v,
				r,
				s,
			);
			assert_permit_dispatch_err(result, pallet_assets::Error::<Test>::AssetNotLive);

			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				100
			);
			assert_eq!(Balances::reserved_balance(&setup.owner_account), deposit);
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::zero()
			);
		});
	}

	/// `to_balance` failure (value > runtime Balance capacity) returns
	/// `Error::Revert("Balance conversion failed")` *after* `use_permit`
	/// has incremented the nonce. The `with_transaction` wrapper must roll
	/// the nonce back. Distinct failure surface from the frozen-asset test
	/// (revert vs DispatchError trap).
	///
	/// Note: this test depends on the mock's `Balance = u64`. On a runtime
	/// with `Balance = u128` the same input would not overflow `to_balance`.
	#[test]
	fn permit_value_overflow_rolls_back() {
		use frame_support::traits::fungibles::approvals::Inspect;

		new_test_ext().execute_with(|| {
			let setup = permit_setup(PRECOMPILE_ADDRESS_PREFIX);

			let huge = AlloyU256::from(1u128 << 64);
			let (v, r, s) = sign_permit(setup.asset_addr, setup.spender_addr, huge, setup.deadline);
			let result = raw_permit(
				setup.submitter,
				setup.asset_addr,
				HARDHAT_ACCOUNT_0,
				setup.spender_addr,
				huge,
				setup.deadline,
				v,
				r,
				s,
			);
			assert_permit_reverted_with(result, "Balance conversion failed");
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::zero(),
				"nonce must roll back when to_balance fails after use_permit"
			);
			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				0
			);
			assert_no_contract_event_from(setup.asset_addr);
		});
	}

	/// If the owner can't afford the `ApprovalDeposit`, `do_approve_transfer`
	/// returns a `DispatchError` (Error::Error → trap). Distinct failure
	/// path from the revert-based `to_balance` test.
	#[test]
	fn permit_rejects_when_owner_lacks_deposit_balance() {
		use frame_support::traits::fungibles::approvals::Inspect;

		new_test_ext().execute_with(|| {
			let setup = permit_setup(PRECOMPILE_ADDRESS_PREFIX);

			Balances::make_free_balance_be(&setup.owner_account, 0);

			let (v, r, s) = sign_permit(
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
			);
			let result = raw_permit(
				setup.submitter,
				setup.asset_addr,
				HARDHAT_ACCOUNT_0,
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
				v,
				r,
				s,
			);
			assert_permit_dispatch_err(result, pallet_balances::Error::<Test>::InsufficientBalance);
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::zero(),
				"nonce must not advance when the deposit reserve fails"
			);
			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				0
			);
			assert_no_contract_event_from(setup.asset_addr);
		});
	}

	/// A signature for asset A must NOT be replayable against asset B —
	/// pins the `verifyingContract` field of the EIP-712 domain. We register
	/// the same underlying asset under both prefixes, sign for one, submit
	/// to the other; both directions are tested.
	#[test_case(PRECOMPILE_ADDRESS_PREFIX, PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
	#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN, PRECOMPILE_ADDRESS_PREFIX)]
	fn permit_signature_bound_to_verifying_contract(sign_prefix: u16, submit_prefix: u16) {
		new_test_ext().execute_with(|| {
			let setup = permit_setup(sign_prefix);
			if sign_prefix != PRECOMPILE_ADDRESS_PREFIX_FOREIGN &&
				submit_prefix == PRECOMPILE_ADDRESS_PREFIX_FOREIGN
			{
				crate::pallet::Pallet::<Test>::insert_asset_mapping(&setup.asset_id)
					.expect("foreign asset mapping must insert");
			}

			let asset_addr_signed = setup.asset_addr;
			let asset_addr_submitted = H160::from(set_prefix_in_address(submit_prefix));
			assert_ne!(asset_addr_signed, asset_addr_submitted);

			let (v, r, s) = sign_permit(
				asset_addr_signed,
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
			);

			let result = raw_permit(
				setup.submitter,
				asset_addr_submitted,
				HARDHAT_ACCOUNT_0,
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
				v,
				r,
				s,
			);
			assert_permit_reverted_with(result, "Signer does not match owner");
			assert_eq!(
				permit::Pallet::<Test>::nonce(&asset_addr_signed, &HARDHAT_ACCOUNT_0),
				U256::zero()
			);
			assert_eq!(
				permit::Pallet::<Test>::nonce(&asset_addr_submitted, &HARDHAT_ACCOUNT_0),
				U256::zero()
			);
		});
	}

	/// Renaming an asset invalidates outstanding permits — the EIP-712
	/// domain separator binds the asset's current `name` metadata. Kept
	/// parametrized over both prefixes for confidence on this
	/// security-relevant invariant.
	#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
	#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
	fn permit_rejects_after_token_name_change(asset_index: u16) {
		new_test_ext().execute_with(|| {
			let setup = permit_setup(asset_index);

			let (v, r, s) = sign_permit(
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
			);

			assert_ok!(Assets::force_set_metadata(
				RuntimeOrigin::root(),
				setup.asset_id,
				b"Renamed Token".to_vec(),
				b"RNM".to_vec(),
				18,
				false,
			));

			let result = raw_permit(
				setup.submitter,
				setup.asset_addr,
				HARDHAT_ACCOUNT_0,
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
				v,
				r,
				s,
			);
			assert_permit_reverted_with(result, "Signer does not match owner");
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::zero()
			);
		});
	}

	/// EIP-2612 forbids the zero address as `owner`. The early
	/// `owner.is_zero()` check inside `do_verify_permit` runs before signature
	/// verification, so dummy `(v, r, s)` is fine.
	#[test]
	fn permit_rejects_zero_owner() {
		new_test_ext().execute_with(|| {
			let setup = permit_setup(PRECOMPILE_ADDRESS_PREFIX);

			let result = raw_permit(
				setup.submitter,
				setup.asset_addr,
				H160::zero(),
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
				27,
				[0u8; 32],
				[0u8; 32],
			);
			assert_permit_reverted_with(result, "Invalid owner address");
			assert_eq!(Balances::reserved_balance(&setup.owner_account), 0);
			// Nonce on the (zero) owner the call would have advanced, plus
			// nonce on the real signer for good measure — both must stay 0
			// to pin the early-reject ordering.
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &H160::zero()),
				U256::zero(),
			);
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::zero(),
			);
			assert_no_contract_event_from(setup.asset_addr);
		});
	}

	/// EIP-2612 forbids the zero address as `spender`. Same rationale as
	/// `permit_rejects_zero_owner` — the spender zero-check runs before
	/// signature verification.
	#[test]
	fn permit_rejects_zero_spender() {
		new_test_ext().execute_with(|| {
			let setup = permit_setup(PRECOMPILE_ADDRESS_PREFIX);

			let result = raw_permit(
				setup.submitter,
				setup.asset_addr,
				HARDHAT_ACCOUNT_0,
				H160::zero(),
				AlloyU256::from(100),
				setup.deadline,
				27,
				[0u8; 32],
				[0u8; 32],
			);
			assert_permit_reverted_with(result, "Invalid spender address");
			assert_eq!(Balances::reserved_balance(&setup.owner_account), 0);
			// Nonce on the declared owner must stay 0; the early-reject
			// ordering would be broken if it advanced.
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::zero(),
			);
			assert_no_contract_event_from(setup.asset_addr);
		});
	}

	/// The deadline check uses strict `deadline < now` — a permit with
	/// `deadline == now` must be accepted. Pins this boundary against an
	/// inadvertent flip to `<=`. Not covered at the pallet level.
	#[test]
	fn permit_accepts_deadline_at_current_timestamp() {
		use frame_support::traits::fungibles::approvals::Inspect;

		new_test_ext().execute_with(|| {
			let setup = permit_setup(PRECOMPILE_ADDRESS_PREFIX);

			let now_seconds: u64 = 2_000_000_000;
			pallet_timestamp::Pallet::<Test>::set_timestamp(now_seconds * 1_000);

			let deadline = AlloyU256::from(now_seconds);
			permit_sign_and_call(
				setup.submitter,
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(100),
				deadline,
			);

			assert_eq!(
				Assets::allowance(setup.asset_id, &setup.owner_account, &setup.spender_account),
				100
			);
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::one()
			);
		});
	}

	/// Exercises the `secp256k1_ecdsa_recover` failure path — one of the
	/// permit-pallet error variants the dispatcher maps to a revert string.
	/// `r = 0` is not a valid signature component (the implied curve point
	/// would have x = 0, but `0³ + 7` has no square root mod p on
	/// secp256k1), so recovery returns `Err`.
	///
	/// Caveat: `"Invalid signature"` is a prefix of `"Invalid signature v
	/// value"`, so the substring matcher cannot, on its own, distinguish
	/// the two reasons. The test inputs are constructed so the v-range
	/// branch is unreachable (`v = 27`, `s = 0` in lower half), making
	/// recovery failure the only "Invalid signature*" error this path can
	/// fire.
	#[test]
	fn permit_rejects_recovery_failure() {
		new_test_ext().execute_with(|| {
			let setup = permit_setup(PRECOMPILE_ADDRESS_PREFIX);

			let result = raw_permit(
				setup.submitter,
				setup.asset_addr,
				HARDHAT_ACCOUNT_0,
				setup.spender_addr,
				AlloyU256::from(100),
				setup.deadline,
				27,
				[0u8; 32],
				[0u8; 32],
			);
			assert_permit_reverted_with(result, "Invalid signature");
			assert_eq!(
				permit::Pallet::<Test>::nonce(&setup.asset_addr, &HARDHAT_ACCOUNT_0),
				U256::zero()
			);
			assert_no_contract_event_from(setup.asset_addr);
		});
	}

	/// `permit()` is a state-changing call and must be rejected inside a
	/// STATICCALL context. The dispatcher's read-only check (the match arm
	/// guarding `transfer | approve | transferFrom | permit` against
	/// `env.is_read_only()`) is what guards this — a regression that drops
	/// `IERC20Calls::permit(_)` from that arm would silently allow
	/// state-changing permits in a static context. Mirrors the
	/// `delegatecall_is_rejected` test in `tests.rs`.
	#[test]
	fn permit_staticcall_is_rejected() {
		new_test_ext().execute_with(|| {
			let asset_id = 0u32;
			let asset_addr = H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX));
			let deployer = DEPLOYER_ACCOUNT;
			Balances::make_free_balance_be(&deployer, SUBMITTER_FUNDING);
			assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, deployer, true, 1));

			let (init_code, _) = pallet_revive_fixtures::compile_module_with_type(
				"Caller",
				pallet_revive_fixtures::FixtureType::Solc,
			)
			.expect("Caller fixture must be compiled");
			let caller_addr = pallet_revive::Pallet::<Test>::bare_instantiate(
				RuntimeOrigin::signed(deployer),
				0u32.into(),
				TransactionLimits::WeightAndDeposit {
					weight_limit: Weight::MAX,
					deposit_limit: u64::MAX,
				},
				Code::Upload(init_code),
				vec![],
				None,
				&ExecConfig::new_substrate_tx(),
			)
			.result
			.expect("Caller deployment must succeed")
			.addr;

			// Signature contents are irrelevant — the read-only check fires
			// before any of them are inspected.
			let permit_calldata = IERC20::permitCall {
				owner: [0u8; 20].into(),
				spender: [0u8; 20].into(),
				value: AlloyU256::from(0),
				deadline: AlloyU256::from(0),
				v: 27,
				r: [0u8; 32].into(),
				s: [0u8; 32].into(),
			}
			.abi_encode();

			let calldata = ICaller::staticCallCall {
				callee: alloy::primitives::Address::from(asset_addr.0),
				data: permit_calldata.into(),
				gas: u64::MAX,
			}
			.abi_encode();

			let result = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(deployer),
				caller_addr,
				0u32.into(),
				TransactionLimits::WeightAndDeposit {
					weight_limit: Weight::MAX,
					deposit_limit: u64::MAX,
				},
				calldata,
				&ExecConfig::new_substrate_tx(),
			)
			.result
			.expect("outer call must succeed");

			let ret = ICaller::staticCallCall::abi_decode_returns(&result.data)
				.expect("return must decode as (bool, bytes)");
			assert!(!ret.success, "STATICCALL to permit() must be rejected");
		});
	}

	/// Drives `nonces(owner)` through `bare_call` to pin the dispatch arm
	/// for the `nonces` selector. A regression that mis-routes the selector
	/// would not be caught by tests that read `permit::Pallet::nonce`
	/// storage directly.
	#[test]
	fn nonces_via_precompile() {
		new_test_ext().execute_with(|| {
			let setup = permit_setup(PRECOMPILE_ADDRESS_PREFIX);

			let read_nonce = |asset_addr: H160| -> AlloyU256 {
				let data = IERC20::noncesCall { owner: HARDHAT_ACCOUNT_0.0.into() }.abi_encode();
				let bytes = pallet_revive::Pallet::<Test>::bare_call(
					RuntimeOrigin::signed(setup.submitter),
					asset_addr,
					0u32.into(),
					TransactionLimits::WeightAndDeposit {
						weight_limit: Weight::MAX,
						deposit_limit: u64::MAX,
					},
					data,
					&ExecConfig::new_substrate_tx(),
				)
				.result
				.expect("nonces() call must succeed")
				.data;
				IERC20::noncesCall::abi_decode_returns(&bytes).expect("decode nonces return")
			};

			assert_eq!(read_nonce(setup.asset_addr), AlloyU256::from(0));

			permit_sign_and_call(
				setup.submitter,
				setup.asset_addr,
				setup.spender_addr,
				AlloyU256::from(50),
				setup.deadline,
			);

			assert_eq!(read_nonce(setup.asset_addr), AlloyU256::from(1));
		});
	}
}
