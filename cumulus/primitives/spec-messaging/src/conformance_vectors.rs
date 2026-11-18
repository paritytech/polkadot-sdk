// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Conformance vectors for extension proofs and requires synthesis, checked against an independent
//! model of the design's rules. They are frozen: a change is a consensus break.

use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, Encode};
use hex_literal::hex;
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives::RequiresSet;

use crate::{
	lift::{build_requires, ConsumptionRecord, Interval, LiftsBySource, RequiresLift},
	mmr::{empty_root, leaf_hash, MmrExtensionProof},
	stream::StreamId,
	streams_root::gen_stream_proof,
	test_utils::{ch, extension, frontier_at},
};

/// Stream leaf `i` is the hash of the 8-byte little-endian encoding of `i`.
fn stream_leaves(n: u64) -> Vec<Hash> {
	(0..n).map(|i| leaf_hash(&i.to_le_bytes())).collect()
}

/// The extension proof from `n1` to `n2` leaves: the identity when equal, the new peaks from the
/// empty stream, else the `mmr_lib` ancestry proof.
fn proof(all: &[Hash], n1: usize, n2: usize) -> MmrExtensionProof {
	if n1 == n2 {
		MmrExtensionProof::identity(&frontier_at(all, n1))
	} else if n1 == 0 {
		let peaks = frontier_at(all, n2).peaks().to_vec();
		MmrExtensionProof { leaf_count: n2 as u64, connecting_nodes: peaks.try_into().unwrap() }
	} else {
		extension(all, n1, n2)
	}
}

fn ack(recipient: u32) -> StreamId {
	StreamId::Ack { recipient: recipient.into(), domain: 0, num: 0 }
}

fn broadcast() -> StreamId {
	StreamId::Broadcast { domain: 0, subdomain: 0, num: 0 }
}

/// A stream's entry in the commitment tree: its `MmrRoot` at `n` leaves.
fn at(all: &[Hash], n: usize) -> Hash {
	frontier_at(all, n).root().0
}

fn interval(all: &[Hash], from: usize, to: usize) -> Interval {
	let start = if from == 0 { empty_root() } else { frontier_at(all, from).root() };
	Interval { start, end: frontier_at(all, to) }
}

/// A lift for `stream`: `advances`, then the extension from `end` to `current` leaves and the tree
/// proof under `entries`.
fn lift(
	all: &[Hash],
	entries: &BTreeMap<StreamId, Hash>,
	stream: StreamId,
	advances: Vec<MmrExtensionProof>,
	end: usize,
	current: usize,
) -> RequiresLift {
	let (_, tree_proof) = gen_stream_proof(entries, stream).unwrap();
	RequiresLift { advances, extension: proof(all, end, current), tree_proof }
}

/// The fixed two-block bundle: source 2000 with a gapless channel and a lossy ack read, source
/// 2001 with one channel.
fn bundle() -> (Vec<ConsumptionRecord>, LiftsBySource) {
	let all = stream_leaves(7);
	let (s2000, s2001) = (ParaId::from(2000), ParaId::from(2001));
	let (channel, ack, broadcast) = (ch(3000), ack(3000), broadcast());

	let block1 = ConsumptionRecord {
		entries: BTreeMap::from([
			(s2000, vec![(channel, interval(&all, 0, 3)), (ack, interval(&all, 1, 2))]),
			(s2001, vec![(channel, interval(&all, 0, 2))]),
		]),
	};
	let block2 = ConsumptionRecord {
		entries: BTreeMap::from([(
			s2000,
			vec![(channel, interval(&all, 3, 5)), (ack, interval(&all, 4, 5))],
		)]),
	};

	let tree2000 =
		BTreeMap::from([(channel, at(&all, 7)), (ack, at(&all, 6)), (broadcast, at(&all, 3))]);
	let tree2001 = BTreeMap::from([(channel, at(&all, 4)), (broadcast, at(&all, 2))]);
	let lifts = LiftsBySource::try_from(BTreeMap::from([
		(
			s2000,
			vec![
				lift(&all, &tree2000, channel, vec![], 5, 7),
				lift(&all, &tree2000, ack, vec![proof(&all, 2, 4)], 5, 6),
			],
		),
		(s2001, vec![lift(&all, &tree2001, channel, vec![], 2, 4)]),
	]))
	.unwrap();
	(vec![block1, block2], lifts)
}

