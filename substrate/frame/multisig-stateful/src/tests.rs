use crate::PendingProposals;
use crate::{mock::*, Error, MultisigAccount, Timepoint};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::BoundedBTreeSet;

// Helpers - Only call in a TestExternality
fn now() -> Timepoint<u64> {
	Multisig::timepoint()
}

fn add_alice_bob_charlie_dave_multisig(threshold: u32) -> u64 {
	let mut owners = BoundedBTreeSet::new();
	owners.try_insert(ALICE).unwrap();
	owners.try_insert(BOB).unwrap();
	owners.try_insert(CHARLIE).unwrap();
	owners.try_insert(DAVE).unwrap();

	let multisig_account = Multisig::get_multisig_account_id(&owners, now());
	let alice_current_balance = Balances::free_balance(&ALICE);
	assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(ALICE), owners, threshold));
	assert!(MultisigAccount::<Test>::contains_key(&multisig_account));
	// 3 owners
	assert!(MultisigAccount::<Test>::get(&multisig_account).unwrap().owners.len() == 4);
	// reserved creation deposit
	assert_eq!(Balances::free_balance(&ALICE), alice_current_balance - CreationDeposit::get());

	multisig_account
}

fn add_alice_bob_charlie_multisig(threshold: u32) -> u64 {
	let mut owners = BoundedBTreeSet::new();
	owners.try_insert(ALICE).unwrap();
	owners.try_insert(BOB).unwrap();
	owners.try_insert(CHARLIE).unwrap();

	let multisig_account = Multisig::get_multisig_account_id(&owners, now());
	let alice_current_balance = Balances::free_balance(&ALICE);
	assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(ALICE), owners, threshold));
	assert!(MultisigAccount::<Test>::contains_key(&multisig_account));
	// 3 owners
	assert!(MultisigAccount::<Test>::get(&multisig_account).unwrap().owners.len() == 3);
	// reserved creation deposit
	assert_eq!(Balances::free_balance(&ALICE), alice_current_balance - CreationDeposit::get());

	multisig_account
}

fn add_alice_bob_multisig(threshold: u32) -> u64 {
	let mut owners = BoundedBTreeSet::new();
	owners.try_insert(ALICE).unwrap();
	owners.try_insert(BOB).unwrap();

	let multisig_account = Multisig::get_multisig_account_id(&owners, now());
	let alice_current_balance = Balances::free_balance(&ALICE);
	assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(ALICE), owners, threshold));
	assert!(MultisigAccount::<Test>::contains_key(&multisig_account));
	// 2 owners
	assert!(MultisigAccount::<Test>::get(&multisig_account).unwrap().owners.len() == 2);
	// reserved creation deposit
	assert_eq!(Balances::free_balance(&ALICE), alice_current_balance - CreationDeposit::get());

	multisig_account
}

// Makes a multisig with alice as the only owner and threshold 1
fn add_alice_multisig() -> u64 {
	let mut owners = BoundedBTreeSet::new();
	owners.try_insert(ALICE).unwrap();

	let multisig_account = Multisig::get_multisig_account_id(&owners, now());
	let alice_current_balance = Balances::free_balance(&ALICE);
	assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(ALICE), owners, 1));
	assert!(MultisigAccount::<Test>::contains_key(&multisig_account));
	// 1 owner
	assert!(MultisigAccount::<Test>::get(&multisig_account).unwrap().owners.len() == 1);
	// reserved creation deposit
	assert_eq!(Balances::free_balance(&ALICE), alice_current_balance - CreationDeposit::get());

	multisig_account
}

fn transfer(src: u64, dest: u64, value: u128) {
	assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(src), dest, value));
}

fn construc_transfer_call(dest: u64, value: u128) -> Box<RuntimeCall> {
	Box::new(RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
		dest,
		value: value.into(),
	}))
}

fn start_alice_bob_charlie_multisig_proposal(
	call: Box<RuntimeCall>,
	threshold: u32,
	proposer: u64,
) -> (u64, Hash) {
	let call_hash = Multisig::hash_of(&call.clone());
	let multisig_account = add_alice_bob_charlie_multisig(threshold);

	let proposer_current_balance = Balances::free_balance(&proposer);
	assert_ok!(Multisig::start_proposal(
		RuntimeOrigin::signed(proposer),
		multisig_account,
		call_hash,
	));
	assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));
	// reserved creation deposit
	assert_eq!(
		Balances::free_balance(&proposer),
		proposer_current_balance - ProposalDeposit::get()
	);
	System::assert_has_event(
		crate::Event::StartedProposal { proposer, multisig_account, call_hash }.into(),
	);
	(multisig_account, call_hash)
}

