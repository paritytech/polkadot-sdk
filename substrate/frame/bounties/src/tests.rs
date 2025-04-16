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

//! bounties pallet tests.

#![cfg(test)]

use super::{Event as BountiesEvent, *};
use crate as pallet_bounties;
use crate::mock::{Bounties, *};

use frame_support::{assert_err_ignore_postinfo, assert_noop, assert_ok, traits::Currency};
use sp_runtime::traits::Dispatchable;

type UtilityCall = pallet_utility::Call<Test>;
type BountiesCall = crate::Call<Test>;

#[test]
fn propose_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		// When
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(0),
			Box::new(1),
			10,
			b"1234567890".to_vec()
		));

		// Then
		assert_eq!(last_event(), BountiesEvent::BountyProposed { index: 0 });
		let deposit: u64 = 85 + 5;
		assert_eq!(Balances::reserved_balance(0), deposit);
		assert_eq!(Balances::free_balance(0), 100 - deposit);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(0).unwrap(),
			Bounty {
				proposer: 0,
				fee: 0,
				curator_deposit: 0,
				asset_kind: 1,
				value: 10,
				bond: deposit,
				status: BountyStatus::Proposed,
			}
		);
		assert_eq!(
			pallet_bounties::BountyDescriptions::<Test>::get(0).unwrap(),
			b"1234567890".to_vec()
		);
		assert_eq!(pallet_bounties::BountyCount::<Test>::get(), 1);
	});
}

#[test]
fn propose_bounty_validation_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		// When/Then
		assert_noop!(
			Bounties::propose_bounty(
				RuntimeOrigin::signed(1),
				Box::new(1),
				0,
				[0; 17_000].to_vec()
			),
			Error::<Test>::ReasonTooBig
		);

		// When/Then
		assert_noop!(
			Bounties::propose_bounty(
				RuntimeOrigin::signed(1),
				Box::new(1),
				10,
				b"12345678901234567890".to_vec()
			),
			Error::<Test>::InsufficientProposersBalance
		);

		// When/Then
		assert_noop!(
			Bounties::propose_bounty(
				RuntimeOrigin::signed(1),
				Box::new(1),
				0,
				b"12345678901234567890".to_vec()
			),
			Error::<Test>::InvalidValue
		);
	});
}

#[test]
fn close_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		// When/Then
		assert_noop!(Bounties::close_bounty(RuntimeOrigin::root(), 0), Error::<Test>::InvalidIndex);

		// Given
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(0),
			Box::new(1),
			10,
			b"12345".to_vec()
		));

		// When
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), 0));

		// Then
		let deposit: u64 = 80 + 5;
		assert_eq!(last_event(), BountiesEvent::BountyRejected { index: 0, bond: deposit });
		assert_eq!(Balances::reserved_balance(0), 0);
		assert_eq!(Balances::free_balance(0), 100 - deposit);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(0), None);
		assert!(!pallet_treasury::Proposals::<Test>::contains_key(0));
		assert_eq!(pallet_bounties::BountyDescriptions::<Test>::get(0), None);
	});
}

#[test]
fn approve_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let bounty_id = 0;

		// When/Then
		assert_noop!(
			Bounties::approve_bounty(RuntimeOrigin::root(), 0),
			Error::<Test>::InvalidIndex
		);

		// Given
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));

		// When
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));

		// Then (deposit not returned -> PaymentState::Attempted)
		let deposit: u64 = 80 + 5;
		assert_eq!(last_event(), BountiesEvent::BountyApproved { index: bounty_id });
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(0).unwrap(),
			Bounty {
				proposer,
				fee: 0,
				asset_kind,
				value,
				curator_deposit: 0,
				bond: deposit,
				status: BountyStatus::Approved {
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), 0),
			Error::<Test>::UnexpectedStatus
		);
		assert_eq!(Balances::reserved_balance(proposer), deposit);
		assert_eq!(Balances::free_balance(proposer), 100 - deposit);

		// When (PaymentState::Success)
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);

		// Then (deposit returned)
		assert_eq!(Balances::reserved_balance(proposer), 0);
		assert_eq!(Balances::free_balance(proposer), 100);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee: 0,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: deposit,
				status: BountyStatus::Funded,
			}
		);
	});
}

#[test]
fn approve_bounty_in_batch_respects_max_total() {
	ExtBuilder::default().build().execute_with(|| {
		// Given
		let proposer = 0;
		let spend_origin = 10; // `max_total` of 5
		let asset_kind = 1;
		let value = 2; // `native_amount` of 2
		BountyDepositBase::set(0); // bond not relevant
		for _ in 0..2 {
			assert_ok!(Bounties::propose_bounty(
				RuntimeOrigin::signed(proposer),
				Box::new(asset_kind),
				value,
				b"12345".to_vec()
			));
		}

		// When/Then
		// Respect the `max_total` for the given origin.
		assert_ok!(RuntimeCall::from(UtilityCall::batch_all {
			calls: vec![
				RuntimeCall::from(BountiesCall::approve_bounty { bounty_id: 0 }),
				RuntimeCall::from(BountiesCall::approve_bounty { bounty_id: 1 })
			]
		})
		.dispatch(RuntimeOrigin::signed(spend_origin)));

		// Given
		let value = 3; // `native_amount` of 3
		for _ in 0..2 {
			assert_ok!(Bounties::propose_bounty(
				RuntimeOrigin::signed(proposer),
				Box::new(asset_kind),
				value,
				b"12345".to_vec()
			));
		}

		// When/Then
		// `spend`` of 6 surpasses `max_total` for the given origin.
		assert_err_ignore_postinfo!(
			RuntimeCall::from(UtilityCall::batch_all {
				calls: vec![
					RuntimeCall::from(BountiesCall::approve_bounty { bounty_id: 2 }),
					RuntimeCall::from(BountiesCall::approve_bounty { bounty_id: 3 })
				]
			})
			.dispatch(RuntimeOrigin::signed(spend_origin)),
			Error::<Test>::InsufficientPermission
		);
	})
}

