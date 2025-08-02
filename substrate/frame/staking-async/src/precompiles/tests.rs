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

use crate::precompiles::staking::IStaking;
use alloy_core::{self as alloy, sol_types::SolCall};

#[test]
fn test_interface_encoding() {
	// Test that the ABI encoding/decoding works correctly
	let bond_call = IStaking::bondCall {
		value: alloy::primitives::U256::from(1000),
		payee: 0,
	};

	// Encode and then decode
	let encoded = bond_call.abi_encode();
	let decoded = IStaking::bondCall::abi_decode(&encoded).unwrap();

	assert_eq!(decoded.value, alloy::primitives::U256::from(1000));
	assert_eq!(decoded.payee, 0);
}

#[test]
fn test_nominate_encoding() {
	let validator1 = alloy::primitives::Address::from([1u8; 20]);
	let validator2 = alloy::primitives::Address::from([2u8; 20]);

	let nominate_call = IStaking::nominateCall {
		targets: vec![validator1, validator2],
	};

	let encoded = nominate_call.abi_encode();
	let decoded = IStaking::nominateCall::abi_decode(&encoded).unwrap();

	assert_eq!(decoded.targets.len(), 2);
	assert_eq!(decoded.targets[0], validator1);
	assert_eq!(decoded.targets[1], validator2);
}

#[test]
fn test_validate_encoding() {
	let validate_call = IStaking::validateCall {
		commission: alloy::primitives::U256::from(50_000_000), // 5%
		blocked: false,
	};

	let encoded = validate_call.abi_encode();
	let decoded = IStaking::validateCall::abi_decode(&encoded).unwrap();

	assert_eq!(decoded.commission, alloy::primitives::U256::from(50_000_000));
	assert_eq!(decoded.blocked, false);
}

#[test]
fn test_query_encoding() {
	// Test ledger query
	let stash = alloy::primitives::Address::from([1u8; 20]);
	let ledger_call = IStaking::ledgerCall { stash };

	let encoded = ledger_call.abi_encode();
	let decoded = IStaking::ledgerCall::abi_decode(&encoded).unwrap();

	assert_eq!(decoded.stash, stash);

	// Test currentEra query
	let era_call = IStaking::currentEraCall {};
	let _encoded = era_call.abi_encode();

	// Test minNominatorBond query
	let min_bond_call = IStaking::minNominatorBondCall {};
	let _encoded = min_bond_call.abi_encode();
}

#[test]
fn test_unbond_encoding() {
	let unbond_call = IStaking::unbondCall {
		value: alloy::primitives::U256::from(500),
	};

	let encoded = unbond_call.abi_encode();
	let decoded = IStaking::unbondCall::abi_decode(&encoded).unwrap();

	assert_eq!(decoded.value, alloy::primitives::U256::from(500));
}

#[test]
fn test_withdraw_unbonded_encoding() {
	let withdraw_call = IStaking::withdrawUnbondedCall {
		numSlashingSpans: 10,
	};

	let encoded = withdraw_call.abi_encode();
	let decoded = IStaking::withdrawUnbondedCall::abi_decode(&encoded).unwrap();

	assert_eq!(decoded.numSlashingSpans, 10);
}

#[test]
fn test_rebond_encoding() {
	let rebond_call = IStaking::rebondCall {
		value: alloy::primitives::U256::from(250),
	};

	let encoded = rebond_call.abi_encode();
	let decoded = IStaking::rebondCall::abi_decode(&encoded).unwrap();

	assert_eq!(decoded.value, alloy::primitives::U256::from(250));
}

#[test]
fn test_payout_stakers_encoding() {
	let validator = alloy::primitives::Address::from([3u8; 20]);
	let payout_call = IStaking::payoutStakersCall {
		validatorStash: validator,
		era: alloy::primitives::U256::from(100),
	};

	let encoded = payout_call.abi_encode();
	let decoded = IStaking::payoutStakersCall::abi_decode(&encoded).unwrap();

	assert_eq!(decoded.validatorStash, validator);
	assert_eq!(decoded.era, alloy::primitives::U256::from(100));
}

#[test]
fn test_validators_query_encoding() {
	let validator = alloy::primitives::Address::from([4u8; 20]);
	let validators_call = IStaking::validatorsCall { validator };

	let encoded = validators_call.abi_encode();
	let decoded = IStaking::validatorsCall::abi_decode(&encoded).unwrap();

	assert_eq!(decoded.validator, validator);
}

#[test]
fn test_nominators_query_encoding() {
	let nominator = alloy::primitives::Address::from([5u8; 20]);
	let nominators_call = IStaking::nominatorsCall { nominator };

	let encoded = nominators_call.abi_encode();
	let decoded = IStaking::nominatorsCall::abi_decode(&encoded).unwrap();

	assert_eq!(decoded.nominator, nominator);
}