fn start_alice_bob_charlie_dave_multisig_proposal(
	call: Box<RuntimeCall>,
	threshold: u32,
	proposer: u64,
) -> (u64, Hash) {
	let call_hash = Multisig::hash_of(&call.clone());
	let multisig_account = add_alice_bob_charlie_dave_multisig(threshold);

	let proposer_current_balance = Balances::free_balance(&proposer);
	assert_ok!(Multisig::start_proposal(
		RuntimeOrigin::signed(proposer),
		multisig_account,
		call_hash,
	));
	assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));
	// reserved creation deposit
	assert_eq!(
		Balances::free_balance(&proposer),
		proposer_current_balance - ProposalDeposit::get()
	);
	System::assert_has_event(
		crate::Event::StartedProposal { proposer, multisig_account, call_hash }.into(),
	);
	(multisig_account, call_hash)
}
// End Helpers

#[test]
fn create_multisig_one_owner() {
	new_test_ext().execute_with(|| {
		let mut owners = BoundedBTreeSet::new();
		owners.try_insert(ALICE).unwrap();

		let multisig_account = Multisig::get_multisig_account_id(&owners, now());
		// No multisig account should exist before creating it
		assert!(!MultisigAccount::<Test>::contains_key(&multisig_account));
		// Create multisig account with ALICE as the first signer
		assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(ALICE), owners.clone(), 1));
		// Check that the multisig account exists
		assert!(MultisigAccount::<Test>::contains_key(&multisig_account));
		let multisig_details = MultisigAccount::<Test>::get(&multisig_account).unwrap();
		assert_eq!(multisig_details.owners, owners);
		assert_eq!(multisig_details.threshold, 1);

		System::assert_last_event(
			crate::Event::CreatedMultisig { multisig_account, created_by: ALICE }.into(),
		);
	});
}

#[test]
fn create_multisig_multiple_owners() {
	new_test_ext().execute_with(|| {
		let mut owners = BoundedBTreeSet::new();
		owners.try_insert(BOB).unwrap();
		owners.try_insert(CHARLIE).unwrap();

		let multisig_account = Multisig::get_multisig_account_id(&owners, now());
		// No multisig account should exist before creating it
		assert!(!MultisigAccount::<Test>::contains_key(&multisig_account));
		// Create multisig account, someone other than the owners can send the request without issues
		assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(ALICE), owners.clone(), 2));
		// Check that the multisig account exists
		assert!(MultisigAccount::<Test>::contains_key(&multisig_account));
		let multisig_details = MultisigAccount::<Test>::get(&multisig_account).unwrap();
		assert_eq!(multisig_details.owners, owners.into_inner());
		assert_eq!(multisig_details.threshold, 2);
	});
}

#[test]
fn create_multisig_fails_wrong_threshold() {
	new_test_ext().execute_with(|| {
		let mut owners = BoundedBTreeSet::new();
		owners.try_insert(BOB).unwrap();
		let multisig_account = Multisig::get_multisig_account_id(&owners, now());
		// No multisig account should exist before creating it
		assert!(!MultisigAccount::<Test>::contains_key(&multisig_account));
		// Create multisig account with ALICE as the first signer
		assert_noop!(
			Multisig::create_multisig(RuntimeOrigin::signed(ALICE), BoundedBTreeSet::default(), 2),
			Error::<Test>::InvalidThreshold
		);

		assert_noop!(
			Multisig::create_multisig(RuntimeOrigin::signed(ALICE), BoundedBTreeSet::default(), 0),
			Error::<Test>::InvalidThreshold
		);
	});
}

#[test]
fn multisig_one_owner_add_owner() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_multisig();

		let add_owner_call: RuntimeCall =
			crate::Call::add_owner { new_owner: BOB, new_threshold: 2 }.into();

		let alice_current_balance = Balances::free_balance(&ALICE);
		// After calling as multisig, the multisig account should have BOB as an owner and the threshold should be 2
		// This is due to the fact that the multisig account is created with only ALICE as an owner and threshold 1
		// which will pass the threshold check for the call.
		assert_ok!(Multisig::start_proposal(
			RuntimeOrigin::signed(ALICE),
			multisig_account,
			Multisig::hash_of(&add_owner_call.clone()),
		));

		// reserved proposal deposit
		assert_eq!(Balances::free_balance(&ALICE), alice_current_balance - ProposalDeposit::get());

		assert_ok!(Multisig::execute_proposal(
			RuntimeOrigin::signed(ALICE),
			multisig_account,
			Box::new(add_owner_call.clone()),
		));

		// release deposit
		assert_eq!(Balances::free_balance(&ALICE), alice_current_balance);

		let multisig_details = MultisigAccount::<Test>::get(&multisig_account).unwrap();
		assert!(multisig_details.owners.contains(&BOB));
		assert_eq!(multisig_details.threshold, 2);

		System::assert_last_event(
			crate::Event::ExecutedProposal {
				executor: 1,
				multisig_account,
				call_hash: Multisig::hash_of(&add_owner_call),
				result: Ok(()),
			}
			.into(),
		);
	});
}

