use crate::{
	authorization_payload, mock::*, ClaimState, Claims, CreditHash, CreditsTrie, Error, Event,
	LatestRootId, RootInfo, Roots, VoucherPublic,
};
use frame_support::{assert_noop, assert_ok, BoundedVec};
use sp_core::{sr25519, Pair, H256};
use sp_runtime::proving_trie::ProvingTrie;

const ROOT_ID: u32 = 7;
const TIMESTAMP: u32 = 123_456;

struct CreditFixture {
	pair: sr25519::Pair,
	voucher: VoucherPublic,
	credit_hash: CreditHash,
	root: H256,
	proof: BoundedVec<u8, <Test as crate::Config>::MaxProofLen>,
}

fn credit_fixture(seed: u8, hash_byte: u8) -> CreditFixture {
	let pair = sr25519::Pair::from_seed(&[seed; 32]);
	let voucher = pair.public();
	let credit_hash = H256::repeat_byte(hash_byte);
	let trie =
		CreditsTrie::generate_for([((voucher, credit_hash), TIMESTAMP)]).expect("valid trie");
	let root = *trie.root();
	let proof = trie
		.create_proof(&(voucher, credit_hash))
		.expect("credit exists")
		.try_into()
		.expect("single-leaf proof is bounded");
	CreditFixture { pair, voucher, credit_hash, root, proof }
}

fn two_credit_fixtures() -> (CreditFixture, CreditFixture) {
	let first_pair = sr25519::Pair::from_seed(&[13; 32]);
	let second_pair = sr25519::Pair::from_seed(&[14; 32]);
	let first_voucher = first_pair.public();
	let second_voucher = second_pair.public();
	let first_hash = H256::repeat_byte(0xcd);
	let second_hash = H256::repeat_byte(0xde);
	let trie = CreditsTrie::generate_for([
		((first_voucher, first_hash), TIMESTAMP),
		((second_voucher, second_hash), TIMESTAMP),
	])
	.expect("valid trie");
	let root = *trie.root();
	let first_proof = trie
		.create_proof(&(first_voucher, first_hash))
		.expect("first credit exists")
		.try_into()
		.expect("two-leaf proof is bounded");
	let second_proof = trie
		.create_proof(&(second_voucher, second_hash))
		.expect("second credit exists")
		.try_into()
		.expect("two-leaf proof is bounded");
	(
		CreditFixture {
			pair: first_pair,
			voucher: first_voucher,
			credit_hash: first_hash,
			root,
			proof: first_proof,
		},
		CreditFixture {
			pair: second_pair,
			voucher: second_voucher,
			credit_hash: second_hash,
			root,
			proof: second_proof,
		},
	)
}

fn setup_collection() -> u32 {
	assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
	assert_ok!(Scarcity::define_item(RuntimeOrigin::signed(OWNER), 0, Vec::new()));
	0
}

fn setup_second_collection() -> u32 {
	assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
	assert_ok!(Scarcity::define_item(RuntimeOrigin::signed(OWNER), 1, Vec::new()));
	1
}

fn ingest(fixture: &CreditFixture) {
	assert_ok!(ScarcityClaims::ingest_root(RuntimeOrigin::root(), ROOT_ID, fixture.root, 1,));
}

fn signature(
	fixture: &CreditFixture,
	root_id: u32,
	collection: u32,
	destination: u64,
) -> sr25519::Signature {
	let payload = authorization_payload(
		&System::block_hash(0),
		root_id,
		fixture.credit_hash,
		collection,
		&destination,
	);
	fixture.pair.sign(&payload)
}

fn claim(
	fixture: &CreditFixture,
	root_id: u32,
	collection: u32,
	destination: u64,
) -> frame_support::dispatch::DispatchResultWithPostInfo {
	ScarcityClaims::claim(
		RuntimeOrigin::signed(RELAYER),
		root_id,
		fixture.voucher,
		fixture.credit_hash,
		TIMESTAMP,
		fixture.proof.clone(),
		collection,
		destination,
		signature(fixture, root_id, collection, destination),
	)
}

