// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT-0

// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
// of the Software, and to permit persons to whom the Software is furnished to do
// so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Tests for pallet-example-authorization-tx-extension.

use codec::Encode;
use frame_support::{
	assert_noop,
	dispatch::GetDispatchInfo,
	pallet_prelude::{InvalidTransaction, TransactionValidityError},
};
use pallet_verify_signature::VerifySignature;
use sp_keyring::Sr25519Keyring;
use sp_runtime::{
	generic::ExtensionVersion,
	traits::{Applyable, Checkable, IdentityLookup, TransactionExtension},
	MultiSignature, MultiSigner,
};

use crate::{extensions::AuthorizeCoownership, mock::*, pallet_assets};

#[test]
fn create_asset_works() {
	new_test_ext().execute_with(|| {
		let alice_keyring = Sr25519Keyring::Alice;
		let alice_account = AccountId::from(alice_keyring.public());
		// Simple call to create asset with Id `42`.
		let create_asset_call =
			RuntimeCall::Assets(pallet_assets::Call::create_asset { asset_id: 42 });
		let ext_version: ExtensionVersion = 0;
		// Create extension that will be used for dispatch.
		let initial_nonce = 23;
		let tx_ext = (
			frame_system::CheckNonce::<Runtime>::from(initial_nonce),
			AuthorizeCoownership::<Runtime, MultiSigner, MultiSignature>::default(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckEra::<Runtime>::from(sp_runtime::generic::Era::immortal()),
		);
		// Create the transaction signature, to be used in the top level `VerifyMultiSignature`
		// extension.
		let tx_sign = MultiSignature::Sr25519(
			(&(ext_version, &create_asset_call), &tx_ext, tx_ext.implicit().unwrap())
				.using_encoded(|e| alice_keyring.sign(&sp_io::hashing::blake2_256(e))),
		);
		// Add the signature to the extension.
		let tx_ext = (
			VerifySignature::new_with_signature(tx_sign, alice_account.clone()),
			frame_system::CheckNonce::<Runtime>::from(initial_nonce),
			AuthorizeCoownership::<Runtime, MultiSigner, MultiSignature>::default(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckEra::<Runtime>::from(sp_runtime::generic::Era::immortal()),
		);
		// Create the transaction and we're ready for dispatch.
		let uxt = UncheckedExtrinsic::new_transaction(create_asset_call, tx_ext);
		// Check Extrinsic validity and apply it.
		let uxt_info = uxt.get_dispatch_info();
		let uxt_len = uxt.using_encoded(|e| e.len());
		// Manually pay for Alice's nonce.
		frame_system::Account::<Runtime>::mutate(&alice_account, |info| {
			info.nonce = initial_nonce;
			info.providers = 1;
		});
		// Check should pass.
		let xt = <UncheckedExtrinsic as Checkable<IdentityLookup<AccountId>>>::check(
			uxt,
			&Default::default(),
		)
		.unwrap();
		// Apply the extrinsic.
		let res = xt.apply::<Runtime>(&uxt_info, uxt_len).unwrap();

		// Asserting the results.
		assert_eq!(frame_system::Account::<Runtime>::get(&alice_account).nonce, initial_nonce + 1);
		assert_eq!(
			pallet_assets::AssetOwners::<Runtime>::get(42),
			Some(pallet_assets::Owner::<AccountId>::Single(alice_account))
		);
		assert!(res.is_ok());
	});
}

#[docify::export]
#[test]
fn create_coowned_asset_works() {
	new_test_ext().execute_with(|| {
		let alice_keyring = Sr25519Keyring::Alice;
		let bob_keyring = Sr25519Keyring::Bob;
		let charlie_keyring = Sr25519Keyring::Charlie;
		let alice_account = AccountId::from(alice_keyring.public());
		let bob_account = AccountId::from(bob_keyring.public());
		let charlie_account = AccountId::from(charlie_keyring.public());
		// Simple call to create asset with Id `42`.
		let create_asset_call =
			RuntimeCall::Assets(pallet_assets::Call::create_asset { asset_id: 42 });
		let ext_version: ExtensionVersion = 0;
		// Create the inner transaction extension, to be signed by our coowners, Alice and Bob.
		let inner_ext: InnerTxExtension = (
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckEra::<Runtime>::from(sp_runtime::generic::Era::immortal()),
		);
		// Create the payload Alice and Bob need to sign.
		let inner_payload =
			(&(ext_version, &create_asset_call), &inner_ext, inner_ext.implicit().unwrap());
		// Create Alice's signature.
		let alice_inner_sig = MultiSignature::Sr25519(
			inner_payload.using_encoded(|e| alice_keyring.sign(&sp_io::hashing::blake2_256(e))),
		);
		// Create Bob's signature.
		let bob_inner_sig = MultiSignature::Sr25519(
			inner_payload.using_encoded(|e| bob_keyring.sign(&sp_io::hashing::blake2_256(e))),
		);
		// Create the transaction extension, to be signed by the submitter of the extrinsic, let's
		// have it be Charlie.
		let initial_nonce = 23;
		let tx_ext = (
			frame_system::CheckNonce::<Runtime>::from(initial_nonce),
			AuthorizeCoownership::<Runtime, MultiSigner, MultiSignature>::new(
				(alice_keyring.into(), alice_inner_sig.clone()),
				(bob_keyring.into(), bob_inner_sig.clone()),
			),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckEra::<Runtime>::from(sp_runtime::generic::Era::immortal()),
		);
		// Create Charlie's transaction signature, to be used in the top level
		// `VerifyMultiSignature` extension.
		let tx_sign = MultiSignature::Sr25519(
			(&(ext_version, &create_asset_call), &tx_ext, tx_ext.implicit().unwrap())
				.using_encoded(|e| charlie_keyring.sign(&sp_io::hashing::blake2_256(e))),
		);
		// Add the signature to the extension.
		let tx_ext = (
			VerifySignature::new_with_signature(tx_sign, charlie_account.clone()),
			frame_system::CheckNonce::<Runtime>::from(initial_nonce),
			AuthorizeCoownership::<Runtime, MultiSigner, MultiSignature>::new(
				(alice_keyring.into(), alice_inner_sig),
				(bob_keyring.into(), bob_inner_sig),
			),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckEra::<Runtime>::from(sp_runtime::generic::Era::immortal()),
		);
		// Create the transaction and we're ready for dispatch.
		let uxt = UncheckedExtrinsic::new_transaction(create_asset_call, tx_ext);
		// Check Extrinsic validity and apply it.
		let uxt_info = uxt.get_dispatch_info();
		let uxt_len = uxt.using_encoded(|e| e.len());
		// Manually pay for Charlie's nonce.
		frame_system::Account::<Runtime>::mutate(&charlie_account, |info| {
			info.nonce = initial_nonce;
			info.providers = 1;
		});
		// Check should pass.
		let xt = <UncheckedExtrinsic as Checkable<IdentityLookup<AccountId>>>::check(
			uxt,
			&Default::default(),
		)
		.unwrap();
		// Apply the extrinsic.
		let res = xt.apply::<Runtime>(&uxt_info, uxt_len).unwrap();

		// Asserting the results.
		assert!(res.is_ok());
		assert_eq!(frame_system::Account::<Runtime>::get(charlie_account).nonce, initial_nonce + 1);
		assert_eq!(
			pallet_assets::AssetOwners::<Runtime>::get(42),
			Some(pallet_assets::Owner::<AccountId>::Double(alice_account, bob_account))
		);
	});
}

#[test]
fn inner_authorization_works() {
	new_test_ext().execute_with(|| {
		let alice_keyring = Sr25519Keyring::Alice;
		let bob_keyring = Sr25519Keyring::Bob;
		let charlie_keyring = Sr25519Keyring::Charlie;
		let charlie_account = AccountId::from(charlie_keyring.public());
		// Simple call to create asset with Id `42`.
		let create_asset_call =
			RuntimeCall::Assets(pallet_assets::Call::create_asset { asset_id: 42 });
		let ext_version: ExtensionVersion = 0;
		// Create the inner transaction extension, to be signed by our coowners, Alice and Bob. They
		// are going to sign this transaction as a mortal one.
		let inner_ext: InnerTxExtension = (
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			// Sign with mortal era check.
			frame_system::CheckEra::<Runtime>::from(sp_runtime::generic::Era::mortal(4, 0)),
		);
		// Create the payload Alice and Bob need to sign.
		let inner_payload = (&create_asset_call, &inner_ext, inner_ext.implicit().unwrap());
		// Create Alice's signature.
		let alice_inner_sig = MultiSignature::Sr25519(
			inner_payload.using_encoded(|e| alice_keyring.sign(&sp_io::hashing::blake2_256(e))),
		);
		// Create Bob's signature.
		let bob_inner_sig = MultiSignature::Sr25519(
			inner_payload.using_encoded(|e| bob_keyring.sign(&sp_io::hashing::blake2_256(e))),
		);
		// Create the transaction extension, to be signed by the submitter of the extrinsic, let's
		// have it be Charlie.
		let initial_nonce = 23;
		let tx_ext = (
			frame_system::CheckNonce::<Runtime>::from(initial_nonce),
			AuthorizeCoownership::<Runtime, MultiSigner, MultiSignature>::new(
				(alice_keyring.into(), alice_inner_sig.clone()),
				(bob_keyring.into(), bob_inner_sig.clone()),
			),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			// Construct the transaction as immortal with a different era check.
			frame_system::CheckEra::<Runtime>::from(sp_runtime::generic::Era::immortal()),
		);
		// Create Charlie's transaction signature, to be used in the top level
		// `VerifyMultiSignature` extension.
		let tx_sign = MultiSignature::Sr25519(
			(&(ext_version, &create_asset_call), &tx_ext, tx_ext.implicit().unwrap())
				.using_encoded(|e| charlie_keyring.sign(&sp_io::hashing::blake2_256(e))),
		);
		// Add the signature to the extension that Charlie signed.
		let tx_ext = (
			VerifySignature::new_with_signature(tx_sign, charlie_account.clone()),
			frame_system::CheckNonce::<Runtime>::from(initial_nonce),
			AuthorizeCoownership::<Runtime, MultiSigner, MultiSignature>::new(
				(alice_keyring.into(), alice_inner_sig),
				(bob_keyring.into(), bob_inner_sig),
			),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			// Construct the transaction as immortal with a different era check.
			frame_system::CheckEra::<Runtime>::from(sp_runtime::generic::Era::immortal()),
		);
		// Create the transaction and we're ready for dispatch.
		let uxt = UncheckedExtrinsic::new_transaction(create_asset_call, tx_ext);
		// Check Extrinsic validity and apply it.
		let uxt_info = uxt.get_dispatch_info();
		let uxt_len = uxt.using_encoded(|e| e.len());
		// Manually pay for Charlie's nonce.
		frame_system::Account::<Runtime>::mutate(charlie_account, |info| {
			info.nonce = initial_nonce;
			info.providers = 1;
		});
		// Check should pass.
		let xt = <UncheckedExtrinsic as Checkable<IdentityLookup<AccountId>>>::check(
			uxt,
			&Default::default(),
		)
		.unwrap();
		// The extrinsic should fail as the signature for the `AuthorizeCoownership` doesn't work
		// for the provided payload with the changed transaction mortality.
		assert_noop!(
			xt.apply::<Runtime>(&uxt_info, uxt_len),
			TransactionValidityError::Invalid(InvalidTransaction::Custom(100))
		);
	});
}