#[test]
fn multisig_2_of_2_add_owner() {
	new_test_ext().execute_with(|| {
		let bob_initial_balance = Balances::free_balance(&BOB);
		let multisig_account = add_alice_bob_multisig(2);

		let add_charlie_call: RuntimeCall =
			crate::Call::add_owner { new_owner: CHARLIE, new_threshold: 2 }.into();

		let call_hash = Multisig::hash_of(&add_charlie_call.clone());

		let alice_current_balance = Balances::free_balance(&ALICE);

		assert_ok!(Multisig::start_proposal(
			RuntimeOrigin::signed(ALICE),
			multisig_account,
			Multisig::hash_of(&add_charlie_call.clone()),
		));

		// reserved proposal deposit
		assert_eq!(Balances::free_balance(&ALICE), alice_current_balance - ProposalDeposit::get());
		// Bob stays the same
		assert_eq!(Balances::free_balance(&BOB), bob_initial_balance);

		let multisig_details = MultisigAccount::<Test>::get(&multisig_account).unwrap();
		// Shouldn't contain CHARLIE yet
		assert!(!multisig_details.owners.contains(&CHARLIE));

		let proposal = PendingProposals::<Test>::get(&multisig_account, call_hash).unwrap();
		// Only ALICE approval should be present
		assert!(proposal.approvers.len() == 1);
		assert!(proposal.approvers.contains(&ALICE));

		// Call again with BOB
		assert_ok!(Multisig::approve(RuntimeOrigin::signed(BOB), multisig_account, call_hash,));

		// Though BOB is the one who's executing, the deposit should be released to ALICE
		assert_ok!(Multisig::execute_proposal(
			RuntimeOrigin::signed(BOB),
			multisig_account,
			Box::new(add_charlie_call),
		));

		// release deposit to ALICE
		assert_eq!(Balances::free_balance(&ALICE), alice_current_balance);
		// BOB stays the same
		assert_eq!(Balances::free_balance(&BOB), bob_initial_balance);

		let multisig_details = MultisigAccount::<Test>::get(&multisig_account).unwrap();
		// Shoul contain CHARLIE
		assert!(multisig_details.owners.contains(&CHARLIE));
		// No proposal should exist anymore
		assert!(!PendingProposals::<Test>::contains_key(&multisig_account, call_hash))
	});
}

#[test]
fn multisig_2_of_3() {
	new_test_ext().execute_with(|| {
		// Start by making sure Eve doesn't have any balance
		assert_eq!(Balances::free_balance(EVE), 0);
		let call = construc_transfer_call(EVE, 15);
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_multisig_proposal(call.clone(), 2, ALICE);

		transfer(ALICE, multisig_account, 5);
		transfer(BOB, multisig_account, 5);
		transfer(CHARLIE, multisig_account, 5);

		assert_eq!(Balances::free_balance(multisig_account), 15);
		// Starting a proposal is not anough to transfer to Eve as the threshold is 2
		assert_eq!(Balances::free_balance(EVE), 0);

		// Approve with BOB
		assert_ok!(Multisig::approve(RuntimeOrigin::signed(BOB), multisig_account, call_hash));

		assert_ok!(Multisig::execute_proposal(
			RuntimeOrigin::signed(CHARLIE),
			multisig_account,
			call.clone(),
		));

		// Eve has 15 balance now since the call has been approved by 2 members.
		assert_eq!(Balances::free_balance(EVE), 15);
		assert_eq!(Balances::free_balance(multisig_account), 0);
		// No proposal should exist anymore
		assert!(!PendingProposals::<Test>::contains_key(&multisig_account, call_hash))
	});
}

#[test]
fn multisig_3_of_3() {
	new_test_ext().execute_with(|| {
		// Start by making sure Eve doesn't have any balance
		assert_eq!(Balances::free_balance(EVE), 0);
		let call = construc_transfer_call(EVE, 15);
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_multisig_proposal(call.clone(), 3, ALICE);

		transfer(ALICE, multisig_account, 5);
		transfer(BOB, multisig_account, 5);
		transfer(CHARLIE, multisig_account, 5);

		assert_eq!(Balances::free_balance(multisig_account), 15);
		// Starting a proposal is not anough to transfer to Eve as the threshold is 2
		assert_eq!(Balances::free_balance(EVE), 0);

		// Bob approves
		assert_ok!(Multisig::approve(RuntimeOrigin::signed(BOB), multisig_account, call_hash));

		// Still not enough approvers
		assert_eq!(Balances::free_balance(multisig_account), 15);
		assert_eq!(Balances::free_balance(EVE), 0);

		// Charlie approves
		assert_ok!(Multisig::approve(RuntimeOrigin::signed(CHARLIE), multisig_account, call_hash));

		// Now we have enough approvers
		assert_ok!(Multisig::execute_proposal(
			RuntimeOrigin::signed(CHARLIE),
			multisig_account,
			call.clone(),
		));

		// Eve has 15 balance now since the call has been approved by 2 members.
		assert_eq!(Balances::free_balance(EVE), 15);
		assert_eq!(Balances::free_balance(multisig_account), 0);
		// No proposal should exist anymore
		assert!(!PendingProposals::<Test>::contains_key(&multisig_account, call_hash))
	});
}