#[test]
fn approve_bounty_with_curator_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let spend_origin = 1;
		let beneficiary = 5;
		let fee = 10;
		let curator = 4;
		let asset_kind = 1;
		let value = 50;
		let bounty_id = 0;
		let curator_stash = 7;
		System::set_block_number(1);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));

		// When/Then
		assert_noop!(
			Bounties::approve_bounty_with_curator(
				RuntimeOrigin::signed(spend_origin),
				bounty_id,
				curator,
				fee
			),
			BadOrigin
		);

		// When/Then
		SpendLimit::set(1);
		assert_noop!(
			Bounties::approve_bounty_with_curator(RuntimeOrigin::root(), bounty_id, curator, fee),
			Error::<Test>::InsufficientPermission
		);

		// When/Then
		SpendLimit::set(u64::MAX);
		assert_noop!(
			Bounties::approve_bounty_with_curator(RuntimeOrigin::root(), bounty_id, curator, 51),
			Error::<Test>::InvalidFee
		);

		// When
		assert_ok!(Bounties::approve_bounty_with_curator(
			RuntimeOrigin::root(),
			bounty_id,
			curator,
			fee
		));

		// Then
		let payment_id = get_payment_id(0, None).expect("no payment attempt");
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(0).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::ApprovedWithCurator {
					curator,
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		expect_events(vec![
			BountiesEvent::BountyApproved { index: bounty_id },
			BountiesEvent::CuratorProposed { bounty_id, curator },
		]);

		// When/Then
		assert_noop!(
			Bounties::approve_bounty_with_curator(RuntimeOrigin::root(), bounty_id, curator, fee),
			Error::<Test>::UnexpectedStatus
		);

		// When
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);

		// Then
		expect_events(vec![BountiesEvent::BountyBecameActive { index: bounty_id }]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::CuratorProposed { curator },
			}
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(RuntimeOrigin::signed(curator), bounty_id, curator_stash),
			pallet_balances::Error::<Test, _>::InsufficientBalance
		);

		// When
		Balances::make_free_balance_be(&curator, 6);
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			bounty_id,
			curator_stash
		));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 5,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Active { curator, curator_stash, update_due: 21 },
			}
		);

		// When/Then
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(curator), bounty_id, beneficiary));
		assert_eq!(last_event(), BountiesEvent::BountyAwarded { index: bounty_id, beneficiary });

		// When/Then (block_number < unlock_at)
		assert_noop!(
			Bounties::claim_bounty(RuntimeOrigin::signed(curator), bounty_id),
			Error::<Test>::Premature
		);

		// When
		go_to_block(4); // block_number >= unlock_at
		assert_ok!(Bounties::claim_bounty(RuntimeOrigin::signed(curator), bounty_id));
		assert_eq!(
			last_event(),
			BountiesEvent::BountyClaimed { index: bounty_id, beneficiary, curator_stash }
		);
		approve_payment(curator_stash, bounty_id, asset_kind, fee); // curator_stash fee
		approve_payment(beneficiary, bounty_id, asset_kind, value - fee); // beneficiary payout

		// Then (final state)
		assert_eq!(pallet_bounties::Bounties::<Test>::iter().count(), 0);
		assert_eq!(
			last_event(),
			BountiesEvent::BountyPayoutProcessed {
				index: bounty_id,
				asset_kind,
				value: value - fee,
				beneficiary
			}
		);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(bounty_id), None);
	});
}

#[test]
fn approve_bounty_with_curator_in_batch_respects_max_total() {
	ExtBuilder::default().build().execute_with(|| {
		// Given
		let proposer = 0;
		let spend_origin = 10; // `max_total` of 5
		let asset_kind = 1;
		let value = 2; // `native_amount` of 2
		let curator = 4;
		let fee = 0;
		BountyDepositBase::set(0); // bond not relevant
		for _ in 0..2 {
			assert_ok!(Bounties::propose_bounty(
				RuntimeOrigin::signed(proposer),
				Box::new(asset_kind),
				value,
				b"12345".to_vec()
			));
		}

		// When/Then
		// Respect the `max_total` for the given origin.
		assert_ok!(RuntimeCall::from(UtilityCall::batch_all {
			calls: vec![
				RuntimeCall::from(BountiesCall::approve_bounty_with_curator {
					bounty_id: 0,
					curator,
					fee
				}),
				RuntimeCall::from(BountiesCall::approve_bounty_with_curator {
					bounty_id: 1,
					curator,
					fee
				})
			]
		})
		.dispatch(RuntimeOrigin::signed(spend_origin)));

		// Given
		let value = 3; // `native_amount` of 3
		for _ in 0..2 {
			assert_ok!(Bounties::propose_bounty(
				RuntimeOrigin::signed(proposer),
				Box::new(asset_kind),
				value,
				b"12345".to_vec()
			));
		}

		// When/Then
		// `spend`` of 6 surpasses `max_total` for the given origin.
		assert_err_ignore_postinfo!(
			RuntimeCall::from(UtilityCall::batch_all {
				calls: vec![
					RuntimeCall::from(BountiesCall::approve_bounty_with_curator {
						bounty_id: 2,
						curator,
						fee
					}),
					RuntimeCall::from(BountiesCall::approve_bounty_with_curator {
						bounty_id: 3,
						curator,
						fee
					})
				]
			})
			.dispatch(RuntimeOrigin::signed(spend_origin)),
			Error::<Test>::InsufficientPermission
		);
	})
}

