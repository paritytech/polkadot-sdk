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

//! Tests for the `eth_estimate_gas` short-circuit fast path.

use crate::{
	EthTransactError, Pallet, SIMPLE_TRANSFER_GAS,
	address::AddressMapper,
	evm::{
		AccessListEntry, AuthorizationListEntry, DryRunConfig, GenericTransaction, StateOverride,
		StateOverrideSet,
	},
	test_utils::{ALICE_ADDR, BOB, BOB_ADDR, CHARLIE_ADDR},
	tests::{Config, ExtBuilder, Test, test_utils::place_contract},
};
use frame_support::traits::fungible::Mutate;
use sp_core::{H256, U256};

fn simple_transfer_tx() -> GenericTransaction {
	GenericTransaction {
		from: Some(ALICE_ADDR),
		to: Some(CHARLIE_ADDR),
		value: Some(U256::from(1_000_000)),
		..Default::default()
	}
}

#[test]
fn is_simple_transfer_eoa_to_eoa_with_empty_input() {
	ExtBuilder::default().build().execute_with(|| {
		assert!(Pallet::<Test>::is_simple_transfer(
			&simple_transfer_tx(),
			&DryRunConfig::default(),
		));
	});
}

#[test]
fn is_simple_transfer_rejects_contract_creation() {
	ExtBuilder::default().build().execute_with(|| {
		let tx = GenericTransaction { to: None, ..simple_transfer_tx() };
		assert!(!Pallet::<Test>::is_simple_transfer(&tx, &DryRunConfig::default()));
	});
}

#[test]
fn is_simple_transfer_rejects_non_empty_input() {
	ExtBuilder::default().build().execute_with(|| {
		let tx = GenericTransaction { input: vec![0x01].into(), ..simple_transfer_tx() };
		assert!(!Pallet::<Test>::is_simple_transfer(&tx, &DryRunConfig::default()));
	});
}

#[test]
fn is_simple_transfer_rejects_access_list() {
	ExtBuilder::default().build().execute_with(|| {
		let tx = GenericTransaction {
			access_list: Some(vec![AccessListEntry {
				address: BOB_ADDR,
				storage_keys: vec![H256::zero()],
			}]),
			..simple_transfer_tx()
		};
		assert!(!Pallet::<Test>::is_simple_transfer(&tx, &DryRunConfig::default()));
	});
}

#[test]
fn is_simple_transfer_accepts_empty_access_list() {
	ExtBuilder::default().build().execute_with(|| {
		let tx = GenericTransaction { access_list: Some(vec![]), ..simple_transfer_tx() };
		assert!(Pallet::<Test>::is_simple_transfer(&tx, &DryRunConfig::default()));
	});
}

#[test]
fn is_simple_transfer_rejects_authorization_list() {
	ExtBuilder::default().build().execute_with(|| {
		let tx = GenericTransaction {
			authorization_list: vec![AuthorizationListEntry {
				chain_id: U256::zero(),
				address: BOB_ADDR,
				nonce: U256::zero(),
				y_parity: U256::zero(),
				r: U256::zero(),
				s: U256::zero(),
			}],
			..simple_transfer_tx()
		};
		assert!(!Pallet::<Test>::is_simple_transfer(&tx, &DryRunConfig::default()));
	});
}

#[test]
fn is_simple_transfer_rejects_blob_payload() {
	ExtBuilder::default().build().execute_with(|| {
		let tx = GenericTransaction {
			blob_versioned_hashes: vec![H256::zero()],
			..simple_transfer_tx()
		};
		assert!(!Pallet::<Test>::is_simple_transfer(&tx, &DryRunConfig::default()));
	});
}

#[test]
fn is_simple_transfer_rejects_non_empty_state_overrides() {
	ExtBuilder::default().build().execute_with(|| {
		let mut overrides = StateOverrideSet::default();
		overrides.0.insert(CHARLIE_ADDR, StateOverride::default());
		let config = DryRunConfig::default().with_state_overrides(overrides);
		assert!(!Pallet::<Test>::is_simple_transfer(&simple_transfer_tx(), &config));
	});
}

#[test]
fn is_simple_transfer_accepts_empty_state_overrides() {
	ExtBuilder::default().build().execute_with(|| {
		let config = DryRunConfig::default().with_state_overrides(StateOverrideSet::default());
		assert!(Pallet::<Test>::is_simple_transfer(&simple_transfer_tx(), &config));
	});
}

#[test]
fn is_simple_transfer_rejects_contract_destination() {
	ExtBuilder::default().build().execute_with(|| {
		// Place a contract at BOB so BOB_ADDR is now a contract account.
		place_contract(&BOB, H256::repeat_byte(0xab));

		let tx = GenericTransaction { to: Some(BOB_ADDR), ..simple_transfer_tx() };
		assert!(!Pallet::<Test>::is_simple_transfer(&tx, &DryRunConfig::default()));
	});
}

#[test]
fn eth_estimate_gas_short_circuits_simple_transfer() {
	ExtBuilder::default().build().execute_with(|| {
		let alice = <Test as Config>::AddressMapper::to_account_id(&ALICE_ADDR);
		let _ = <Test as Config>::Currency::set_balance(&alice, u64::MAX as u128);

		let estimate =
			Pallet::<Test>::eth_estimate_gas(simple_transfer_tx(), DryRunConfig::default())
				.expect("simple transfer should be estimable");
		assert_eq!(estimate, U256::from(SIMPLE_TRANSFER_GAS));
	});
}

#[test]
fn eth_estimate_gas_short_circuit_errors_when_value_exceeds_balance() {
	ExtBuilder::default().build().execute_with(|| {
		let err = Pallet::<Test>::eth_estimate_gas(simple_transfer_tx(), DryRunConfig::default())
			.expect_err("transfer with empty balance must error");
		match err {
			EthTransactError::Message(msg) => {
				assert!(msg.contains("insufficient funds"), "unexpected error message: {msg}")
			},
			other => panic!("expected EthTransactError::Message, got {other:?}"),
		}
	});
}