#[test]
fn starting_same_approval_fails() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_charlie_multisig(2);

		transfer(ALICE, multisig_account, 5);
		transfer(BOB, multisig_account, 5);
		transfer(CHARLIE, multisig_account, 5);

		let call: Box<RuntimeCall> = construc_transfer_call(EVE, 15);

		// Eve has no initial balance
		assert_eq!(Balances::free_balance(EVE), 0);

		let call_hash = Multisig::hash_of(&call.clone());

		// Bob approving
		assert_ok!(Multisig::start_proposal(
			RuntimeOrigin::signed(BOB),
			multisig_account,
			call_hash,
		));

		// Eve still has 0 balance since the call hasn't been approved by 2 members yet.
		assert_eq!(Balances::free_balance(EVE), 0);

		// Bob starts the same proposal again
		assert_noop!(
			Multisig::start_proposal(RuntimeOrigin::signed(BOB), multisig_account, call_hash),
			Error::<Test>::ProposalAlreadyExists
		);

		// Eve still has only 0 balance
		assert_eq!(Balances::free_balance(EVE), 0);
		assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));
		// Only one approver still
		assert!(
			PendingProposals::<Test>::get(&multisig_account, &call_hash)
				.unwrap()
				.approvers
				.len() == 1
		);
	});
}

// NOTE: Next tests assumes that the origin is a multisig account which passed the threshold check in a previous start_proposal call that was tested above with both multisig pallet calls and balances calls.

#[test]
fn add_owner_works() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_multisig(2);
		assert_ok!(Multisig::add_owner(RuntimeOrigin::signed(multisig_account), CHARLIE, 3));
		let multisig_details = MultisigAccount::<Test>::get(&multisig_account).unwrap();
		assert!(multisig_details.owners.contains(&CHARLIE));
		assert!(multisig_details.owners.len() == 3);
		assert!(multisig_details.threshold == 3);
		System::assert_has_event(
			crate::Event::AddedOwner { multisig_account, added_owner: CHARLIE, threshold: 3 }
				.into(),
		);
	});
}

#[test]
fn add_owner_fails_when_wrong_threshold() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_multisig(2);
		assert_noop!(
			Multisig::add_owner(RuntimeOrigin::signed(multisig_account), multisig_account, 4),
			Error::<Test>::InvalidThreshold
		);
	});
}

#[test]
fn add_owner_fails_when_wrong_multisig_origin() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_multisig(2);
		assert_noop!(
			Multisig::add_owner(RuntimeOrigin::signed(ALICE), multisig_account, 4),
			Error::<Test>::MultisigNotFound
		);
	});
}

#[test]
fn add_owner_fails_when_existing_owner_added() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_multisig(2);
		assert_noop!(
			Multisig::add_owner(RuntimeOrigin::signed(multisig_account), ALICE, 2),
			Error::<Test>::OwnerAlreadyExists
		);
	});
}

#[test]
fn add_owner_fails_when_more_owners_than_max() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_charlie_multisig(2);
		assert_ok!(Multisig::add_owner(RuntimeOrigin::signed(multisig_account), DAVE, 2));
		assert!(MultisigAccount::<Test>::get(&multisig_account).unwrap().owners.len() == 4);

		assert_noop!(
			Multisig::add_owner(RuntimeOrigin::signed(multisig_account), EVE, 2),
			Error::<Test>::TooManyOwners
		);
	});
}

#[test]
fn remove_owner_works() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_charlie_multisig(2);
		assert_ok!(Multisig::remove_owner(RuntimeOrigin::signed(multisig_account), CHARLIE, 1));
		let multisig_details = MultisigAccount::<Test>::get(&multisig_account).unwrap();
		assert!(multisig_details.owners.len() == 2);
		// Charlie deleted
		assert!(!multisig_details.owners.contains(&CHARLIE));
		assert_eq!(multisig_details.threshold, 1);
		System::assert_has_event(
			crate::Event::RemovedOwner { multisig_account, removed_owner: CHARLIE, threshold: 1 }
				.into(),
		);
	});
}