#[test]
fn approve_bounty_with_curator_early_unassign_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let fee = 10;
		let bounty_id = 0;
		System::set_block_number(1);

		// When
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty_with_curator(
			RuntimeOrigin::root(),
			bounty_id,
			curator,
			fee
		));
		// unassign curator while bounty is not yet funded
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::root(), bounty_id));

		// Then
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Approved {
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		assert_eq!(last_event(), BountiesEvent::CuratorUnassigned { bounty_id });

		// When (PaymentState::Success)
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);

		// Then
		assert_eq!(last_event(), BountiesEvent::BountyBecameActive { index: bounty_id });
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Funded,
			}
		);

		// When (assign curator again through separate process)
		let new_fee = 15;
		let new_curator = 5;
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::root(),
			bounty_id,
			new_curator,
			new_fee
		));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee: new_fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::CuratorProposed { curator: new_curator },
			}
		);
		assert_eq!(
			last_event(),
			BountiesEvent::CuratorProposed { bounty_id, curator: new_curator }
		);
	});
}

#[test]
fn approve_bounty_with_curator_proposed_unassign_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let fee = 10;
		let curator = 4;
		let bounty_id = 0;
		System::set_block_number(1);

		// When
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty_with_curator(
			RuntimeOrigin::root(),
			bounty_id,
			curator,
			fee
		));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);

		// Then
		assert_eq!(last_event(), BountiesEvent::BountyBecameActive { index: bounty_id });
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::CuratorProposed { curator },
			}
		);

		// When
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::signed(curator), bounty_id));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Funded,
			}
		);
		assert_eq!(last_event(), BountiesEvent::CuratorUnassigned { bounty_id });
	});
}

#[test]
fn assign_curator_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let bounty_id = 0;

		// When/Then
		assert_noop!(
			Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, 4, 4),
			Error::<Test>::InvalidIndex
		);

		// When
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		assert_noop!(
			Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, value),
			Error::<Test>::InvalidFee
		);
		let fee = 4;
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));

		// Then
		assert_eq!(last_event(), BountiesEvent::CuratorProposed { bounty_id, curator });
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::CuratorProposed { curator },
			}
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(RuntimeOrigin::signed(1), bounty_id, proposer),
			Error::<Test>::RequireCurator
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(RuntimeOrigin::signed(curator), bounty_id, proposer),
			pallet_balances::Error::<Test, _>::InsufficientBalance
		);

		// Given
		Balances::make_free_balance_be(&curator, 10);

		// When
		go_to_block(2);
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator), bounty_id, proposer));

		// Then
		let expected_deposit = Bounties::calculate_curator_deposit(&fee, asset_kind).unwrap();
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: expected_deposit,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Active { curator, update_due: 22, curator_stash: proposer },
			}
		);
		assert_eq!(last_event(), BountiesEvent::CuratorAccepted { bounty_id, curator });
		assert_eq!(Balances::free_balance(&curator), 10 - expected_deposit);
		assert_eq!(Balances::reserved_balance(&curator), expected_deposit);
	});
}

#[test]
fn unassign_curator_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let bounty_id = 0;

		// When/Then
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));

		// When/Then
		let fee = 4;
		assert_noop!(
			Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee),
			Error::<Test>::UnexpectedStatus
		);

		// Given
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));

		// When/Then
		assert_noop!(Bounties::unassign_curator(RuntimeOrigin::signed(1), bounty_id), BadOrigin);

		// When
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::signed(curator), bounty_id));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(0).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Funded,
			}
		);

		// Given
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));
		Balances::make_free_balance_be(&curator, 10);
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator), bounty_id, proposer));
		let expected_deposit = Bounties::calculate_curator_deposit(&fee, asset_kind).unwrap();

		// When
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::root(), bounty_id));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Funded,
			}
		);
		assert_eq!(Balances::free_balance(&curator), 10 - expected_deposit);
		assert_eq!(Balances::reserved_balance(&curator), 0); // slashed curator deposit
	});
}