#[test]
fn root_ingestion_requires_authority_and_is_monotonic_and_idempotent() {
	new_test_ext().execute_with(|| {
		let fixture = credit_fixture(1, 0x11);
		assert_noop!(
			ScarcityClaims::ingest_root(RuntimeOrigin::signed(OWNER), ROOT_ID, fixture.root, 1),
			sp_runtime::DispatchError::BadOrigin
		);
		assert_noop!(
			ScarcityClaims::ingest_root(RuntimeOrigin::root(), ROOT_ID, fixture.root, 0),
			Error::<Test>::EmptyRoot
		);
		assert_noop!(
			ScarcityClaims::ingest_root(RuntimeOrigin::root(), ROOT_ID, H256::zero(), 1),
			Error::<Test>::InvalidRoot
		);

		ingest(&fixture);
		let events_before = System::events().len();
		assert_ok!(ScarcityClaims::ingest_root(RuntimeOrigin::root(), ROOT_ID, fixture.root, 1,));
		assert_eq!(System::events().len(), events_before, "idempotent delivery emits no event");
		assert_noop!(
			ScarcityClaims::ingest_root(RuntimeOrigin::root(), ROOT_ID, H256::repeat_byte(0x99), 1),
			Error::<Test>::ConflictingRoot
		);
		assert_noop!(
			ScarcityClaims::ingest_root(
				RuntimeOrigin::root(),
				ROOT_ID - 1,
				H256::repeat_byte(0x22),
				1
			),
			Error::<Test>::StaleRoot
		);
		assert_eq!(LatestRootId::<Test>::get(), Some(ROOT_ID));
	});
}

#[test]
fn valid_relayer_claim_selects_with_credit_hash_and_mints_without_deposit() {
	new_test_ext().execute_with(|| {
		let collection = setup_collection();
		let fixture = credit_fixture(2, 0x22);
		ingest(&fixture);
		let held_before = pallet_balances::Holds::<Test>::get(OWNER)
			.into_iter()
			.map(|hold| hold.amount)
			.sum::<u64>();

		assert_ok!(claim(&fixture, ROOT_ID, collection, DESTINATION));

		let nft = pallet_scarcity::NftsByOwner::<Test>::get(DESTINATION)
			.expect("destination received an NFT");
		assert_eq!(nft.collection, collection);
		assert_eq!(nft.item, 0);
		assert_eq!(
			selections(),
			vec![(OWNER, collection, fixture.credit_hash)],
			"the exact credit hash is selector entropy"
		);
		assert!(!pallet_scarcity::InstanceDeposits::<Test>::contains_key(nft.instance));
		let held_after = pallet_balances::Holds::<Test>::get(OWNER)
			.into_iter()
			.map(|hold| hold.amount)
			.sum::<u64>();
		assert_eq!(held_after, held_before, "claim mint added no storage deposit");
		assert_eq!(
			Roots::<Test>::get(ROOT_ID),
			Some(RootInfo { root: fixture.root, claim_count: 1, claimed_count: 1 })
		);
		assert_eq!(
			Claims::<Test>::get(fixture.credit_hash),
			Some(ClaimState::Claimed {
				root_id: ROOT_ID,
				collection,
				item: 0,
				instance: nft.instance,
				destination: DESTINATION,
			})
		);
		System::assert_has_event(
			Event::<Test>::Claimed {
				root_id: ROOT_ID,
				credit_hash: fixture.credit_hash,
				collection,
				item: 0,
				instance: nft.instance,
				destination: DESTINATION,
				submitter: RELAYER,
			}
			.into(),
		);
		System::assert_last_event(Event::<Test>::RootCompleted { root_id: ROOT_ID }.into());
		assert_ok!(ScarcityClaims::do_try_state());
	});
}

#[test]
fn proof_binds_voucher_credit_timestamp_root_and_leaf_count() {
	new_test_ext().execute_with(|| {
		let collection = setup_collection();
		let fixture = credit_fixture(3, 0x33);
		ingest(&fixture);

		assert_noop!(
			ScarcityClaims::claim(
				RuntimeOrigin::signed(RELAYER),
				ROOT_ID,
				fixture.voucher,
				fixture.credit_hash,
				TIMESTAMP + 1,
				fixture.proof.clone(),
				collection,
				DESTINATION,
				signature(&fixture, ROOT_ID, collection, DESTINATION),
			),
			Error::<Test>::InvalidProof
		);

		let other = credit_fixture(4, 0x44);
		assert_noop!(
			ScarcityClaims::claim(
				RuntimeOrigin::signed(RELAYER),
				ROOT_ID,
				other.voucher,
				fixture.credit_hash,
				TIMESTAMP,
				fixture.proof.clone(),
				collection,
				DESTINATION,
				signature(&fixture, ROOT_ID, collection, DESTINATION),
			),
			Error::<Test>::InvalidProof
		);

		let malformed: BoundedVec<_, <Test as crate::Config>::MaxProofLen> =
			vec![1, 2, 3].try_into().unwrap();
		assert_noop!(
			ScarcityClaims::claim(
				RuntimeOrigin::signed(RELAYER),
				ROOT_ID,
				fixture.voucher,
				fixture.credit_hash,
				TIMESTAMP,
				malformed,
				collection,
				DESTINATION,
				signature(&fixture, ROOT_ID, collection, DESTINATION),
			),
			Error::<Test>::InvalidProof
		);

		Roots::<Test>::mutate(ROOT_ID, |root| {
			root.as_mut().unwrap().claim_count = 2;
		});
		assert_noop!(
			claim(&fixture, ROOT_ID, collection, DESTINATION),
			Error::<Test>::WrongLeafCount
		);
		assert!(!Claims::<Test>::contains_key(fixture.credit_hash));
	});
}

