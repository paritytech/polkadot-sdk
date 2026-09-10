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

//! Tests for `pallet_bootnodes`.

use crate::{mock::*, Bootnode, BootnodeOf, Bootnodes as BootnodeSet, Error, Event, LastSeq};
use codec::Encode;
use frame_support::{assert_noop, assert_ok, BoundedVec};
use multiaddr::Multiaddr;
use sp_core::{crypto::Pair as _, ed25519, H256};
use sp_runtime::DispatchError;

const PEER: &str = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";

fn pair(seed: u8) -> ed25519::Pair {
	ed25519::Pair::from_seed(&[seed; 32])
}

fn addr(addr: &str) -> BoundedVec<u8, <Test as crate::Config>::MaxAddrLen> {
	addr.parse::<Multiaddr>().unwrap().to_vec().try_into().unwrap()
}

/// A bootnode entry for `pair` with a valid proof of possession.
fn signed(pair: &ed25519::Pair, seq: u64, address: &str) -> BootnodeOf<Test> {
	let addr = addr(address);
	let sig = pair.sign(&Bootnodes::pop_payload(seq, &addr));
	Bootnode { addr, node_key: pair.public(), seq, sig }
}

fn node_keys() -> Vec<ed25519::Public> {
	BootnodeSet::<Test>::get().iter().map(|b| b.node_key).collect()
}

#[test]
fn pop_payload_has_the_documented_layout() {
	let genesis = H256::repeat_byte(0x4a);
	let address = addr("/ip4/1.2.3.4/tcp/30333");
	let payload = crate::pop_payload(&genesis, 2_000, 7, &address);

	let mut expected = b"bootnode-pop-v1".to_vec();
	expected.extend_from_slice(genesis.as_bytes());
	expected.extend_from_slice(&2_000u32.to_le_bytes());
	expected.extend_from_slice(&7u64.to_le_bytes());
	expected.extend(address.to_vec().encode());
	assert_eq!(payload, expected);

	// The pallet binds its own constants.
	assert_eq!(Bootnodes::pop_payload(7, &address), payload);
}

#[test]
fn add_bootnode_works() {
	new_test_ext().execute_with(|| {
		let bootnode = signed(&pair(1), 0, "/ip4/1.2.3.4/tcp/30333");
		assert_ok!(Bootnodes::add_bootnode(RuntimeOrigin::root(), bootnode.clone()));

		assert_eq!(BootnodeSet::<Test>::get().to_vec(), vec![bootnode.clone()]);
		assert_eq!(LastSeq::<Test>::get(bootnode.node_key), Some(0));
		System::assert_last_event(
			Event::BootnodeAdded { node_key: bootnode.node_key, seq: 0 }.into(),
		);
		assert_ok!(Bootnodes::do_try_state());
	});
}