#[test]
fn remove_owner_deletes_multisig_when_only_one_owner_left() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_multisig();
		let alice_current_balance = Balances::free_balance(&ALICE);

		assert_ok!(Multisig::remove_owner(RuntimeOrigin::signed(multisig_account), ALICE, 0));

		assert!(MultisigAccount::<Test>::get(&multisig_account).is_none());
		// Return deposit after deletion
		assert_eq!(Balances::free_balance(&ALICE), alice_current_balance + CreationDeposit::get());

		System::assert_has_event(
			crate::Event::RemovedOwner { multisig_account, removed_owner: ALICE, threshold: 0 }
				.into(),
		);
		System::assert_has_event(crate::Event::DeletedMultisig { multisig_account }.into());
	});
}

#[test]
fn remove_owner_fails_when_only_one_owner_left_and_threshold_above_zero() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_multisig();
		assert_noop!(
			Multisig::remove_owner(RuntimeOrigin::signed(multisig_account), ALICE, 1),
			Error::<Test>::InvalidThreshold
		);
	});
}

#[test]
fn remove_owner_fails_for_normal_signed_account() {
	new_test_ext().execute_with(|| {
		add_alice_bob_charlie_multisig(2);
		assert_noop!(
			Multisig::remove_owner(RuntimeOrigin::signed(ALICE), CHARLIE, 2),
			Error::<Test>::MultisigNotFound
		);
	});
}

#[test]
fn remove_owner_fails_if_theshold_is_more() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_charlie_multisig(2);
		assert_noop!(
			Multisig::remove_owner(RuntimeOrigin::signed(multisig_account), CHARLIE, 3),
			Error::<Test>::InvalidThreshold
		);
	});
}

#[test]
fn remove_owner_fails_if_owner_doesnt_exist() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_charlie_multisig(2);
		assert_noop!(
			Multisig::remove_owner(RuntimeOrigin::signed(multisig_account), EVE, 2),
			Error::<Test>::OwnerNotFound
		);
	});
}

#[test]
fn set_threshold_works() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_charlie_multisig(2);
		assert_ok!(Multisig::set_threshold(RuntimeOrigin::signed(multisig_account), 1));
		let multisig_details = MultisigAccount::<Test>::get(&multisig_account).unwrap();
		assert_eq!(multisig_details.threshold, 1);
		System::assert_has_event(
			crate::Event::ChangedThreshold { multisig_account, new_threshold: 1 }.into(),
		);
	});
}

#[test]
fn set_threshold_fails_if_invalid() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_charlie_multisig(3);
		assert_noop!(
			Multisig::set_threshold(RuntimeOrigin::signed(multisig_account), 4),
			Error::<Test>::InvalidThreshold
		);
		assert_noop!(
			Multisig::set_threshold(RuntimeOrigin::signed(multisig_account), 0),
			Error::<Test>::InvalidThreshold
		);
	});
}

#[test]
fn set_threshold_fails_if_not_owner() {
	new_test_ext().execute_with(|| {
		add_alice_bob_charlie_multisig(3);
		assert_noop!(
			Multisig::set_threshold(RuntimeOrigin::signed(ALICE), 2),
			Error::<Test>::MultisigNotFound
		);
	});
}

#[test]
fn approve_works() {
	new_test_ext().execute_with(|| {
		// The call itself doesn't matter in this test, we're testing only that approvers add an approver to the proposal
		let call = construc_transfer_call(EVE, 15);
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_multisig_proposal(call.clone(), 2, ALICE);
		let proposal = PendingProposals::<Test>::get(&multisig_account, call_hash).unwrap();

		// Only ALICE approval should be present
		assert!(proposal.approvers.len() == 1);
		assert!(proposal.approvers.contains(&ALICE));

		// Bob approves
		assert_ok!(Multisig::approve(RuntimeOrigin::signed(BOB), multisig_account, call_hash));
		let proposal_after_approval =
			PendingProposals::<Test>::get(&multisig_account, call_hash).unwrap();
		assert!(proposal_after_approval.approvers.len() == 2);
		assert!(proposal_after_approval.approvers.contains(&ALICE));
		assert!(proposal_after_approval.approvers.contains(&BOB));
		System::assert_has_event(
			crate::Event::ApprovedProposal {
				approving_account: BOB, // Bob was the approving account
				multisig_account,
				call_hash,
			}
			.into(),
		);
	});
}

#[test]
fn duplicate_approvers_fail() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15); // The call itself doesn't matter in this test
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_multisig_proposal(call.clone(), 2, ALICE);

		// Approving again should fail
		assert_noop!(
			Multisig::approve(RuntimeOrigin::signed(ALICE), multisig_account, call_hash),
			Error::<Test>::AlreadyApproved
		);
	});
}

#[test]
fn revoke_approval_works() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15); // The call itself doesn't matter in this test
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_multisig_proposal(call.clone(), 2, ALICE);

		let proposal = PendingProposals::<Test>::get(&multisig_account, call_hash).unwrap();
		// Only ALICE approval should be present
		assert!(proposal.approvers.len() == 1);
		assert!(proposal.approvers.contains(&ALICE));

		// Revoke approval with ALICE
		assert_ok!(Multisig::revoke(RuntimeOrigin::signed(ALICE), multisig_account, call_hash));

		let proposal = PendingProposals::<Test>::get(&multisig_account, call_hash).unwrap();
		// No approvers should be present
		// Don't delete the proposal itself if all approvers are revoked.
		assert!(proposal.approvers.len() == 0);

		System::assert_has_event(
			crate::Event::RevokedApproval { revoking_account: ALICE, multisig_account, call_hash }
				.into(),
		);
	});
}