#[test]
fn award_and_claim_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let bounty_id = 0;
		let curator = 4;
		let curator_stash = 5;
		let beneficiary = 3;
		Balances::make_free_balance_be(&curator, 10);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		go_to_block(2);
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		let fee = 4;
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			bounty_id,
			curator_stash
		));
		let expected_deposit = Bounties::calculate_curator_deposit(&fee, asset_kind).unwrap();
		assert_eq!(Balances::free_balance(curator), 10 - expected_deposit);

		// When/Then
		assert_noop!(
			Bounties::award_bounty(RuntimeOrigin::signed(1), bounty_id, beneficiary),
			Error::<Test>::RequireCurator
		);

		// When
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(curator), bounty_id, beneficiary));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: expected_deposit,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::PendingPayout {
					curator,
					beneficiary,
					unlock_at: 5,
					curator_stash
				},
			}
		);

		// When/Then
		assert_noop!(
			Bounties::claim_bounty(RuntimeOrigin::signed(1), bounty_id),
			Error::<Test>::Premature
		);

		// Given
		go_to_block(5);

		// When
		assert_ok!(Bounties::claim_bounty(RuntimeOrigin::signed(1), bounty_id));

		// Then (PaymentState::Attempted)
		assert_eq!(
			last_event(),
			BountiesEvent::BountyClaimed { index: bounty_id, beneficiary, curator_stash }
		);
		let curator_payment_id =
			get_payment_id(bounty_id, Some(curator_stash)).expect("no payment attempt");
		let beneficiary_payment_id =
			get_payment_id(bounty_id, Some(beneficiary)).expect("no payment attempt");
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(0).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: expected_deposit,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::PayoutAttempted {
					curator,
					curator_stash: (
						curator_stash,
						PaymentState::Attempted { id: curator_payment_id }
					),
					beneficiary: (
						beneficiary,
						PaymentState::Attempted { id: beneficiary_payment_id }
					)
				},
			}
		);

		// When (PaymentState::Success)
		approve_payment(curator_stash, bounty_id, asset_kind, fee); // pay curator_stash final_fee
		approve_payment(beneficiary, bounty_id, asset_kind, value - fee); // pay beneficiary payout

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::BountyPayoutProcessed {
				index: bounty_id,
				asset_kind,
				value: value - fee,
				beneficiary
			}
		);
		assert_eq!(Balances::free_balance(curator), 10); // initial 10 (curator)
		assert_eq!(pallet_bounties::Bounties::<Test>::iter().count(), 0);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(bounty_id), None);
		assert_eq!(pallet_bounties::BountyDescriptions::<Test>::get(bounty_id), None);
	});
}

#[test]
fn claim_handles_high_fee() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let bounty_id = 0;
		let curator = 4;
		let curator_stash = 5;
		let beneficiary = 3;
		Balances::make_free_balance_be(&curator, 30);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);

		// When/Then
		let fee = 50;
		assert_noop!(
			Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee),
			Error::<Test>::InvalidFee
		);

		// When
		let fee = 49;
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			bounty_id,
			curator_stash
		));
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(curator), bounty_id, beneficiary));
		go_to_block(5);
		assert_ok!(Bounties::claim_bounty(RuntimeOrigin::signed(1), 0));
		let (final_fee, payout) = Bounties::calculate_curator_fee_and_payout(bounty_id, fee, value);
		approve_payment(curator_stash, bounty_id, asset_kind, final_fee); // pay curator_stash final_fee
		approve_payment(beneficiary, bounty_id, asset_kind, payout); // pay beneficiary payout

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::BountyPayoutProcessed {
				index: bounty_id,
				asset_kind,
				value: payout,
				beneficiary
			}
		);
		assert_eq!(Balances::free_balance(curator), 30);
		assert_eq!(Balances::free_balance(beneficiary), 0);
		assert_eq!(Balances::free_balance(bounty_account), 0);
		assert_eq!(pallet_bounties::Bounties::<Test>::iter().count(), 0);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(bounty_id), None);
		assert_eq!(pallet_bounties::BountyDescriptions::<Test>::get(bounty_id), None);
	});
}

#[test]
fn cancel_and_refund() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let bounty_id = 0;
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		go_to_block(2);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee: 0,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Funded,
			}
		);

		// When/Then
		assert_noop!(Bounties::close_bounty(RuntimeOrigin::signed(0), bounty_id), BadOrigin);

		// When
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), bounty_id));

		// Then (PaymentState::Attempted)
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid { index: bounty_id, payment_id },
			BountiesEvent::BountyCanceled { index: bounty_id },
		]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee: 0,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::RefundAttempted {
					payment_status: PaymentState::Attempted { id: payment_id },
					curator: None,
				},
			}
		);

		// When
		let treasury_account = Bounties::account_id();
		approve_payment(treasury_account, bounty_id, asset_kind, value);

		// Then (PaymentState::Success)
		assert_eq!(pallet_bounties::Bounties::<Test>::iter().count(), 0);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(0), None);
		assert_eq!(pallet_bounties::BountyDescriptions::<Test>::get(0), None);
		assert_eq!(last_event(), BountiesEvent::BountyRefundProcessed { index: bounty_id });
	});
}

