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

use crate::*;
use frame_support::traits::tokens::fungible::Inspect;
use mock::*;
use sp_keyring::AccountKeyring;
use sp_runtime::{
	generic::Era,
	traits::{Applyable, Checkable, Hash, IdentityLookup},
	DispatchErrorWithPostInfo, MultiSignature,
};

fn create_tx_extension(account: AccountId) -> TxExtension {
	(
		frame_system::CheckNonZeroSender::<Runtime>::new(),
		frame_system::CheckSpecVersion::<Runtime>::new(),
		frame_system::CheckTxVersion::<Runtime>::new(),
		frame_system::CheckGenesis::<Runtime>::new(),
		frame_system::CheckMortality::<Runtime>::from(Era::immortal()),
		frame_system::CheckNonce::<Runtime>::from(
			frame_system::Pallet::<Runtime>::account(&account).nonce,
		),
		frame_system::CheckWeight::<Runtime>::new(),
		pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0),
	)
}

#[cfg(not(feature = "runtime-benchmarks"))]
fn create_meta_tx_extension(account: AccountId) -> MetaTxExtension {
	(
		frame_system::CheckNonZeroSender::<Runtime>::new(),
		frame_system::CheckSpecVersion::<Runtime>::new(),
		frame_system::CheckTxVersion::<Runtime>::new(),
		frame_system::CheckGenesis::<Runtime>::new(),
		frame_system::CheckMortality::<Runtime>::from(Era::immortal()),
		frame_system::CheckNonce::<Runtime>::from(
			frame_system::Pallet::<Runtime>::account(&account).nonce,
		),
	)
}

#[cfg(feature = "runtime-benchmarks")]
fn create_meta_tx_extension(account: AccountId) -> MetaTxExtension {
	crate::benchmarking::types::WeightlessExtension::<Runtime>::default()
}

fn create_signature<Call: Encode, Ext: Encode + TransactionExtension<RuntimeCall>>(
	call: Call,
	ext: Ext,
	signer: AccountKeyring,
) -> MultiSignature {
	MultiSignature::Sr25519(
		(call, ext.clone(), ext.implicit().unwrap()).using_encoded(|e| signer.sign(&e)),
	)
}

fn force_set_balance(account: AccountId) -> Balance {
	let balance = Balances::minimum_balance() * 100;
	Balances::force_set_balance(RuntimeOrigin::root(), account.into(), balance).unwrap();
	balance
}

fn apply_extrinsic(uxt: UncheckedExtrinsic) -> DispatchResultWithPostInfo {
	let uxt_info = uxt.get_dispatch_info();
	let uxt_len = uxt.using_encoded(|e| e.len());
	let xt = <UncheckedExtrinsic as Checkable<IdentityLookup<AccountId>>>::check(
		uxt,
		&Default::default(),
	)
	.unwrap();
	xt.apply::<Runtime>(&uxt_info, uxt_len).unwrap()
}

#[docify::export]
#[test]
fn sign_and_execute_meta_tx() {
	new_test_ext().execute_with(|| {
		// meta tx signer
		let alice_keyring = AccountKeyring::Alice;
		// meta tx relayer
		let bob_keyring = AccountKeyring::Bob;

		let alice_account: AccountId = alice_keyring.public().into();
		let bob_account: AccountId = bob_keyring.public().into();

		let tx_fee: Balance = (2 * TX_FEE).into(); // base tx fee + weight fee
		let alice_balance = force_set_balance(alice_account.clone());
		let bob_balance = force_set_balance(bob_account.clone());

		// Alice builds a meta transaction.

		let remark_call =
			RuntimeCall::System(frame_system::Call::remark_with_event { remark: vec![1] });
		let meta_tx_ext = create_meta_tx_extension(alice_account.clone());
		let meta_tx_sig = create_signature(remark_call.clone(), meta_tx_ext.clone(), alice_keyring);

		let meta_tx = MetaTxFor::<Runtime>::new(
			alice_account.clone(),
			meta_tx_sig,
			remark_call.clone(),
			meta_tx_ext.clone(),
		);

		// Encode and share with the world.
		let meta_tx_encoded = meta_tx.encode();

		// Bob acts as meta transaction relayer.

		let meta_tx = MetaTxFor::<Runtime>::decode(&mut &meta_tx_encoded[..]).unwrap();
		let call = RuntimeCall::MetaTx(Call::dispatch { meta_tx: Box::new(meta_tx.clone()) });
		let tx_ext = create_tx_extension(bob_account.clone());
		let tx_sig = create_signature(call.clone(), tx_ext.clone(), bob_keyring);

		let uxt = UncheckedExtrinsic::new_signed(
			call.clone(),
			bob_account.clone(),
			tx_sig,
			tx_ext.clone(),
		);

		// Check Extrinsic validity and apply it.
		let result = apply_extrinsic(uxt);

		// Asserting the results and make sure the weight is correct.

		// TODO: + dispatch weight
		let tx_weight = tx_ext.weight(&call) + Weight::from_all(1);
		let meta_tx_weight = remark_call
			.get_dispatch_info()
			.call_weight
			.add(meta_tx_ext.weight(&remark_call));

		assert_eq!(
			result,
			Ok(PostDispatchInfo {
				actual_weight: Some(meta_tx_weight + tx_weight),
				pays_fee: Pays::Yes,
			})
		);

		System::assert_has_event(RuntimeEvent::MetaTx(crate::Event::Dispatched {
			result: Ok(PostDispatchInfo {
				actual_weight: Some(meta_tx_weight),
				pays_fee: Pays::Yes,
			}),
		}));

		System::assert_has_event(RuntimeEvent::System(frame_system::Event::Remarked {
			sender: alice_account.clone(),
			hash: <Runtime as frame_system::Config>::Hashing::hash(&[1]),
		}));

		// Alice balance is unchanged, Bob paid the transaction fee.
		assert_eq!(alice_balance, Balances::free_balance(alice_account));
		assert_eq!(bob_balance - tx_fee, Balances::free_balance(bob_account));
	});
}