#[test]
fn revoke_approval_fails_if_owner_not_in_approvers() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15); // The call itself doesn't matter in this test
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_multisig_proposal(call.clone(), 2, ALICE);

		// Can't revoke approval with BOB because he's not in the approvers
		assert_noop!(
			Multisig::revoke(RuntimeOrigin::signed(BOB), multisig_account, call_hash),
			Error::<Test>::OwnerNotFound
		);
	});
}

#[test]
fn revoke_approval_fails_if_not_owner() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15); // The call itself doesn't matter in this test
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_multisig_proposal(call.clone(), 2, ALICE);

		// Revoke approval with EVE fails because she's not an owner
		assert_noop!(
			Multisig::revoke(RuntimeOrigin::signed(EVE), multisig_account, call_hash),
			Error::<Test>::UnAuthorizedOwner
		);
	});
}

#[test]
fn cancel_proposal_works() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15); // The call itself doesn't matter in this test
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_multisig_proposal(call.clone(), 2, ALICE);

		// Proposal should exist
		assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));

		let alice_current_balance = Balances::free_balance(&ALICE);

		// Cancel proposal, only multisig accounts can cancel proposals directly. So it passed approvers process by the time it reaches this call.
		assert_ok!(Multisig::cancel_proposal(RuntimeOrigin::signed(multisig_account), call_hash));

		// Proposal should not exist anymore
		assert!(!PendingProposals::<Test>::contains_key(&multisig_account, call_hash));
		// Deposit released
		assert_eq!(Balances::free_balance(&ALICE), alice_current_balance + ProposalDeposit::get());

		System::assert_has_event(
			crate::Event::CanceledProposal { multisig_account, call_hash }.into(),
		);
	});
}

#[test]
fn cancel_proposal_fails_if_multisig_doesnt_exist() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15); // The call itself doesn't matter in this test
		let (_, call_hash) = start_alice_bob_charlie_multisig_proposal(call.clone(), 2, ALICE);
		let non_existent_multisig_account = 2199;

		// Cancel proposal with EVE fails because she's not an owner
		assert_noop!(
			Multisig::cancel_proposal(
				RuntimeOrigin::signed(non_existent_multisig_account),
				call_hash
			),
			Error::<Test>::MultisigNotFound
		);
	});
}

#[test]
fn cancel_proposal_fails_if_no_proposal() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_charlie_multisig(2);
		let call_hash = Multisig::hash_of(&construc_transfer_call(EVE, 15)); // The call itself doesn't matter in this test

		// Proposal should not exist
		assert!(!PendingProposals::<Test>::contains_key(&multisig_account, call_hash));

		// Cancel proposal
		assert_noop!(
			Multisig::cancel_proposal(RuntimeOrigin::signed(multisig_account), call_hash),
			Error::<Test>::ProposalNotFound
		);
	});
}

#[test]
fn delete_account_works() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_charlie_multisig(2);

		// Account exists
		assert!(MultisigAccount::<Test>::contains_key(&multisig_account));

		let current_alice_balance = Balances::free_balance(&ALICE);
		// The deletion happens after getting enough approvers already
		assert_ok!(Multisig::delete_account(RuntimeOrigin::signed(multisig_account)));
		// Account deleted
		assert!(!MultisigAccount::<Test>::contains_key(&multisig_account));
		// return creation deposit after deletion
		assert_eq!(Balances::free_balance(&ALICE), current_alice_balance + CreationDeposit::get());
		System::assert_last_event(crate::Event::DeletedMultisig { multisig_account }.into());
	});
}

#[test]
fn delete_account_fails_if_not_multisig() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_charlie_multisig(2);
		let non_existent_multisig_account = 2199;

		// Account exists
		assert!(MultisigAccount::<Test>::contains_key(&multisig_account));

		// Account doesn't exist
		assert!(!MultisigAccount::<Test>::contains_key(&non_existent_multisig_account));

		// The deletion happens after getting enough approvers already
		assert_noop!(
			Multisig::delete_account(RuntimeOrigin::signed(non_existent_multisig_account)),
			Error::<Test>::MultisigNotFound
		);

		// Account still exists
		assert!(MultisigAccount::<Test>::contains_key(&multisig_account));
	});
}