#[test]
fn cancel_and_refund_with_curator() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let curator_stash = 5;
		let fee = 10;
		let bounty_id = 0;
		Balances::make_free_balance_be(&curator, 101);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty_with_curator(
			RuntimeOrigin::root(),
			bounty_id,
			curator,
			fee
		));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		go_to_block(2);
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			bounty_id,
			curator_stash
		));
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 5,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Active { curator, curator_stash, update_due: 22 },
			}
		);

		// When
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), bounty_id));

		// Then (PaymentState::Attempted)
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid { index: bounty_id, payment_id },
			BountiesEvent::BountyCanceled { index: bounty_id },
		]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 5,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::RefundAttempted {
					payment_status: PaymentState::Attempted { id: payment_id },
					curator: Some(curator),
				},
			}
		);
		assert_eq!(Balances::free_balance(curator), 101 - 5);
		assert_eq!(Balances::reserved_balance(curator), 5);

		// When
		let treasury_account = Bounties::account_id();
		approve_payment(treasury_account, bounty_id, asset_kind, value);

		// Then (PaymentState::Success)
		assert_eq!(pallet_bounties::Bounties::<Test>::iter().count(), 0);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(0), None);
		assert_eq!(pallet_bounties::BountyDescriptions::<Test>::get(0), None);
		assert_eq!(last_event(), BountiesEvent::BountyRefundProcessed { index: bounty_id });
		assert_eq!(Balances::free_balance(curator), 101);
		assert_eq!(Balances::reserved_balance(curator), 0); // deposit refunded
	});
}

#[test]
fn award_and_cancel() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let beneficiary = 3;
		let fee = 10;
		let bounty_id = 0;
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, proposer, fee));
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(0), bounty_id, proposer));
		assert_eq!(Balances::free_balance(proposer), 95);
		assert_eq!(Balances::reserved_balance(proposer), 5);
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(proposer), bounty_id, beneficiary));

		// When/Then (cannot close bounty directly when payout is happening)
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), bounty_id),
			Error::<Test>::PendingPayout
		);

		// When
		// Instead unassign the curator to slash them and then close.
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::root(), bounty_id));
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), bounty_id));
		assert_eq!(last_event(), BountiesEvent::BountyCanceled { index: bounty_id });
		let treasury_account = Bounties::account_id();
		approve_payment(treasury_account, bounty_id, asset_kind, value);

		// Then
		assert_eq!(last_event(), BountiesEvent::BountyRefundProcessed { index: bounty_id });
		assert_eq!(Balances::free_balance(bounty_account), 0);
		// Slashed.
		assert_eq!(Balances::free_balance(0), 95);
		assert_eq!(Balances::reserved_balance(0), 0);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(bounty_id), None);
		assert_eq!(pallet_bounties::BountyDescriptions::<Test>::get(bounty_id), None);
	});
}

#[test]
fn expire_and_unassign() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let curator = 1;
		let fee = 10;
		let bounty_id = 0;
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(1), bounty_id, proposer));
		assert_eq!(Balances::free_balance(1), 93);
		assert_eq!(Balances::reserved_balance(1), 5);

		// When/Then
		go_to_block(22);
		assert_noop!(
			Bounties::unassign_curator(RuntimeOrigin::signed(proposer), bounty_id),
			Error::<Test>::Premature
		);

		// When
		go_to_block(23);

		// Then
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::signed(proposer), bounty_id));
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(0).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Funded,
			}
		);
		assert_eq!(Balances::free_balance(curator), 93);
		assert_eq!(Balances::reserved_balance(curator), 0); // slashed
	});
}

#[test]
fn extend_expiry() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let fee = 10;
		let bounty_id = 0;
		Balances::make_free_balance_be(&curator, 10);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account = Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);

		// When/Then
		assert_noop!(
			Bounties::extend_bounty_expiry(RuntimeOrigin::signed(1), bounty_id, Vec::new()),
			Error::<Test>::UnexpectedStatus
		);

		// Given
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator), bounty_id, proposer));
		assert_eq!(Balances::free_balance(curator), 5);
		assert_eq!(Balances::reserved_balance(curator), 5);

		// When
		go_to_block(10);
		assert_noop!(
			Bounties::extend_bounty_expiry(RuntimeOrigin::signed(proposer), bounty_id, Vec::new()),
			Error::<Test>::RequireCurator
		);
		assert_ok!(Bounties::extend_bounty_expiry(RuntimeOrigin::signed(curator), bounty_id, Vec::new()));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(0).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 5,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Active { curator, curator_stash: proposer, update_due: 30 },
			}
		);

		// When
		assert_ok!(Bounties::extend_bounty_expiry(RuntimeOrigin::signed(curator), bounty_id, Vec::new()));

		// Then
		assert_eq!(last_event(), BountiesEvent::BountyExtended { index: bounty_id });
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 5,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Active { curator, curator_stash: proposer, update_due: 30 }, /* still the same */
			}
		);

		// When/Then
		go_to_block(25);
		assert_noop!(
			Bounties::unassign_curator(RuntimeOrigin::signed(proposer), bounty_id),
			Error::<Test>::Premature
		);
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::signed(curator), bounty_id));
		assert_eq!(Balances::free_balance(curator), 10); // not slashed
		assert_eq!(Balances::reserved_balance(curator), 0);
	});
}

#[test]
fn unassign_curator_self() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 50;
		let curator = 1;
		let fee = 10;
		let bounty_id = 0;
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, 1, 10));
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(1), bounty_id, 0));
		assert_eq!(Balances::free_balance(curator), 93);
		assert_eq!(Balances::reserved_balance(curator), 5);

		// When
		go_to_block(8);
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::signed(curator), bounty_id));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 0,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::Funded,
			}
		);
		assert_eq!(Balances::free_balance(curator), 98);
		assert_eq!(Balances::reserved_balance(curator), 0); // not slashed
	});
}