#[test]
fn invalid_signature() {
	new_test_ext().execute_with(|| {
		// meta tx signer
		let alice_keyring = AccountKeyring::Alice;
		// meta tx relayer
		let bob_keyring = AccountKeyring::Bob;

		let alice_account: AccountId = alice_keyring.public().into();
		let bob_account: AccountId = bob_keyring.public().into();

		let tx_fee: Balance = (2 * TX_FEE).into(); // base tx fee + weight fee
		let alice_balance = force_set_balance(alice_account.clone());
		let bob_balance = force_set_balance(bob_account.clone());

		// Alice builds a meta transaction.

		let remark_call =
			RuntimeCall::System(frame_system::Call::remark_with_event { remark: vec![1] });
		let meta_tx_ext = create_meta_tx_extension(alice_account.clone());
		// signature is invalid since it's signed by charlie instead of alice.
		let invalid_meta_tx_sig =
			create_signature(remark_call.clone(), meta_tx_ext.clone(), AccountKeyring::Charlie);

		let meta_tx = MetaTxFor::<Runtime>::new(
			alice_account.clone(),
			invalid_meta_tx_sig,
			remark_call.clone(),
			meta_tx_ext.clone(),
		);

		// Encode and share with the world.
		let meta_tx_encoded = meta_tx.encode();

		// Bob acts as meta transaction relayer.

		let meta_tx = MetaTxFor::<Runtime>::decode(&mut &meta_tx_encoded[..]).unwrap();
		let call = RuntimeCall::MetaTx(Call::dispatch { meta_tx: Box::new(meta_tx.clone()) });
		let tx_ext = create_tx_extension(bob_account.clone());
		let tx_sig = create_signature(call.clone(), tx_ext.clone(), bob_keyring);

		let uxt = UncheckedExtrinsic::new_signed(call, bob_account.clone(), tx_sig, tx_ext);

		// Check Extrinsic validity and apply it.
		let result = apply_extrinsic(uxt);

		// Asserting the results.

		assert_eq!(result.unwrap_err().error, Error::<Runtime>::BadProof.into());

		// Alice balance is unchanged, Bob paid the transaction fee.
		assert_eq!(alice_balance, Balances::free_balance(alice_account));
		assert_eq!(bob_balance - tx_fee, Balances::free_balance(bob_account));
	});
}