#[test]
fn cancel_own_proposal_works() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15); // The call itself doesn't matter in this test
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_multisig_proposal(call.clone(), 2, ALICE);

		// Proposal should exist
		assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));

		let alice_current_balance = Balances::free_balance(&ALICE);

		// Alice canceling own proposal
		assert_ok!(Multisig::cancel_own_proposal(
			RuntimeOrigin::signed(ALICE),
			multisig_account,
			call_hash
		));

		// Proposal should not exist anymore
		assert!(!PendingProposals::<Test>::contains_key(&multisig_account, call_hash));

		// Deposit released
		assert_eq!(Balances::free_balance(&ALICE), alice_current_balance + ProposalDeposit::get());

		System::assert_has_event(
			crate::Event::CanceledProposal { multisig_account, call_hash }.into(),
		);
	});
}

#[test]
fn cancel_others_proposal_fails() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15); // The call itself doesn't matter in this test
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_multisig_proposal(call.clone(), 2, ALICE);

		// Proposal should exist
		assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));

		// BOB tries to cancel Alice's proposal
		assert_noop!(
			Multisig::cancel_own_proposal(RuntimeOrigin::signed(BOB), multisig_account, call_hash),
			Error::<Test>::UnAuthorizedOwner
		);
	});
}
//==================================================================================================
// 									  Edge cases
//==================================================================================================
#[test]
fn remove_owner_same_threshold_during_active_proposal() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15);
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_dave_multisig_proposal(call.clone(), 3, ALICE); // Start with threshold 3

		transfer(ALICE, multisig_account, 5);
		transfer(BOB, multisig_account, 5);
		transfer(CHARLIE, multisig_account, 5);

		// Proposal should exist
		assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));

		// Bob and Charlie approve, We have in total 3 approvers to execute the proposal.
		assert_ok!(Multisig::approve(RuntimeOrigin::signed(BOB), multisig_account, call_hash));
		assert_ok!(Multisig::approve(RuntimeOrigin::signed(CHARLIE), multisig_account, call_hash));

		// Now remove Charlie from the multisig account
		assert_ok!(Multisig::remove_owner(RuntimeOrigin::signed(multisig_account), CHARLIE, 3));

		// Proposal should still exist
		assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));

		// Charlie should not be an owner anymore
		assert!(!MultisigAccount::<Test>::get(&multisig_account)
			.unwrap()
			.owners
			.contains(&CHARLIE));

		// Executing proposal fails because the threshold is 3 and one of the 3 approvers is not an owner anymore.
		assert_noop!(
			Multisig::execute_proposal(
				RuntimeOrigin::signed(ALICE),
				multisig_account,
				call.clone()
			),
			Error::<Test>::NotEnoughApprovers
		);

		// Dave approves
		assert_ok!(Multisig::approve(RuntimeOrigin::signed(DAVE), multisig_account, call_hash));

		// Now we can execute the proposal
		assert_ok!(Multisig::execute_proposal(
			RuntimeOrigin::signed(ALICE),
			multisig_account,
			call
		));
		// Proposal removed after executing
		assert!(!PendingProposals::<Test>::contains_key(&multisig_account, call_hash));
		assert!(Balances::free_balance(multisig_account) == 0);
		assert!(Balances::free_balance(EVE) == 15);
	});
}

#[test]
fn change_threshold_down_while_proposal_active_works() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15);
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_dave_multisig_proposal(call.clone(), 3, ALICE); // Start with threshold 3

		transfer(ALICE, multisig_account, 5);
		transfer(BOB, multisig_account, 5);
		transfer(CHARLIE, multisig_account, 5);

		// Proposal should exist
		assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));

		// Bob approves, We have in total 2 approvers to execute the proposal.
		assert_ok!(Multisig::approve(RuntimeOrigin::signed(BOB), multisig_account, call_hash));

		// Execute proposal fails because the threshold is 3 and we have only 2 approvers
		assert_noop!(
			Multisig::execute_proposal(
				RuntimeOrigin::signed(ALICE),
				multisig_account,
				call.clone()
			),
			Error::<Test>::NotEnoughApprovers
		);

		// Now change the threshold to 2
		assert_ok!(Multisig::set_threshold(RuntimeOrigin::signed(multisig_account), 2));

		// Now we can execute the proposal
		assert_ok!(Multisig::execute_proposal(
			RuntimeOrigin::signed(ALICE),
			multisig_account,
			call
		));
		// Proposal removed after executing
		assert!(!PendingProposals::<Test>::contains_key(&multisig_account, call_hash));
		assert!(Balances::free_balance(multisig_account) == 0);
		assert!(Balances::free_balance(EVE) == 15);
	});
}