#[test]
fn add_bootnode_requires_admin_origin() {
	new_test_ext().execute_with(|| {
		let bootnode = signed(&pair(1), 0, "/ip4/1.2.3.4/tcp/30333");
		assert_noop!(
			Bootnodes::add_bootnode(RuntimeOrigin::signed(1), bootnode.clone()),
			DispatchError::BadOrigin
		);
		assert_noop!(
			Bootnodes::add_bootnode(RuntimeOrigin::none(), bootnode),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn add_bootnode_rejects_invalid_multiaddr() {
	new_test_ext().execute_with(|| {
		let pair = pair(1);
		for bad in [
			vec![],
			vec![4, 1, 2, 3],
			vec![0xff, 0xff, 0xff, 0xff, 0x7f],
			vec![0x7f, 0],
			addr("/ip4/1.2.3.4/udp/30333/quic-v1").to_vec(),
			addr("/ip4/1.2.3.4").to_vec(),
		] {
			let addr: BoundedVec<u8, _> = bad.try_into().unwrap();
			let sig = pair.sign(&Bootnodes::pop_payload(0, &addr));
			let bootnode = Bootnode { addr, node_key: pair.public(), seq: 0, sig };
			assert_noop!(
				Bootnodes::add_bootnode(RuntimeOrigin::root(), bootnode),
				Error::<Test>::InvalidMultiaddr
			);
		}
	});
}

#[test]
fn add_bootnode_rejects_multiaddr_with_peer_id() {
	new_test_ext().execute_with(|| {
		let bootnode = signed(&pair(1), 0, &format!("/ip4/1.2.3.4/tcp/30333/p2p/{PEER}"));
		assert_noop!(
			Bootnodes::add_bootnode(RuntimeOrigin::root(), bootnode),
			Error::<Test>::MultiaddrContainsPeerId
		);
	});
}

#[test]
fn add_bootnode_rejects_bad_proof_of_possession() {
	new_test_ext().execute_with(|| {
		let good = signed(&pair(1), 3, "/ip4/1.2.3.4/tcp/30333");

		// Signed by another key.
		let mut bad = good.clone();
		bad.node_key = pair(2).public();
		assert_noop!(
			Bootnodes::add_bootnode(RuntimeOrigin::root(), bad),
			Error::<Test>::InvalidSignature
		);

		// Signed over a different seq.
		let mut bad = good.clone();
		bad.seq = 4;
		assert_noop!(
			Bootnodes::add_bootnode(RuntimeOrigin::root(), bad),
			Error::<Test>::InvalidSignature
		);

		// Signed over a different address.
		let mut bad = good.clone();
		bad.addr = addr("/ip4/1.2.3.5/tcp/30333");
		assert_noop!(
			Bootnodes::add_bootnode(RuntimeOrigin::root(), bad),
			Error::<Test>::InvalidSignature
		);

		// Signed for another para.
		let mut bad = good.clone();
		bad.sig =
			pair(1).sign(&crate::pop_payload(&JamGenesisHash::get(), PARA_ID + 1, 3, &bad.addr));
		assert_noop!(
			Bootnodes::add_bootnode(RuntimeOrigin::root(), bad),
			Error::<Test>::InvalidSignature
		);

		// Signed for another JAM chain.
		let mut bad = good.clone();
		bad.sig = pair(1).sign(&crate::pop_payload(&H256::zero(), PARA_ID, 3, &bad.addr));
		assert_noop!(
			Bootnodes::add_bootnode(RuntimeOrigin::root(), bad),
			Error::<Test>::InvalidSignature
		);

		// Signed without the domain separator.
		let mut bad = good.clone();
		bad.sig = pair(1).sign(&(JamGenesisHash::get(), PARA_ID, 3u64, bad.addr.to_vec()).encode());
		assert_noop!(
			Bootnodes::add_bootnode(RuntimeOrigin::root(), bad),
			Error::<Test>::InvalidSignature
		);

		assert_ok!(Bootnodes::add_bootnode(RuntimeOrigin::root(), good));
	});
}

#[test]
fn seq_must_exceed_last_accepted() {
	new_test_ext().execute_with(|| {
		let pair = pair(1);
		assert_ok!(Bootnodes::add_bootnode(
			RuntimeOrigin::root(),
			signed(&pair, 5, "/ip4/1.2.3.4/tcp/30333")
		));

		for seq in [5, 4, 0] {
			assert_noop!(
				Bootnodes::add_bootnode(
					RuntimeOrigin::root(),
					signed(&pair, seq, "/ip4/1.2.3.5/tcp/30333")
				),
				Error::<Test>::StaleSeq
			);
		}
		assert_eq!(BootnodeSet::<Test>::get()[0].addr, addr("/ip4/1.2.3.4/tcp/30333"));
	});
}

#[test]
fn seq_u64_max_is_rejected_so_the_key_stays_updatable() {
	new_test_ext().execute_with(|| {
		let pair = pair(1);
		assert_noop!(
			Bootnodes::add_bootnode(
				RuntimeOrigin::root(),
				signed(&pair, u64::MAX, "/ip4/1.2.3.4/tcp/30333")
			),
			Error::<Test>::SeqTooLarge
		);
		assert_eq!(LastSeq::<Test>::get(pair.public()), None);
		assert_ok!(Bootnodes::add_bootnode(
			RuntimeOrigin::root(),
			signed(&pair, u64::MAX - 1, "/ip4/1.2.3.4/tcp/30333")
		));
	});
}

#[test]
fn higher_seq_updates_entry_in_place() {
	new_test_ext().execute_with(|| {
		let (a, b, c) = (pair(1), pair(2), pair(3));
		for (p, port) in [(&a, 1), (&b, 2), (&c, 3)] {
			assert_ok!(Bootnodes::add_bootnode(
				RuntimeOrigin::root(),
				signed(p, 0, &format!("/ip4/1.2.3.4/tcp/{port}"))
			));
		}

		let updated = signed(&b, 1, "/dns/boot.example.org/tcp/30333/ws");
		assert_ok!(Bootnodes::add_bootnode(RuntimeOrigin::root(), updated.clone()));

		let set = BootnodeSet::<Test>::get();
		assert_eq!(set.len(), 3);
		assert_eq!(node_keys(), vec![a.public(), b.public(), c.public()]);
		assert_eq!(set[1], updated);
		assert_eq!(LastSeq::<Test>::get(b.public()), Some(1));
		System::assert_last_event(Event::BootnodeUpdated { node_key: b.public(), seq: 1 }.into());
		assert_ok!(Bootnodes::do_try_state());
	});
}

#[test]
fn set_is_bounded_by_max_bootnodes() {
	new_test_ext().execute_with(|| {
		for i in 0..MAX_BOOTNODES as u8 {
			assert_ok!(Bootnodes::add_bootnode(
				RuntimeOrigin::root(),
				signed(&pair(i + 1), 0, &format!("/ip4/1.2.3.4/tcp/{}", 30_000 + i as u16))
			));
		}
		assert_eq!(BootnodeSet::<Test>::get().len(), MAX_BOOTNODES as usize);

		// A new key does not fit and leaves no trace.
		let extra = signed(&pair(200), 0, "/ip4/1.2.3.4/tcp/40000");
		assert_noop!(
			Bootnodes::add_bootnode(RuntimeOrigin::root(), extra.clone()),
			Error::<Test>::TooManyBootnodes
		);
		assert_eq!(LastSeq::<Test>::get(extra.node_key), None);

		// A listed key can still be updated.
		assert_ok!(Bootnodes::add_bootnode(
			RuntimeOrigin::root(),
			signed(&pair(1), 1, "/ip4/1.2.3.4/tcp/40000")
		));
		assert_eq!(BootnodeSet::<Test>::get().len(), MAX_BOOTNODES as usize);
	});
}

#[test]
fn remove_bootnode_works() {
	new_test_ext().execute_with(|| {
		let (a, b) = (pair(1), pair(2));
		assert_ok!(Bootnodes::add_bootnode(
			RuntimeOrigin::root(),
			signed(&a, 0, "/ip4/1.2.3.4/tcp/1")
		));
		assert_ok!(Bootnodes::add_bootnode(
			RuntimeOrigin::root(),
			signed(&b, 0, "/ip4/1.2.3.4/tcp/2")
		));

		assert_ok!(Bootnodes::remove_bootnode(RuntimeOrigin::root(), a.public()));

		assert_eq!(node_keys(), vec![b.public()]);
		// The last accepted seq survives removal.
		assert_eq!(LastSeq::<Test>::get(a.public()), Some(0));
		System::assert_last_event(Event::BootnodeRemoved { node_key: a.public() }.into());
		assert_ok!(Bootnodes::do_try_state());
	});
}

#[test]
fn remove_bootnode_requires_admin_origin_and_listed_key() {
	new_test_ext().execute_with(|| {
		let a = pair(1);
		assert_ok!(Bootnodes::add_bootnode(
			RuntimeOrigin::root(),
			signed(&a, 0, "/ip4/1.2.3.4/tcp/1")
		));

		assert_noop!(
			Bootnodes::remove_bootnode(RuntimeOrigin::signed(1), a.public()),
			DispatchError::BadOrigin
		);
		assert_noop!(
			Bootnodes::remove_bootnode(RuntimeOrigin::root(), pair(2).public()),
			Error::<Test>::UnknownBootnode
		);
		assert_eq!(node_keys(), vec![a.public()]);
	});
}

#[test]
fn removed_node_cannot_be_relisted_with_old_signature() {
	new_test_ext().execute_with(|| {
		let a = pair(1);
		let original = signed(&a, 3, "/ip4/1.2.3.4/tcp/1");
		assert_ok!(Bootnodes::add_bootnode(RuntimeOrigin::root(), original.clone()));
		assert_ok!(Bootnodes::remove_bootnode(RuntimeOrigin::root(), a.public()));

		assert_noop!(
			Bootnodes::add_bootnode(RuntimeOrigin::root(), original),
			Error::<Test>::StaleSeq
		);
		assert_ok!(Bootnodes::add_bootnode(
			RuntimeOrigin::root(),
			signed(&a, 4, "/ip4/1.2.3.4/tcp/1")
		));
		assert_eq!(node_keys(), vec![a.public()]);
		assert_eq!(LastSeq::<Test>::get(a.public()), Some(4));
	});
}

#[test]
fn try_state_detects_broken_invariants() {
	new_test_ext().execute_with(|| {
		let a = pair(1);
		assert_ok!(Bootnodes::add_bootnode(
			RuntimeOrigin::root(),
			signed(&a, 2, "/ip4/1.2.3.4/tcp/1")
		));
		assert_ok!(Bootnodes::do_try_state());

		LastSeq::<Test>::insert(a.public(), 3);
		assert!(Bootnodes::do_try_state().is_err());
		LastSeq::<Test>::insert(a.public(), 2);

		let entry = BootnodeSet::<Test>::get()[0].clone();
		BootnodeSet::<Test>::mutate(|set| set.try_push(entry).unwrap());
		assert!(Bootnodes::do_try_state().is_err());
	});
}