#[test]
fn accept_curator_handles_different_deposit_calculations() {
	// This test will verify that a bounty with and without a fee results
	// in a different curator deposit: one using the value, and one using the fee.
	ExtBuilder::default().build_and_execute(|| {
		// Case 1: With a fee
		// Given
		let proposer = 0;
		let curator = 1;
		let bounty_id = 0;
		let asset_kind = 1;
		let value = 88;
		let fee = 42;
		Balances::make_free_balance_be(&curator, 100);
		// Allow for a larger spend limit:
		SpendLimit::set(value);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(0),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));

		// When
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator), bounty_id, proposer));

		// Then
		let expected_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(Balances::free_balance(&curator), 100 - expected_deposit);
		assert_eq!(Balances::reserved_balance(&curator), expected_deposit);

		// Case 2: Lower bound
		// Given
		let curator = 2;
		let bounty_id = 1;
		let value = 35;
		let fee = 0;
		Balances::make_free_balance_be(&curator, 100);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(0),
			Box::new(1),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		go_to_block(4);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));

		// When
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator), bounty_id, 0));

		// Then
		let expected_deposit = CuratorDepositMin::get();
		assert_eq!(Balances::free_balance(&curator), 100 - expected_deposit);
		assert_eq!(Balances::reserved_balance(&curator), expected_deposit);

		// Case 3: Upper bound
		// Given
		let curator = 3;
		let bounty_id = 2;
		let value = 1_000_000;
		let fee = 50_000;
		let starting_balance = fee * 2;
		Balances::make_free_balance_be(&curator, starting_balance);
		Balances::make_free_balance_be(&0, starting_balance);
		// Allow for a larger spend limit:
		SpendLimit::set(value);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(0),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		go_to_block(6);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));

		// When
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator), bounty_id, 0));

		// Then
		let expected_deposit = CuratorDepositMax::get();
		assert_eq!(Balances::free_balance(&curator), starting_balance - expected_deposit);
		assert_eq!(Balances::reserved_balance(&curator), expected_deposit);
	});
}

#[test]
fn approve_bounty_works_second_instance() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let value = 10;
		let bounty_id = 0;
		assert_ok!(Bounties1::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));

		// When
		assert_ok!(Bounties1::approve_bounty(RuntimeOrigin::root(), bounty_id));

		// Then
		// Bounties 2 is funded
		let bounty_account =
			Bounties1::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		assert_eq!(paid(bounty_account, asset_kind), value);
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		// Bounties 1 is unchanged
		assert_eq!(paid(bounty_account, asset_kind), 0);
	});
}

#[test]
fn approve_bounty_insufficient_spend_limit_errors() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let proposer = 0;
		let asset_kind = 1;
		let bounty_id = 0;
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			51,
			b"123".to_vec()
		));

		// When/Then
		// 51 will not work since the limit is 50.
		SpendLimit::set(50);
		assert_noop!(
			Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id),
			Error::<Test>::InsufficientPermission
		);
	});
}

#[test]
fn approve_bounty_instance1_insufficient_spend_limit_errors() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		Balances::make_free_balance_be(&Treasury1::account_id(), 101);
		assert_eq!(Treasury1::pot(), 100);
		assert_ok!(Bounties1::propose_bounty(
			RuntimeOrigin::signed(0),
			Box::new(1),
			51,
			b"123".to_vec()
		));

		// When/Then
		// 51 will not work since the limit is 50.
		SpendLimit1::set(50);
		assert_noop!(
			Bounties1::approve_bounty(RuntimeOrigin::root(), 0),
			Error::<Test, Instance1>::InsufficientPermission
		);
	});
}

#[test]
fn propose_curator_insufficient_spend_limit_errors() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		// Temporarily set a larger spend limit;
		SpendLimit::set(51);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(0),
			Box::new(1),
			51,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		go_to_block(2);

		// When/Then
		SpendLimit::set(50);
		// 51 will not work since the limit is 50.
		assert_noop!(
			Bounties::propose_curator(RuntimeOrigin::root(), 0, 0, 0),
			Error::<Test>::InsufficientPermission
		);
	});
}

#[test]
fn propose_curator_instance1_insufficient_spend_limit_errors() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		// Temporarily set a larger spend limit;
		SpendLimit1::set(11);
		assert_ok!(Bounties1::propose_bounty(
			RuntimeOrigin::signed(0),
			Box::new(1),
			11,
			b"12345".to_vec()
		));
		assert_ok!(Bounties1::approve_bounty(RuntimeOrigin::root(), 0));

		// When/Then
		SpendLimit1::set(10);
		// 11 will not work since the limit is 10.
		assert_noop!(
			Bounties1::propose_curator(RuntimeOrigin::root(), 0, 0, 0),
			Error::<Test, Instance1>::InsufficientPermission
		);
	});
}