#[test]
fn extension_proofs_are_frozen() {
	// Each row is `(from, to, proof bytes, MmrRoot the proof extends to)`.
	let vectors: &[(usize, usize, &[u8], [u8; 32])] = &[
		(
			0,
			1,
			&hex!(
				"040499872a65aa2984ba5a26b637366c91164f5a01248118bf0ed050aa12be08"
				"d196"
			),
			hex!("78e696ff7605271c6171cba458fe0093744f076148ae1dd581c87f21959cc360"),
		),
		(
			0,
			5,
			&hex!(
				"1408e8047ece16a16ecb92bc721428458a8075ee7de6de69733999d8ddddab95"
				"1ee62df5323f51c887ff05d7b4739e846925ab9472456e7edaafa863c9985c65"
				"b578"
			),
			hex!("54d952fcd13e906c3f05df4fc5911cb5d9407b415aff58dbb0fb12fb6a2efc82"),
		),
		(
			1,
			2,
			&hex!(
				"08045f912eae1f17b9acd74c4eb2a582b556af9575c803d0e52d9566e066ae32"
				"fae8"
			),
			hex!("d69a1e538951c288ed577b9946369e9d97f4b2fb72893376567de50446130858"),
		),
		(
			1,
			3,
			&hex!(
				"0c085f912eae1f17b9acd74c4eb2a582b556af9575c803d0e52d9566e066ae32"
				"fae8f5e8079c0f938135cc0d0f65f7c730c75649975704beb863e75a20bf418e"
				"1f6f"
			),
			hex!("d3f05cae258c627b5c4cbaa7dc6c556aa8bbbc3301b86227fe57c279c8881768"),
		),
		(
			1,
			4,
			&hex!(
				"10085f912eae1f17b9acd74c4eb2a582b556af9575c803d0e52d9566e066ae32"
				"fae8a198a002637abf68acdcda69f78aa3567e3932ca182455a42ad1d7a06b8d"
				"ec89"
			),
			hex!("b8fdada6830da1d63e1a8c3bdc405f826e5b78b7361ca6c5b30f9ef8e949b970"),
		),
		(
			3,
			4,
			&hex!(
				"1004cbe153950334c384aa324e885ed8996634f6a50bdac6bc979fd979684840"
				"272f"
			),
			hex!("b8fdada6830da1d63e1a8c3bdc405f826e5b78b7361ca6c5b30f9ef8e949b970"),
		),
		(
			3,
			7,
			&hex!(
				"1c08cbe153950334c384aa324e885ed8996634f6a50bdac6bc979fd979684840"
				"272f6d759047b258c173a1f9a1834840f79cf5c33e939df752a514198a25d46c"
				"adb5"
			),
			hex!("1123faf52fe01ced39c24881fa4a55ff6e2a1370cbf1b4f7dc6cd5d9e37e90bf"),
		),
		(
			7,
			8,
			&hex!(
				"20049ad4a27ccc8c1aaa116f1eb55a6b23e7e1fad63f2d1d4f5678cb1c5f03dc"
				"1ff1"
			),
			hex!("7361afcae447963ed67d50b6cc80ac17bccf0ec029f99c6382a146fb44fe53b6"),
		),
		(
			64,
			65,
			&hex!(
				"0501043b44cff902e5ff1e5303f67e42b7afeb0491869d8dfb161c3fc0a43746"
				"ec7b81"
			),
			hex!("d9690b5e8ff4099aa15db65fee47256c55a67a279284cf0bee7898f866dfd514"),
		),
		(
			65,
			128,
			&hex!(
				"01021809c8024c9cf153240ce170b97ee877874ef5c272f4f89e74d8bf174f91"
				"5ff3d75c243502b6125522285f1f611c77d129910b32c0eda2d8341b55a544bf"
				"500cb4b834504b256b21c2194b87d67e77f951f9dff62492d5d3f1067333c37c"
				"7d64a586fa9fd4c4491c2c7be1da6b3b61429de868a6df570fdb63871d3bb0cc"
				"51a168b2702d2035b582e456075aa8e7e43a7f47bacd4b3a8394581cdc3b6b0f"
				"3d9662afe81c540b01bd075c2e1724b077ae0d99067878c4489be4d4072eac4a"
				"e807fb"
			),
			hex!("8d1c8d874b5bbcf69a6b85355a06465a974e533582d4c33df4109492cae6c2c0"),
		),
		(
			100,
			1000,
			&hex!(
				"a10f18a1b387762a1f27ee6b5278555737a70c7bc795757d563b78e9e9e103d8"
				"4e35951e7d5e6b62537a235f3ebf62bcc6c504a61e5fe204f8737f4930a8de1e"
				"8cf9b59669259f64a342b4bac8759cb8bf95c36fc3228e1e746e79cfdfa91065"
				"fb96e76c0d998843d8014fd61b22d7a219853692c705769af40bdd7166b15a90"
				"7e97bb439f12ff807ba15ab88fddea541205c07cc5a8d2d0023ccafb076abfb5"
				"fc0e6687b66bea0cb549dae204f71cd4a268f8df5f9b62f62c8b356b70c7b54f"
				"596a8b"
			),
			hex!("ace7440c3c9d86c6c316b3df0f23de52e64508473674d732c741eb648922c087"),
		),
		(
			5,
			5,
			&hex!("1400"),
			hex!("54d952fcd13e906c3f05df4fc5911cb5d9407b415aff58dbb0fb12fb6a2efc82"),
		),
	];
	let all = stream_leaves(1000);
	for &(n1, n2, bytes, root) in vectors {
		let from = frontier_at(&all, n1);
		assert_eq!(proof(&all, n1, n2).encode(), bytes, "proof bytes for ({n1}, {n2})");
		let decoded = MmrExtensionProof::decode(&mut &bytes[..]).unwrap();
		assert_eq!(
			decoded.calculate_root(&from).unwrap().0,
			Hash::from(root),
			"root for ({n1}, {n2})"
		);
	}
}