#[test]
fn meta_tx_extension_work() {
	new_test_ext().execute_with(|| {
		// meta tx signer
		let alice_keyring = AccountKeyring::Alice;
		// meta tx relayer
		let bob_keyring = AccountKeyring::Bob;

		let alice_account: AccountId = alice_keyring.public().into();
		let bob_account: AccountId = bob_keyring.public().into();

		let tx_fee: Balance = (2 * TX_FEE).into(); // base tx fee + weight fee
		let alice_balance = force_set_balance(alice_account.clone());
		let bob_balance = force_set_balance(bob_account.clone());

		// Alice builds a meta transaction.

		let remark_call =
			RuntimeCall::System(frame_system::Call::remark_with_event { remark: vec![1] });

		let meta_tx_ext = create_meta_tx_extension(alice_account.clone());
		let meta_tx_sig = create_signature(remark_call.clone(), meta_tx_ext.clone(), alice_keyring);

		let meta_tx = MetaTxFor::<Runtime>::new(
			alice_account.clone(),
			meta_tx_sig,
			remark_call.clone(),
			meta_tx_ext.clone(),
		);

		// Encode and share with the world.
		let meta_tx_encoded = meta_tx.encode();

		// Bob acts as meta transaction relayer.

		let meta_tx = MetaTxFor::<Runtime>::decode(&mut &meta_tx_encoded[..]).unwrap();
		let call = RuntimeCall::MetaTx(Call::dispatch { meta_tx: Box::new(meta_tx.clone()) });
		let tx_ext = create_tx_extension(bob_account.clone());
		let tx_sig = create_signature(call.clone(), tx_ext.clone(), bob_keyring);

		let uxt = UncheckedExtrinsic::new_signed(call.clone(), bob_account.clone(), tx_sig, tx_ext);

		// increment alice's nonce to invalidate the meta tx and verify that the
		// meta tx extension works.
		frame_system::Pallet::<Runtime>::inc_account_nonce(alice_account.clone());

		// Check Extrinsic validity and apply it.
		let result = apply_extrinsic(uxt);

		// Asserting the results.
		assert_eq!(result.unwrap_err().error, Error::<Runtime>::Stale.into());

		// Alice balance is unchanged, Bob paid the transaction fee.
		assert_eq!(alice_balance, Balances::free_balance(alice_account));
		assert_eq!(bob_balance - tx_fee, Balances::free_balance(bob_account));
	});
}

#[test]
fn meta_tx_call_fails() {
	new_test_ext().execute_with(|| {
		// meta tx signer
		let alice_keyring = AccountKeyring::Alice;
		// meta tx relayer
		let bob_keyring = AccountKeyring::Bob;

		let alice_account: AccountId = alice_keyring.public().into();
		let bob_account: AccountId = bob_keyring.public().into();

		let tx_fee: Balance = (2 * TX_FEE).into(); // base tx fee + weight fee
		let alice_balance = force_set_balance(alice_account.clone());
		let bob_balance = force_set_balance(bob_account.clone());

		// Alice builds a meta transaction.

		// transfer more than alice has
		let transfer_call = RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
			dest: bob_account.clone(),
			value: alice_balance * 2,
		});

		let meta_tx_ext = create_meta_tx_extension(alice_account.clone());
		let meta_tx_sig =
			create_signature(transfer_call.clone(), meta_tx_ext.clone(), alice_keyring);

		let meta_tx = MetaTxFor::<Runtime>::new(
			alice_account.clone(),
			meta_tx_sig,
			transfer_call.clone(),
			meta_tx_ext.clone(),
		);

		// Encode and share with the world.
		let meta_tx_encoded = meta_tx.encode();

		// Bob acts as meta transaction relayer.

		let meta_tx = MetaTxFor::<Runtime>::decode(&mut &meta_tx_encoded[..]).unwrap();
		let call = RuntimeCall::MetaTx(Call::dispatch { meta_tx: Box::new(meta_tx.clone()) });
		let tx_ext = create_tx_extension(bob_account.clone());
		let tx_sig = create_signature(call.clone(), tx_ext.clone(), bob_keyring);

		let uxt = UncheckedExtrinsic::new_signed(
			call.clone(),
			bob_account.clone(),
			tx_sig,
			tx_ext.clone(),
		);

		// Check Extrinsic validity and apply it.
		let result = apply_extrinsic(uxt);

		// Asserting the results and make sure the weight is correct.

		// TODO: + dispatch weight
		let tx_weight = tx_ext.weight(&call) + Weight::from_all(1);
		let meta_tx_weight = transfer_call
			.get_dispatch_info()
			.call_weight
			.add(meta_tx_ext.weight(&transfer_call));

		assert_eq!(
			result,
			Ok(PostDispatchInfo {
				actual_weight: Some(meta_tx_weight + tx_weight),
				pays_fee: Pays::Yes,
			})
		);

		System::assert_has_event(RuntimeEvent::MetaTx(crate::Event::Dispatched {
			result: Err(DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(meta_tx_weight),
					pays_fee: Pays::Yes,
				},
				error: sp_runtime::DispatchError::Token(sp_runtime::TokenError::FundsUnavailable),
			}),
		}));

		// Alice balance is unchanged, Bob paid the transaction fee.
		assert_eq!(alice_balance, Balances::free_balance(alice_account));
		assert_eq!(bob_balance - tx_fee, Balances::free_balance(bob_account));
	});
}