#[test]
fn check_and_process_funding_and_payout_payment_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given (approve_bounty)
		let proposer = 0;
		let user = 1;
		let asset_kind = 1;
		let value = 50;
		let bounty_id = 0;
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid { index: bounty_id, payment_id },
			BountiesEvent::BountyApproved { index: bounty_id },
		]);

		// When/Then (check BountyStatus::Approved - PaymentState::Attempted -
		// PaymentStatus::InProgress)
		set_status(payment_id, PaymentStatus::InProgress);
		assert_noop!(
			Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id),
			Error::<Test>::FundingInconclusive
		);

		// When/Then (check BountyStatus::Approved - PaymentState::Attempted -
		// PaymentStatus::Failure)
		set_status(payment_id, PaymentStatus::Failure);
		let res = Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id);
		assert_eq!(last_event(), BountiesEvent::PaymentFailed { index: bounty_id, payment_id });
		assert_eq!(res.unwrap().pays_fee, Pays::Yes);

		// When/Then (check BountyStatus::Approved - PaymentState::Failed)
		assert_noop!(
			Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id),
			Error::<Test>::UnexpectedStatus
		);

		// When (process BountyStatus::Approved and check PaymentState::Success)
		assert_ok!(Bounties::process_payment(RuntimeOrigin::signed(user), bounty_id));
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		assert_eq!(last_event(), BountiesEvent::Paid { index: bounty_id, payment_id });
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id));

		// Then
		assert_eq!(last_event(), BountiesEvent::BountyBecameActive { index: bounty_id });
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee: 0,
				asset_kind,
				value,
				curator_deposit: 0,
				bond: 85,
				status: BountyStatus::Funded
			}
		);

		// Given (claim_bounty)
		let curator = 4;
		let fee = 1;
		let curator_stash = 7;
		let beneficiary = 3;
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, curator, fee));
		Balances::make_free_balance_be(&curator, 6);
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator), 0, curator_stash));
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(curator), 0, beneficiary));
		go_to_block(5);
		assert_ok!(Bounties::claim_bounty(RuntimeOrigin::signed(beneficiary), 0));
		let curator_payment_id =
			get_payment_id(bounty_id, Some(curator_stash)).expect("no payment attempt");
		let beneficiary_payment_id =
			get_payment_id(bounty_id, Some(beneficiary)).expect("no payment attempt");
		set_status(beneficiary_payment_id, PaymentStatus::InProgress);
		expect_events(vec![
			BountiesEvent::Paid { index: bounty_id, payment_id: curator_payment_id },
			BountiesEvent::Paid { index: bounty_id, payment_id: beneficiary_payment_id },
			BountiesEvent::BountyClaimed { index: bounty_id, beneficiary, curator_stash },
		]);

		// When (check BountyStatus::PayoutAttempted - PaymentState::Attempted -
		// 2x PaymentStatus::InProgress)
		set_status(curator_payment_id, PaymentStatus::InProgress);
		assert_noop!(
			Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id),
			Error::<Test>::PayoutInconclusive
		);

		// When/Then (check BountyStatus::PayoutAttempted - 2x PaymentState::Attempted -
		// 1x PaymentStatus::Failure 1x PaymentStatus::InProgress)
		set_status(beneficiary_payment_id, PaymentStatus::Failure);
		unpay(beneficiary, asset_kind, value - fee);
		let res = Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id);
		assert_eq!(res.unwrap().pays_fee, Pays::Yes);
		assert_eq!(
			last_event(),
			BountiesEvent::PaymentFailed { index: bounty_id, payment_id: beneficiary_payment_id }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 3,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::PayoutAttempted {
					curator,
					curator_stash: (
						curator_stash,
						PaymentState::Attempted { id: curator_payment_id }
					),
					beneficiary: (beneficiary, PaymentState::Failed)
				}
			}
		);

		// When/Then (check BountyStatus::PayoutAttempted - 1x PaymentState::Failed -
		// 1x PaymentState::Attempted)
		assert_noop!(
			Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id),
			Error::<Test>::PayoutInconclusive
		);

		// When (process BountyStatus::PayoutAttempted)
		assert_ok!(Bounties::process_payment(RuntimeOrigin::signed(user), bounty_id));
		assert_noop!(
			Bounties::process_payment(RuntimeOrigin::signed(user), bounty_id),
			Error::<Test>::UnexpectedStatus
		);

		// Then
		let beneficiary_payment_id =
			get_payment_id(bounty_id, Some(beneficiary)).expect("no payment attempt");
		assert_eq!(
			last_event(),
			BountiesEvent::Paid { index: bounty_id, payment_id: beneficiary_payment_id }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 3,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::PayoutAttempted {
					curator,
					curator_stash: (
						curator_stash,
						PaymentState::Attempted { id: curator_payment_id }
					),
					beneficiary: (
						beneficiary,
						PaymentState::Attempted { id: beneficiary_payment_id }
					)
				}
			}
		);

		// When (check 1x PaymentState::Success 1x PaymentState::Attempted)
		approve_payment(beneficiary, bounty_id, asset_kind, value - fee);

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				curator_deposit: 3,
				asset_kind,
				value,
				bond: 85,
				status: BountyStatus::PayoutAttempted {
					curator,
					curator_stash: (
						curator_stash,
						PaymentState::Attempted { id: curator_payment_id }
					),
					beneficiary: (beneficiary, PaymentState::Succeeded)
				}
			}
		);
		assert!(get_payment_id(bounty_id, Some(beneficiary)).is_none());

		// When (check 2x PaymentState::Success)
		approve_payment(curator_stash, bounty_id, asset_kind, fee);

		// Then
		expect_events(vec![BountiesEvent::BountyPayoutProcessed {
			index: bounty_id,
			asset_kind,
			value: value - fee,
			beneficiary,
		}]);
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		assert_eq!(Balances::free_balance(bounty_account), 0);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(bounty_id), None);
		assert_eq!(pallet_bounties::BountyDescriptions::<Test>::get(bounty_id), None);
	});
}