#[test]
fn voucher_signature_binds_chain_root_credit_collection_and_destination() {
	new_test_ext().execute_with(|| {
		let collection = setup_collection();
		let fixture = credit_fixture(5, 0x55);
		ingest(&fixture);

		let wrong_destination_signature = signature(&fixture, ROOT_ID, collection, OTHER);
		assert_noop!(
			ScarcityClaims::claim(
				RuntimeOrigin::signed(RELAYER),
				ROOT_ID,
				fixture.voucher,
				fixture.credit_hash,
				TIMESTAMP,
				fixture.proof.clone(),
				collection,
				DESTINATION,
				wrong_destination_signature,
			),
			Error::<Test>::InvalidVoucherSignature
		);

		let other_pair = sr25519::Pair::from_seed(&[99; 32]);
		let payload = authorization_payload(
			&System::block_hash(0),
			ROOT_ID,
			fixture.credit_hash,
			collection,
			&DESTINATION,
		);
		assert_noop!(
			ScarcityClaims::claim(
				RuntimeOrigin::signed(OTHER),
				ROOT_ID,
				fixture.voucher,
				fixture.credit_hash,
				TIMESTAMP,
				fixture.proof.clone(),
				collection,
				DESTINATION,
				other_pair.sign(&payload),
			),
			Error::<Test>::InvalidVoucherSignature
		);
		assert!(!Claims::<Test>::contains_key(fixture.credit_hash));
	});
}

#[test]
fn credit_is_globally_single_use_across_collections_and_roots() {
	new_test_ext().execute_with(|| {
		let first = setup_collection();
		let second = setup_second_collection();
		let fixture = credit_fixture(6, 0x66);
		ingest(&fixture);
		assert_ok!(claim(&fixture, ROOT_ID, first, DESTINATION));

		assert_noop!(claim(&fixture, ROOT_ID, second, OTHER), Error::<Test>::AlreadyClaimed);
		assert_ok!(ScarcityClaims::ingest_root(
			RuntimeOrigin::root(),
			ROOT_ID + 1,
			fixture.root,
			1,
		));
		assert_noop!(claim(&fixture, ROOT_ID + 1, second, OTHER), Error::<Test>::AlreadyClaimed);
		assert!(!pallet_scarcity::NftsByOwner::<Test>::contains_key(OTHER));
	});
}

#[test]
fn selector_failure_rolls_back_provisional_claim_and_keeps_credit_retryable() {
	new_test_ext().execute_with(|| {
		let collection = setup_collection();
		let fixture = credit_fixture(7, 0x77);
		ingest(&fixture);
		set_selector_fails(true);

		assert!(claim(&fixture, ROOT_ID, collection, DESTINATION).is_err());
		assert!(!Claims::<Test>::contains_key(fixture.credit_hash));
		assert_eq!(Roots::<Test>::get(ROOT_ID).unwrap().claimed_count, 0);
		assert!(!pallet_scarcity::NftsByOwner::<Test>::contains_key(DESTINATION));

		set_selector_fails(false);
		assert_ok!(claim(&fixture, ROOT_ID, collection, DESTINATION));
	});
}

#[test]
fn reentrant_claim_from_same_root_does_not_lose_accounting() {
	new_test_ext().execute_with(|| {
		let collection = setup_collection();
		let (outer, nested) = two_credit_fixtures();
		assert_ok!(ScarcityClaims::ingest_root(RuntimeOrigin::root(), ROOT_ID, outer.root, 2,));
		set_reentrant_claim(ReentrantClaim {
			root_id: ROOT_ID,
			voucher: nested.voucher,
			credit_hash: nested.credit_hash,
			timestamp: TIMESTAMP,
			proof: nested.proof.clone(),
			collection,
			destination: OTHER,
			signature: signature(&nested, ROOT_ID, collection, OTHER),
		});

		assert_ok!(claim(&outer, ROOT_ID, collection, DESTINATION));

		assert!(pallet_scarcity::NftsByOwner::<Test>::contains_key(DESTINATION));
		assert!(pallet_scarcity::NftsByOwner::<Test>::contains_key(OTHER));
		assert!(matches!(Claims::<Test>::get(outer.credit_hash), Some(ClaimState::Claimed { .. })));
		assert!(matches!(
			Claims::<Test>::get(nested.credit_hash),
			Some(ClaimState::Claimed { .. })
		));
		assert_eq!(Roots::<Test>::get(ROOT_ID).unwrap().claimed_count, 2);
		System::assert_last_event(Event::<Test>::RootCompleted { root_id: ROOT_ID }.into());
		assert_ok!(ScarcityClaims::do_try_state());
	});
}