#[test]
fn change_threshold_up_while_proposal_active_works() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15); // The call itself doesn't matter in this test
		let (multisig_account, call_hash) =
			start_alice_bob_charlie_dave_multisig_proposal(call.clone(), 2, ALICE); // Start with threshold 2

		// Proposal should exist
		assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));

		// Bob approves, We have in total 2 approvers to execute the proposal.
		assert_ok!(Multisig::approve(RuntimeOrigin::signed(BOB), multisig_account, call_hash));

		// Now change the threshold to 3
		assert_ok!(Multisig::set_threshold(RuntimeOrigin::signed(multisig_account), 3));

		// Execute proposal fails because the threshold is 3 and we have only 2 approvers
		assert_noop!(
			Multisig::execute_proposal(
				RuntimeOrigin::signed(ALICE),
				multisig_account,
				call.clone()
			),
			Error::<Test>::NotEnoughApprovers
		);
	});
}

#[test]
fn cleanup_proposals_works() {
	new_test_ext().execute_with(|| {
		let multisig_account = add_alice_bob_charlie_dave_multisig(2);
		let mut call_hash_vec = vec![];

		let alice_current_balance = Balances::free_balance(&ALICE);
		let n_proposals: u128 = 10;
		for i in 0..n_proposals {
			let call = construc_transfer_call(DAVE, 10 + i);
			call_hash_vec.push(Multisig::hash_of(&call.clone()));
		}

		call_hash_vec.iter().for_each(|call_hash| {
			assert_ok!(Multisig::start_proposal(
				RuntimeOrigin::signed(ALICE),
				multisig_account,
				*call_hash,
			));
		});

		call_hash_vec.iter().for_each(|call_hash| {
			assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));
		});

		// Delete account
		assert_ok!(Multisig::delete_account(RuntimeOrigin::signed(multisig_account)));

		// Still exists
		call_hash_vec.iter().for_each(|call_hash| {
			assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash));
		});

		assert_eq!(
			Balances::free_balance(&ALICE),
			// As the account is deleted, the creation deposit should be returned
			alice_current_balance - (n_proposals * ProposalDeposit::get()) + CreationDeposit::get()
		);

		assert_ok!(Multisig::cleanup_proposals(RuntimeOrigin::signed(EVE), multisig_account));

		// After cleanup, all deposits should be returned and all proposals should be removed
		assert_eq!(
			Balances::free_balance(&ALICE),
			alice_current_balance + CreationDeposit::get()
		);

		//TODO: This should only clear RemoveProposalLimit proposals at a time, It's clearing all instead.
		// From documentation it mentions that it clears the overlay completely but can't mock the behvaior to check
		// the actual implementation.
		call_hash_vec.iter().for_each(|call_hash| {
			assert!(!PendingProposals::<Test>::contains_key(&multisig_account, call_hash));
		});
		// This should only be fired after the last proposal is cleared.
		System::assert_has_event(crate::Event::PendingProposalsCleared { multisig_account }.into());
	});
}

#[test]
fn cleanup_proposals_for_non_deleted_multisig_fails() {
	new_test_ext().execute_with(|| {
		let call = construc_transfer_call(EVE, 15); // The call itself doesn't matter in this test
		let call2 = construc_transfer_call(DAVE, 15); // The call itself doesn't matter in this test

		let (multisig_account, call_hash_1) =
			start_alice_bob_charlie_dave_multisig_proposal(call.clone(), 2, ALICE);

		assert_ok!(Multisig::start_proposal(
			RuntimeOrigin::signed(ALICE),
			multisig_account,
			Multisig::hash_of(&call2.clone()),
		));

		let call_hash_2 = Multisig::hash_of(&call.clone());
		// Proposal should exist
		assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash_1));
		assert!(PendingProposals::<Test>::contains_key(&multisig_account, call_hash_2));

		assert_noop!(
			Multisig::cleanup_proposals(RuntimeOrigin::signed(EVE), multisig_account),
			Error::<Test>::MultisigStillExists
		);
	});
}

#[test]
fn multisig_of_multisig_accounts() {
	new_test_ext().execute_with(|| {
		// Create two multisig accounts with threshold as 1 for the sake of simplicity of the test
		let multisig_account = add_alice_bob_multisig(1);
		let multisig_account_2 = add_alice_bob_charlie_multisig(1);
		// Add multisig accounts as owners for the new multisig
		let mut owners = BoundedBTreeSet::new();
		owners.try_insert(multisig_account).unwrap();
		owners.try_insert(multisig_account_2).unwrap();

		let multisig_of_multisig_account = Multisig::get_multisig_account_id(&owners, now());
		assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(ALICE), owners, 2));
		assert!(MultisigAccount::<Test>::contains_key(&multisig_of_multisig_account));

		// This multisig contains 2 multisig accounts
		assert!(
			MultisigAccount::<Test>::get(&multisig_of_multisig_account)
				.unwrap()
				.owners
				.len() == 2
		);
		// Basically the rest is the same as the other tests but longer because of the heirarchical nature of the multisig accounts here.
		// 1. Alice starts a proposal (The proposal is a `StartProposal` call with the multisig1) - as threshold is 1 it will be approved
	});
}
