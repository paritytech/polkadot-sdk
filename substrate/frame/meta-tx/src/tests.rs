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
use frame_support::traits::tokens::fungible::{Inspect, InspectHold};
use keyring::AccountKeyring;
use mock::*;
use sp_io::hashing::blake2_256;
use sp_runtime::{
	traits::{Applyable, Checkable, Hash, IdentityLookup},
	MultiSignature,
};

#[docify::export]
#[test]
fn sign_and_execute_meta_tx() {
	new_test_ext().execute_with(|| {
		// meta tx signer
		let alice_keyring = AccountKeyring::Alice;
		// meta tx relayer
		let bob_keyring = AccountKeyring::Bob;

		let alice_account = AccountId::from(alice_keyring.public());
		let bob_account = AccountId::from(bob_keyring.public());

		let ed = Balances::minimum_balance();
		let tx_fee: Balance = (2 * TX_FEE).into(); // base tx fee + weight fee
		let alice_balance = ed * 100;
		let bob_balance = ed * 100;

		{
			// setup initial balances for alice and bob
			Balances::force_set_balance(
				RuntimeOrigin::root(),
				alice_account.clone().into(),
				alice_balance,
			)
			.unwrap();
			Balances::force_set_balance(
				RuntimeOrigin::root(),
				bob_account.clone().into(),
				bob_balance,
			)
			.unwrap();
		}

		// Alice builds a meta transaction.

		let remark_call =
			RuntimeCall::System(frame_system::Call::remark_with_event { remark: vec![1] });
		let meta_tx_ext: MetaTxExtension = (
			frame_system::CheckNonZeroSender::<Runtime>::new(),
			frame_system::CheckSpecVersion::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckMortality::<Runtime>::from(sp_runtime::generic::Era::immortal()),
			frame_system::CheckNonce::<Runtime>::from(
				frame_system::Pallet::<Runtime>::account(&alice_account).nonce,
			),
		);

		let meta_tx_sig = MultiSignature::Sr25519(
			(remark_call.clone(), meta_tx_ext.clone(), meta_tx_ext.implicit().unwrap())
				.using_encoded(|e| alice_keyring.sign(&blake2_256(e))),
		);

		let meta_tx = MetaTxFor::<Runtime>::new_signed(
			alice_account.clone(),
			meta_tx_sig,
			remark_call.clone(),
			meta_tx_ext.clone(),
		);

		// Encode and share with the world.
		let meta_tx_encoded = meta_tx.encode();

		// Bob acts as meta transaction relayer.

		let meta_tx = MetaTxFor::<Runtime>::decode(&mut &meta_tx_encoded[..]).unwrap();
		let call = RuntimeCall::MetaTx(Call::dispatch { meta_tx: meta_tx.clone() });
		let tx_ext: Extension = (
			frame_system::CheckNonZeroSender::<Runtime>::new(),
			frame_system::CheckSpecVersion::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckMortality::<Runtime>::from(sp_runtime::generic::Era::immortal()),
			frame_system::CheckNonce::<Runtime>::from(
				frame_system::Pallet::<Runtime>::account(&bob_account).nonce,
			),
			frame_system::CheckWeight::<Runtime>::new(),
			pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0),
		);

		let tx_sig = MultiSignature::Sr25519(
			(call.clone(), tx_ext.clone(), tx_ext.implicit().unwrap())
				.using_encoded(|e| bob_keyring.sign(&blake2_256(e))),
		);

		let uxt = UncheckedExtrinsic::new_signed(call, bob_account.clone(), tx_sig, tx_ext);

		// Check Extrinsic validity and apply it.

		let uxt_info = uxt.get_dispatch_info();
		let uxt_len = uxt.using_encoded(|e| e.len());

		let xt = <UncheckedExtrinsic as Checkable<IdentityLookup<AccountId>>>::check(
			uxt,
			&Default::default(),
		)
		.unwrap();

		let res = xt.apply::<Runtime>(&uxt_info, uxt_len).unwrap();

		// Asserting the results.

		assert!(res.is_ok());

		System::assert_has_event(RuntimeEvent::MetaTx(crate::Event::Dispatched { result: res }));

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
fn nonexistent_account_meta_tx() {
	new_test_ext().execute_with(|| {
		// meta tx signer
		let alice_keyring = AccountKeyring::Alice;
		// meta tx relayer
		let bob_keyring = AccountKeyring::Bob;

		let alice_account = AccountId::from(alice_keyring.public());
		let bob_account = AccountId::from(bob_keyring.public());

		let ed = Balances::minimum_balance();
		let tx_fee: Balance = (2 * TX_FEE).into(); // base tx fee + weight fee
		let bob_balance = ed * 100;

		{
			// setup initial balance only for bob
			Balances::force_set_balance(
				RuntimeOrigin::root(),
				bob_account.clone().into(),
				bob_balance,
			)
			.unwrap();
		}

		// Alice builds a meta transaction.

		let remark_call =
			RuntimeCall::System(frame_system::Call::remark_with_event { remark: vec![1] });
		let meta_tx_ext: MetaTxExtension = (
			frame_system::CheckNonZeroSender::<Runtime>::new(),
			frame_system::CheckSpecVersion::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckMortality::<Runtime>::from(sp_runtime::generic::Era::immortal()),
			frame_system::CheckNonce::<Runtime>::from(
				frame_system::Pallet::<Runtime>::account(&alice_account).nonce,
			),
		);

		let meta_tx_sig = MultiSignature::Sr25519(
			(remark_call.clone(), meta_tx_ext.clone(), meta_tx_ext.implicit().unwrap())
				.using_encoded(|e| alice_keyring.sign(&blake2_256(e))),
		);

		let meta_tx = MetaTxFor::<Runtime>::new_signed(
			alice_account.clone(),
			meta_tx_sig,
			remark_call.clone(),
			meta_tx_ext.clone(),
		);

		// Encode and share with the world.
		let meta_tx_encoded = meta_tx.encode();

		// Bob acts as meta transaction relayer and as the sponsor for Alice's account existence.

		let meta_tx = MetaTxFor::<Runtime>::decode(&mut &meta_tx_encoded[..]).unwrap();
		// Use meta dispatch which also creates Alice's account.
		let call = RuntimeCall::MetaTx(Call::dispatch_creating { meta_tx: meta_tx.clone() });
		let tx_ext: Extension = (
			frame_system::CheckNonZeroSender::<Runtime>::new(),
			frame_system::CheckSpecVersion::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckMortality::<Runtime>::from(sp_runtime::generic::Era::immortal()),
			frame_system::CheckNonce::<Runtime>::from(
				frame_system::Pallet::<Runtime>::account(&bob_account).nonce,
			),
			frame_system::CheckWeight::<Runtime>::new(),
			pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0),
		);

		let tx_sig = MultiSignature::Sr25519(
			(call.clone(), tx_ext.clone(), tx_ext.implicit().unwrap())
				.using_encoded(|e| bob_keyring.sign(&blake2_256(e))),
		);

		let uxt = UncheckedExtrinsic::new_signed(call, bob_account.clone(), tx_sig, tx_ext);

		// Alice's account doesn't exist yet.
		assert!(!System::account_exists(&alice_account));

		// Check Extrinsic validity and apply it.

		let uxt_info = uxt.get_dispatch_info();
		let uxt_len = uxt.using_encoded(|e| e.len());

		let xt = <UncheckedExtrinsic as Checkable<IdentityLookup<AccountId>>>::check(
			uxt,
			&Default::default(),
		)
		.unwrap();

		let res = xt.apply::<Runtime>(&uxt_info, uxt_len).unwrap();

		// Asserting the results.

		assert!(res.is_ok());

		System::assert_has_event(RuntimeEvent::MetaTx(crate::Event::Dispatched { result: res }));

		System::assert_has_event(RuntimeEvent::System(frame_system::Event::Remarked {
			sender: alice_account.clone(),
			hash: <Runtime as frame_system::Config>::Hashing::hash(&[1]),
		}));

		// Alice's account has been created and ran the transaction, and Bob paid the transaction
		// fee.
		assert!(System::account_exists(&alice_account));
		// Nonce is stored and updated.
		assert_eq!(System::account_nonce(&alice_account), 1);
		assert_eq!(0, Balances::free_balance(&alice_account));
		let provider_deposit =
			<<Runtime as pallet_account_sponsorship::Config>::BaseDeposit as sp_core::Get<u64>>::get() +
				<<Runtime as pallet_account_sponsorship::Config>::BeneficiaryDeposit as sp_core::Get<u64>>::get() +
				pallet_account_sponsorship::AccountDeposit::<Runtime>::get();
		assert_eq!(bob_balance - tx_fee - provider_deposit, Balances::free_balance(&bob_account));
		assert_eq!(provider_deposit, Balances::total_balance_on_hold(&bob_account));

		// Alice has just been sponsored, the sponsorship cannot be withdrawn yet.
		assert_eq!(
			AccountSponsorship::withdraw_sponsorship(
				Some(bob_account.clone()).into(),
				alice_account.clone()
			)
			.unwrap_err(),
			pallet_account_sponsorship::Error::<Runtime>::EarlyWithdrawal.into()
		);

		// Let the grace period pass.
		let grace_period =
			<<Runtime as pallet_account_sponsorship::Config>::GracePeriod as sp_core::Get<u64>>::get();
		System::set_block_number(System::block_number() + grace_period);

		// Bob can now withdraw his sponsorship and release the deposit.
		frame_support::assert_ok!(AccountSponsorship::withdraw_sponsorship(
			Some(bob_account.clone()).into(),
			alice_account.clone()
		));
		assert!(!System::account_exists(&alice_account));
		assert_eq!(Balances::total_balance_on_hold(&bob_account), 0);
	});
}