#[test]
fn requires_synthesis_is_frozen() {
	let record1 = hex!(
		"08d00700000800b80b00000000003efd2c3528b8f163c2e2349211af73f86181"
		"b748be1b6a5eebfbadc9428643ce03000000000000000853052f72b5bf7028bf"
		"db9a413d64720b55f599c0c2089c0f5b28c35d43963560f5e8079c0f938135cc"
		"0d0f65f7c730c75649975704beb863e75a20bf418e1f6f01b80b000000000078"
		"e696ff7605271c6171cba458fe0093744f076148ae1dd581c87f21959cc36002"
		"000000000000000453052f72b5bf7028bfdb9a413d64720b55f599c0c2089c0f"
		"5b28c35d43963560d10700000400b80b00000000003efd2c3528b8f163c2e234"
		"9211af73f86181b748be1b6a5eebfbadc9428643ce0200000000000000045305"
		"2f72b5bf7028bfdb9a413d64720b55f599c0c2089c0f5b28c35d43963560"
	);
	let record2 = hex!(
		"04d00700000800b80b0000000000d3f05cae258c627b5c4cbaa7dc6c556aa8bb"
		"bc3301b86227fe57c279c8881768050000000000000008e8047ece16a16ecb92"
		"bc721428458a8075ee7de6de69733999d8ddddab951ee62df5323f51c887ff05"
		"d7b4739e846925ab9472456e7edaafa863c9985c65b57801b80b0000000000b8"
		"fdada6830da1d63e1a8c3bdc405f826e5b78b7361ca6c5b30f9ef8e949b97005"
		"0000000000000008e8047ece16a16ecb92bc721428458a8075ee7de6de697339"
		"99d8ddddab951ee62df5323f51c887ff05d7b4739e846925ab9472456e7edaaf"
		"a863c9985c65b578"
	);
	let lifts = hex!(
		"08d007000008001c081211b3b2f6e246a2ac568c9fedbfa24a4e053edf1b15d8"
		"a1658bd48524dc7602d30ed65cb900be55c1e8591d318460f841f56c918a89cc"
		"23b10dc45ab259aa8f08071c8009edc86e324fbc1ce7fca81c4e3a72f9b9d234"
		"96776361ab9de661a422b0065fe211558df8ee760208b706eab2fe860bc3f333"
		"ec4d85b3ab4a860601ca2261041004a198a002637abf68acdcda69f78aa3567e"
		"3932ca182455a42ad1d7a06b8dec8918041211b3b2f6e246a2ac568c9fedbfa2"
		"4a4e053edf1b15d8a1658bd48524dc760208075ecd2eaf6193b2a929adfe6706"
		"38b29899573c0d610c99276793f060c507ee57065fe211558df8ee760208b706"
		"eab2fe860bc3f333ec4d85b3ab4a860601ca2261d107000004001004a198a002"
		"637abf68acdcda69f78aa3567e3932ca182455a42ad1d7a06b8dec8904063547"
		"0f46982fe97730ebe4fa1a4faa34fef2633db4c38353f660758c83aa8593"
	);
	let requires = hex!(
		"08d007000058f10e8cff04a6bb9098569c262d6e5d59ff38a0d7fb21f0307092"
		"7c62d2d2d4d1070000299686898b7480b295b84816c86cce1fd3945451ef127b"
		"d6ae299242b0a1a75d"
	);

	let (records, built_lifts) = bundle();
	assert_eq!(records[0].encode(), record1);
	assert_eq!(records[1].encode(), record2);
	assert_eq!(built_lifts.encode(), lifts);

	let decoded = [
		ConsumptionRecord::decode(&mut &record1[..]).unwrap(),
		ConsumptionRecord::decode(&mut &record2[..]).unwrap(),
	];
	let lifts = LiftsBySource::decode(&mut &lifts[..]).unwrap();
	let set = build_requires(&decoded, &lifts).unwrap().unwrap();
	assert_eq!(set.encode(), requires);
	assert_eq!(RequiresSet::decode(&mut &requires[..]).unwrap(), set);
}