#[test]
fn same_credit_reentrancy_is_rejected_without_consuming_the_credit() {
	new_test_ext().execute_with(|| {
		let collection = setup_collection();
		let fixture = credit_fixture(15, 0xef);
		ingest(&fixture);
		set_reentrant_claim(ReentrantClaim {
			root_id: ROOT_ID,
			voucher: fixture.voucher,
			credit_hash: fixture.credit_hash,
			timestamp: TIMESTAMP,
			proof: fixture.proof.clone(),
			collection,
			destination: OTHER,
			signature: signature(&fixture, ROOT_ID, collection, OTHER),
		});

		assert!(claim(&fixture, ROOT_ID, collection, DESTINATION).is_err());
		assert!(!Claims::<Test>::contains_key(fixture.credit_hash));
		assert_eq!(Roots::<Test>::get(ROOT_ID).unwrap().claimed_count, 0);
		assert!(!pallet_scarcity::NftsByOwner::<Test>::contains_key(DESTINATION));
		assert!(!pallet_scarcity::NftsByOwner::<Test>::contains_key(OTHER));

		assert_ok!(claim(&fixture, ROOT_ID, collection, DESTINATION));
	});
}

#[test]
fn unknown_selected_item_rolls_back_and_can_be_retried() {
	new_test_ext().execute_with(|| {
		let collection = setup_collection();
		let fixture = credit_fixture(8, 0x88);
		ingest(&fixture);
		set_selector_item(99);

		assert_noop!(
			claim(&fixture, ROOT_ID, collection, DESTINATION),
			pallet_scarcity::Error::<Test>::UnknownItem
		);
		assert!(!Claims::<Test>::contains_key(fixture.credit_hash));
		assert_eq!(Roots::<Test>::get(ROOT_ID).unwrap().claimed_count, 0);

		set_selector_item(0);
		assert_ok!(claim(&fixture, ROOT_ID, collection, DESTINATION));
	});
}

#[test]
fn occupied_destination_does_not_consume_credit() {
	new_test_ext().execute_with(|| {
		let collection = setup_collection();
		assert_ok!(Scarcity::mint(
			RuntimeOrigin::signed(OWNER),
			collection,
			0,
			DESTINATION,
			Vec::new(),
		));
		let fixture = credit_fixture(9, 0x99);
		ingest(&fixture);

		assert_noop!(
			claim(&fixture, ROOT_ID, collection, DESTINATION),
			pallet_scarcity::Error::<Test>::AddressOccupied
		);
		assert!(!Claims::<Test>::contains_key(fixture.credit_hash));
		assert_eq!(Roots::<Test>::get(ROOT_ID).unwrap().claimed_count, 0);
	});
}

#[test]
fn collection_must_be_accepted_by_the_selector_adapter() {
	new_test_ext().execute_with(|| {
		let collection = setup_collection();
		let fixture = credit_fixture(10, 0xaa);
		ingest(&fixture);
		set_selector_owner(OTHER);

		assert!(claim(&fixture, ROOT_ID, collection, DESTINATION).is_err());
		assert!(!Claims::<Test>::contains_key(fixture.credit_hash));
		assert_eq!(Roots::<Test>::get(ROOT_ID).unwrap().claimed_count, 0);
	});
}

#[test]
fn try_state_rejects_provisional_and_miscounted_claims() {
	new_test_ext().execute_with(|| {
		let fixture = credit_fixture(11, 0xbb);
		ingest(&fixture);
		Claims::<Test>::insert(fixture.credit_hash, ClaimState::Claiming { root_id: ROOT_ID });
		assert!(ScarcityClaims::do_try_state().is_err());

		Claims::<Test>::remove(fixture.credit_hash);
		Roots::<Test>::mutate(ROOT_ID, |root| root.as_mut().unwrap().claimed_count = 1);
		assert!(ScarcityClaims::do_try_state().is_err());
	});
}

#[test]
fn try_state_rejects_invalid_root_bookkeeping() {
	new_test_ext().execute_with(|| {
		let fixture = credit_fixture(12, 0xbc);
		ingest(&fixture);

		LatestRootId::<Test>::put(ROOT_ID + 1);
		assert!(ScarcityClaims::do_try_state().is_err());

		LatestRootId::<Test>::put(ROOT_ID);
		Roots::<Test>::mutate(ROOT_ID, |root| root.as_mut().unwrap().root = H256::zero());
		assert!(ScarcityClaims::do_try_state().is_err());
	});
}