#[test]
fn check_payment_status_approved_with_curator() {
	ExtBuilder::default().build_and_execute(|| {
		// Given (approve_bounty)
		let proposer = 0;
		let user = 1;
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let fee = 10;
		let bounty_id = 0;
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty_with_curator(
			RuntimeOrigin::root(),
			bounty_id,
			curator,
			fee
		));
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid { index: bounty_id, payment_id },
			BountiesEvent::BountyApproved { index: bounty_id },
			BountiesEvent::CuratorProposed { bounty_id, curator },
		]);

		// When/Then (check BountyStatus::ApprovedWithCurator - PaymentState::Attempted -
		// PaymentStatus::InProgress)
		set_status(payment_id, PaymentStatus::InProgress);
		assert_noop!(
			Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id),
			Error::<Test>::FundingInconclusive
		);

		// When/Then (check BountyStatus::ApprovedWithCurator - PaymentState::Attempted -
		// PaymentStatus::Failure)
		set_status(payment_id, PaymentStatus::Failure);
		let res = Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id);
		assert_eq!(res.unwrap().pays_fee, Pays::Yes);

		// When/Then (check BountyStatus::ApprovedWithCurator - PaymentState::Failed)
		assert_noop!(
			Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id),
			Error::<Test>::UnexpectedStatus
		);

		// When (process BountyStatus::ApprovedWithCurator and check PaymentState::Success)
		assert_ok!(Bounties::process_payment(RuntimeOrigin::signed(user), bounty_id));
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		assert_eq!(last_event(), BountiesEvent::Paid { index: bounty_id, payment_id });
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id));

		// Then
		assert_eq!(last_event(), BountiesEvent::BountyBecameActive { index: bounty_id });
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				proposer,
				fee,
				asset_kind,
				value,
				curator_deposit: 0,
				bond: 85,
				status: BountyStatus::CuratorProposed { curator }
			}
		);
	});
}

#[test]
fn check_and_process_refund_payment_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given (approve_bounty)
		let user = 1;
		let asset_kind = 1;
		let bounty_id = 0;
		let value = 50;
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(0),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id));
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), 0));
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid { index: bounty_id, payment_id },
			BountiesEvent::BountyCanceled { index: bounty_id },
		]);

		// When/Then (check BountyStatus::RefundAttempted - PaymentState::Attempted -
		// PaymentStatus::InProgress)
		set_status(payment_id, PaymentStatus::InProgress);
		assert_noop!(
			Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id),
			Error::<Test>::RefundInconclusive
		);

		// When/Then (check BountyStatus::RefundAttempted - PaymentState::Attempted -
		// PaymentStatus::Failure)
		set_status(payment_id, PaymentStatus::Failure);
		let res = Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id);
		assert_eq!(res.unwrap().pays_fee, Pays::Yes);

		// When/Then (check BountyStatus::RefundAttempted - PaymentState::Failed)
		assert_noop!(
			Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id),
			Error::<Test>::UnexpectedStatus
		);

		// When (process BountyStatus::RefundAttempted and check PaymentState::Success)
		assert_ok!(Bounties::process_payment(RuntimeOrigin::signed(user), bounty_id));
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		assert_eq!(last_event(), BountiesEvent::Paid { index: bounty_id, payment_id });
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Bounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id));

		// Then
		assert_eq!(last_event(), BountiesEvent::BountyRefundProcessed { index: bounty_id });
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		assert_eq!(Balances::free_balance(bounty_account), 0);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(bounty_id), None);
		assert_eq!(pallet_bounties::BountyDescriptions::<Test>::get(bounty_id), None);
	});
}

#[test]
fn accept_curator_sets_update_due_correctly() {
	ExtBuilder::default().build_and_execute(|| {
		// Given (BountyUpdatePeriod = 20)
		let bounty_id = 0;
		let proposer = 0;
		let fee = 10;
		let value = 50;
		let asset_kind = 1;
		let curator = 4;
		let curator_stash = 7;
		Balances::make_free_balance_be(&curator, 12);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));

		// When
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			bounty_id,
			curator_stash
		));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap().status,
			BountyStatus::Active { curator, curator_stash, update_due: 21 }
		);

		// Given (BountyUpdatePeriod = BlockNumber::max_value())
		let bounty_id = 1;
		BountyUpdatePeriod::set(SystemBlockNumberFor::<Test>::max_value());
		Balances::make_free_balance_be(&Treasury1::account_id(), 101);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(proposer),
			Box::new(asset_kind),
			value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		approve_payment(bounty_account, bounty_id, asset_kind, value);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), bounty_id, curator, fee));

		// When
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			bounty_id,
			curator_stash
		));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap().status,
			BountyStatus::Active {
				curator,
				curator_stash,
				update_due: SystemBlockNumberFor::<Test>::max_value()
			}
		);

		// When
		assert_ok!(Bounties::extend_bounty_expiry(
			RuntimeOrigin::signed(curator),
			bounty_id,
			Vec::new()
		));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap().status,
			BountyStatus::Active {
				curator,
				curator_stash,
				update_due: SystemBlockNumberFor::<Test>::max_value()
			}
		);
	});
}
