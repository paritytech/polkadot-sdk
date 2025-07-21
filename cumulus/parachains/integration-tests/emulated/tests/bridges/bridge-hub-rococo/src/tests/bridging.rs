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

//! Tests related to bridging itself.

use crate::tests::*;
use bp_header_chain::HeaderChain;
use bp_parachains::RelayBlockHash;
use bp_polkadot_core::parachains::{ParaHash, ParaHead};
use bp_runtime::{BasicOperatingMode, Chain as BpChain, HashOf, HeaderOf, Parachain};
use bp_test_utils::{
	make_default_justification, prepare_parachain_heads_proof, test_header_with_root,
};

/// Submits to BH's `pallet-bridge-parachains` some header and return (bridged_block_hash,
/// bridged_state_root)
fn submit_fake_parachain_heads<
	BridgedRelay: BpChain<Hash = RelayBlockHash, Header = bp_polkadot_core::Header>,
	BridgedPara: Parachain<Hash = ParaHash>,
>(
	para_header_number: u32,
	relay_chain_header_number: u32,
) -> (HashOf<BridgedPara>, HashOf<BridgedPara>) {
	// prepare parachain proofs
	let para_state_root = ParaHash::from([para_header_number as u8; 32]);
	let bridged_para_head = ParaHead(
		test_header_with_root::<HeaderOf<BridgedPara>>(para_header_number.into(), para_state_root)
			.encode(),
	);
	let (relay_chain_state_root, para_heads_proof, parachain_heads) =
		prepare_parachain_heads_proof::<HeaderOf<BridgedPara>>(vec![(
			BridgedPara::PARACHAIN_ID,
			bridged_para_head.clone(),
		)]);

	// prepare Rococo grandpa proof
	let relay_chain_header = test_header_with_root::<HeaderOf<BridgedRelay>>(
		relay_chain_header_number.into(),
		relay_chain_state_root.into(),
	);
	let relay_chain_header_hash = relay_chain_header.hash();
	let justification = make_default_justification(&relay_chain_header);

	// prepare init bridge data
	let init_bridge_data = bp_header_chain::InitializationData {
		header: Box::new(bp_test_utils::test_header(0_u32)),
		authority_list: bp_test_utils::authority_list(),
		set_id: 1,
		operating_mode: BasicOperatingMode::Normal,
	};

	// submit head proofs as the relayer would do
	BridgeHubRococo::execute_with(|| {
		let relayer = <BridgeHubRococo as Chain>::RuntimeOrigin::signed(
			BridgeHubRococo::account_id_of(ALICE),
		);

		// initialize bridge (if not)
		if <BridgeHubRococo as BridgeHubRococoPallet>::BridgeWestendGrandpa::best_finalized()
			.is_none()
		{
			assert_ok!(
				<BridgeHubRococo as BridgeHubRococoPallet>::BridgeWestendGrandpa::initialize(
					<BridgeHubRococo as Chain>::RuntimeOrigin::root(),
					init_bridge_data
				)
			);
		}

		// grandpa (if needed)
		if <BridgeHubRococo as BridgeHubRococoPallet>::BridgeWestendGrandpa::finalized_header_state_root(relay_chain_header_hash.into()).is_none()
		{
			assert_ok!(
					<BridgeHubRococo as BridgeHubRococoPallet>::BridgeWestendGrandpa::submit_finality_proof_ex(
						relayer.clone(),
						relay_chain_header.into(),
						justification,
						1,
						true
					)
			);
		}

		// parachains
		assert_ok!(
				<BridgeHubRococo as BridgeHubRococoPallet>::BridgeWestendParachains::submit_parachain_heads_ex(
					relayer,
					(
						relay_chain_header_number,
						relay_chain_header_hash.into(),
					),
					parachain_heads,
					para_heads_proof,
					true,
				)
		);
	});

	(bridged_para_head.hash(), para_state_root)
}

fn is_parachain_header_submitted<BridgedPara: Parachain>(block_hash: ParaHash) -> bool {
	BridgeHubRococo::ext_wrapper(|| {
		<BridgeHubRococo as BridgeHubRococoPallet>::BridgeWestendParachains::parachain_head(
			BridgedPara::PARACHAIN_ID.into(),
			block_hash,
		)
		.is_some()
	})
}

#[test]
fn can_submit_ahw_and_bhw_parachain_proofs_works() {
	// Submit BridgeHubWestend
	let (bhr_para_head_hash, _) = submit_fake_parachain_heads::<
		bp_westend::Westend,
		bp_bridge_hub_westend::BridgeHubWestend,
	>(1, 1);
	assert!(is_parachain_header_submitted::<bp_bridge_hub_westend::BridgeHubWestend>(
		bhr_para_head_hash
	));

	let (ahr_para_head_hash, _) = submit_fake_parachain_heads::<
		bp_westend::Westend,
		bp_asset_hub_westend::AssetHubWestend,
	>(1, 2);
	// Submit AssetHubWestend header
	assert!(is_parachain_header_submitted::<bp_asset_hub_westend::AssetHubWestend>(
		ahr_para_head_hash
	));
}

#[test]
fn asset_hub_westend_proof_root_sync_works() {
	// check AssetHubRococo for no synced proof roots
	AssetHubRococo::execute_with(|| {
		assert!(
			<AssetHubRococo as AssetHubRococoPallet>::AssetHubWestendProofRootStore::get_root_keys(
			)
			.is_empty()
		)
	});

	// (simulate) relay AHW parachain head (triggers `pallet-bridge-parachains`'s `OnNewHead`).
	let (bridged_para_head_hash, bridged_para_state_root) = submit_fake_parachain_heads::<
		bp_westend::Westend,
		bp_asset_hub_westend::AssetHubWestend,
	>(1, 1);

	// check AssetHubWestend for synced proof root (only the AHR is set up for syncing)
	AssetHubRococo::execute_with(|| {
		assert_eq!(
			<AssetHubRococo as AssetHubRococoPallet>::AssetHubWestendProofRootStore::get_root(
				&bridged_para_head_hash,
			),
			Some(bridged_para_state_root)
		);
		assert!(
			<AssetHubRococo as AssetHubRococoPallet>::AssetHubWestendProofRootStore::get_root_keys(
			)
			.contains(&bridged_para_head_hash)
		);
	});
}
