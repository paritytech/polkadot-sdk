// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! Utilities for handling proofs from the stalled AssetHub chain.

use crate::{dday::DDayVotingInstance, DDayProofRootStore, Runtime};
use bp_runtime::{RawStorageProof, StorageProofChecker};
use codec::{Decode, Encode};
use core::ops::ControlFlow;
use cumulus_primitives_core::relay_chain::{
	BlockNumber as RelayChainBlockNumber, Hash as RelayChainHash,
};
use cumulus_primitives_core::Weight;
use frame_support::{
	ensure,
	storage::storage_prefix,
	traits::{Contains, ProcessMessageError},
	Blake2_128Concat, StorageHasher, Twox64Concat,
};
use pallet_dday_voting::{
	ProofAccountIdOf, ProofBalanceOf, ProofBlockNumberOf, ProofDescription, ProofHashOf,
	ProofHasherOf, ProofInterface, ProofOf, ProofRoot, TotalForTallyProvider, Totals, VotingPower,
};
use polkadot_parachain_primitives::primitives::{HeadData, Id as ParaId};
use sp_runtime::traits::BlakeTwo256;
use westend_runtime_constants::system_parachain::ASSET_HUB_ID;
use xcm::{latest::prelude::*, DoubleEncoded};
use xcm_builder::{CreateMatcher, MatchXcm};
use xcm_executor::traits::{Properties, ShouldExecute};

/// Required description of AssetHub chain (hash, balance, accountId).
pub struct AssetHubProofDescription;
impl ProofDescription for AssetHubProofDescription {
	type BlockNumber = parachains_common::BlockNumber;
	type Hasher = BlakeTwo256;
	type AccountId = parachains_common::AccountId;
	type Balance = parachains_common::Balance;
	/// Multi-key proofs are supported.
	type Proof = RawStorageProof;
}

/// Account data representation on AssetHub.
type AssetHubAccountData = frame_system::AccountInfo<
	parachains_common::Nonce,
	pallet_balances::AccountData<ProofBalanceOf<AssetHubProver>>,
>;

// /// Implementation of `TotalForTallyProvider` which return recorded/proved issuance that is
// /// relevant. If nothing is recorded yet, the `ProofBalanceOf<AssetHubAccountProver>::MAX` is used.
// impl<Chain: IsStalled> TotalForTallyProvider for StalledAssetHubDataProvider<Chain> {
// 	type TotalKey = ProofBlockNumberOf<AssetHubAccountProver>;
// 	type Total = ProofBalanceOf<AssetHubAccountProver>;
//
// 	fn total_from(totals: &Totals<Self::TotalKey, Self::Total>) -> Self::Total {
// 		Chain::stalled_head()
// 			.and_then(|head| {
// 				parachains_common::Header::decode(&mut &head.0[..])
// 					.ok()
// 					.map(|header| header.number)
// 			})
// 			.and_then(|stalled_block_number| {
// 				// get from recorded ones.
// 				totals.0.get(&stalled_block_number).map(|b| *b)
// 			})
// 			.unwrap_or(ProofBalanceOf::<AssetHubAccountProver>::MAX)
// 	}
// }

pub struct AssetHubProver;
impl AssetHubProver {
	/// Generate a proof key of account balance data.
	fn account_balance_storage_key(account: &ProofAccountIdOf<Self>) -> alloc::vec::Vec<u8> {
		let mut key = storage_prefix(b"System", b"Account").to_vec();
		account.using_encoded(|p| {
			key.extend(Blake2_128Concat::hash(p));
		});
		key
	}

	/// Proof key of total issuance.
	fn total_issuance_storage_key() -> alloc::vec::Vec<u8> {
		storage_prefix(b"Balances", b"TotalIssuance").to_vec()
	}

	/// Proof key of inactive issuance.
	fn inactive_issuance_storage_key() -> alloc::vec::Vec<u8> {
		storage_prefix(b"Balances", b"InactiveIssuance").to_vec()
	}

	/// Proof key of the relay chain `Paras::Heads`.
	fn paras_heads_storage_key(para_id: ParaId) -> alloc::vec::Vec<u8> {
		let mut key = storage_prefix(b"Paras", b"Heads").to_vec();
		para_id.using_encoded(|p| {
			key.extend(Twox64Concat::hash(p));
		});
		key
	}
}

const LOG_TARGET: &'static str = "runtime::dday";

impl ProofInterface for AssetHubProver {
	type RemoteProof = AssetHubProofDescription;
	/// For `submit_proof_root_for_voting` we expect to submit:
	/// * RelayChainBlockNumber - `DDayProofRootStore` stores mapping to the Relay Chain storage root as `RelayChainBlockNumber<>RelayChainHash`.
	/// * ProofBlockNumberOf - AssetHub block number, whose state_root we want to use for voting.
	/// * RawStorageProof - the Relay Chain proof about `Paras::Heads::get(AssetHubParaId)`, where we get AssetHub's state_root.
	type RemoteProofRootInput = (RelayChainBlockNumber, ProofBlockNumberOf<Self>, RawStorageProof);
	type RemoteProofRootOutput = ProofRoot<Runtime, DDayVotingInstance>;

	fn query_voting_power_for(
		who: &ProofAccountIdOf<Self>,
		hash: ProofHashOf<Self>,
		proof: ProofOf<Self>,
	) -> Option<VotingPower<ProofBalanceOf<Self>>> {
		// Init proof checker.
		let mut proof_checker = StorageProofChecker::<ProofHasherOf<Self>>::new(hash, proof)
			.inspect_err(|error| tracing::error!(target: LOG_TARGET, ?error, "Invalid hash/proof"))
			.ok()?;

		// Read account balance.
		let account_balance: AssetHubAccountData = proof_checker
			.read_and_decode_mandatory_value(&Self::account_balance_storage_key(who))
			.inspect_err(
				|error| tracing::error!(target: LOG_TARGET, ?error, "Invalid proof value for account balance"),
			)
			.ok()?;

		// Read total issuance.
		let total_issuance: ProofBalanceOf<Self> = proof_checker
			.read_and_decode_mandatory_value(&Self::total_issuance_storage_key())
			.inspect_err(
				|error| tracing::error!(target: LOG_TARGET, ?error, "Invalid proof value for total issuance"),
			)
			.ok()?;

		// Read inactive issuance.
		let inactive_issuance: ProofBalanceOf<Self> = proof_checker
            .read_and_decode_mandatory_value(&Self::inactive_issuance_storage_key())
            .inspect_err(|error| {
                tracing::error!(target: LOG_TARGET, ?error, "Invalid proof value for inactive issuance")
            })
            .ok()?;

		// check no unsed node in the proof
		proof_checker
			.ensure_no_unused_nodes()
			.inspect_err(
				|error| tracing::error!(target: LOG_TARGET, ?error, "Invalid proof contains unused keys"),
			)
			.ok()?;

		// Calculate active issuance as AssetHub's Balances.
		let active_issuance = total_issuance.saturating_sub(inactive_issuance);

		// return proved/parsed/valid data
		Some(VotingPower { account_power: account_balance.data.total(), total: active_issuance })
	}

	fn verify_proof_root(input: Self::RemoteProofRootInput) -> Option<Self::RemoteProofRootOutput> {
		let (relay_block_number, asset_hub_block_number, relay_proof) = input;

		// Find the relay chain storage root for relay_block_number.
		let relay_chain_state_root = DDayProofRootStore::get_root(&relay_block_number)
            .inspect(|stored| {
                tracing::error!(target: LOG_TARGET, ?relay_block_number, "Missing the relay chain storage root for the given block number!")
            })?;

		// Init proof checker for the relay chain (BlakeTwo256).
		let mut proof_checker =
			StorageProofChecker::<BlakeTwo256>::new(relay_chain_state_root, relay_proof)
				.inspect_err(
					|error| tracing::error!(target: LOG_TARGET, ?error, "Invalid hash/proof"),
				)
				.ok()?;

		// Read total issuance.
		let asset_hub_head_data: HeadData = proof_checker
            .read_and_decode_mandatory_value(&Self::paras_heads_storage_key(ASSET_HUB_ID.into()))
            .inspect_err(|error| {
                tracing::error!(target: LOG_TARGET, ?error, for_para = ASSET_HUB_ID, "Missing value `Paras::Heads`!")
            })
            .ok()?;

		let asset_hub_head_data: parachains_common::Header =
			Decode::decode(&mut &asset_hub_head_data.0[..])
				.inspect_err(
					|error| tracing::error!(target: LOG_TARGET, ?error, "Decode asset_hub_head_data failed!"),
				)
				.ok()?;

		if asset_hub_block_number == asset_hub_head_data.number {
			Some(ProofRoot {
				at_block: asset_hub_block_number,
				proof_root: asset_hub_head_data.state_root,
			})
		} else {
			tracing::warn!(target: LOG_TARGET, ?asset_hub_block_number, expected = asset_hub_head_data.number, "Invalid AssetHub block number!");
			None
		}
	}
}

impl TotalForTallyProvider for AssetHubProver {
	type TotalKey = ProofBlockNumberOf<AssetHubProver>;
	type Total = ProofBalanceOf<AssetHubProver>;

	fn total_from(totals: &Totals<Self::TotalKey, Self::Total>) -> Self::Total {
		todo!("ProofBalanceOf::<AssetHubProver>::MAX")
	}
}

#[cfg(any(test, feature = "std"))]
pub mod tests {
	use super::{ProofRoot, RawStorageProof, RelayChainBlockNumber, RelayChainHash};

	/// Sample proof downloaded from AssetHubWestend:
	///
	/// For account 5HVxofJkZcPs1emaJMWiJqd5aoWfDWobP7RiKBbbNTEDp5yy at block: (10_990_425)
	/// 0xbad834d093eae042d175d304b1850c37c63e386e9f315b81a46af4867a78625c: {
	///   parentHash: 0x347c2e995b0bbd470f083077529a9f8fcc8b18287195cd778ba1023b7fcc1fb3
	///   number: 10,990,425
	///   stateRoot: 0xb61ad16ff3226be01a583fdb83daad568f690d540bf78770bca841c6099fce8a
	///   extrinsicsRoot: 0x32bb8ded17ded4b9b73d6efef7fd7e533a647fa868e60e2dff2d288a7300456f
	///   digest: {
	///     logs: [
	///       {
	///         PreRuntime: [
	///           aura
	///           0xa74c4a1100000000
	///         ]
	///       }
	///       {
	///         Consensus: [
	///           RPSR
	///           0xeff55cbcba39a6fb903bf960fc0036e731b439dd4275d4816ceeb386f1932fbf92bdf005
	///         ]
	///       }
	///       {
	///        Seal: [
	///           aura
	///           0xaac09b8f89fb7b22a3a03a4dfb75485bec4bc1fd31db5f3afaf20157642d5738d74aadc81893e2231ef4eb2ba98febfc4e2e94984ae9da87604b0bb656d6138e
	///         ]
	///       }
	///     ]
	///   }
	/// }
	///
	/// Balances.total_issuance: 88,831,707,570,053,009 at block:
	/// 0xbad834d093eae042d175d304b1850c37c63e386e9f315b81a46af4867a78625c:
	/// 	key: 0xc2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80
	///
	/// Balances.total_issuance: 0 at block:
	/// 0xbad834d093eae042d175d304b1850c37c63e386e9f315b81a46af4867a78625c:
	/// 	key: 0xc2261276cc9d1f8598ea4b6a74b15c2f1ccde6872881f893a21de93dfe970cd5
	///
	/// System.account(5HVxofJkZcPs1emaJMWiJqd5aoWfDWobP7RiKBbbNTEDp5yy) - free balance at block:
	/// 0xbad834d093eae042d175d304b1850c37c63e386e9f315b81a46af4867a78625c:
	/// 	key: 0x26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da9942ba479328ac3d026b2c4cced7e2508f0709f5078496e42469e70752483d4f820702bb01e335d9e03bad0b54f729251
	/// {
	/// 	nonce: 5,
	/// 	consumers: 0,
	/// 	providers: 1,
	/// 	sufficients: 0,
	/// 	data: {
	/// 		free: 99,987,783,034,636,
	/// 		reserved: 0,
	/// 		frozen: 0,
	/// 		flags: 0
	/// }
	///
	///
	/// Proof generated by RPC call `state.getReadProof(key, at)`::
	/// 	key1: 0xc2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80
	/// 	key2: 0xc2261276cc9d1f8598ea4b6a74b15c2f1ccde6872881f893a21de93dfe970cd5
	/// 	key3: 0x26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da9942ba479328ac3d026b2c4cced7e2508f0709f5078496e42469e70752483d4f820702bb01e335d9e03bad0b54f729251
	/// 	at: 0xbad834d093eae042d175d304b1850c37c63e386e9f315b81a46af4867a78625c
	///
	/// {
	///   at: 0xbad834d093eae042d175d304b1850c37c63e386e9f315b81a46af4867a78625c
	///   proof: [
	///     0x5f07c875e4cff74148e4628f264b974c8040911f81a6f7973b010000000000000000
	///     0x5f0ccde6872881f893a21de93dfe970cd54000000000000000000000000000000000
	///     0x7f1c0479328ac3d026b2c4cced7e2508f0709f5078496e42469e70752483d4f820702bb01e335d9e03bad0b54f7292514101050000000000000001000000000000000c274a38f05a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
	///     0x80046080e9bdbd6c203a096900e1dd2f2d47d45946a067e4cd551196eab26d36051ecba380cc9b5db56b13ff27c4d6e0095813bd91f530b17f2f2a910031c53a1b383bd6218016acd44501b41bb36fd22a8909b61f9d7bb7f45cb6130438299eec917f1b939f
	///     0x80c4f580767b3186e7d27b3ceaa7e246378683297760c8f42153788d52809d434d312a4980fc5adff30326ec3f500b34717369d33df240cbd92c963293ab82ba729e356a3b80a339a34e79ef509d9b58915488cb1f77e1cd1c0a5b04c19005de49d0095767ab80c8d419ae09dd82730fb6268c2bf550275b4fd049df142cbd2f15d7f683e140c78012c401e3cc61c9737c3e885d04dbd0b5fd621dc799a04b13c7a6237b8aed95e180dc012813ac654f8f5969cb4f574789cfd3ce9ae3499f56054030e2c45b5c2e5580eb552b0c46b7bb06f1dac22994130f43808b7e5f7da2de5093e677c52ab6819b805f55510441212c67f0a7104c4e184a8e6e3147a318f4c4d30fbd1a52ac3abe6b80c78706c96a78fd90cc65f656254b03b65f4172983c44f323b676d4087d898aa2
	///     0x80fff9807fa50eeb70098a437a761aeb66827da80cd8207d35c2d7de6758ce5c3653b326802bd1003353af06e985b5f5575a1031fb9aef8e56448e824a9d636bd6121c1792803329646206fb88c84de424a6873feec85f582dafa4611cc9455c30b5fae7dc5580c46863f8caedf652f2e017a8ae1b1759c168f61b9a796707c6555303bcbad9a180c856363121a92532705af64424520359bca2e0553bef10ad9e0d46f0337865ab80e3939781e5587eb15d24b1003b0cddc779df202f6501a5af2fb945f7bc2efbbe80de0e104d48fdc81c1d8ae11fd7782e2e0633f9dd9c1084d39815baaf321307b58094ecfe19be5055ae905d7c5289849ec30cfc244fa4f0ea125c78781be31308ae80a2b681af74242c25cf4ccb90882422d594627ab8eea6edde4e17c6cd4a1b547880ecacdf555569b8fb9c07c88a53a36c20b14cc43e17932c6c3285ae43a804e4b080759522955003da04b57df9a6b207187b27f8094c2d539d4aeb89e2699baa0fc98085ba31b59e40c53c4a00a0c0fbe984b967dbe21550d448db14547f644946cfeb808c98be96ecc63f5feebcfa9845ffdb0bb55a03c22963a6e22f154fd3a699c9aa80a4106285a651e841f3c87183aef50c2539137065e0474e4b76b4106225c13e3b
	///     0x80ffff8053802bea8e47ef2040edd0f45b3c204cb5d5d73519bf2133e3dec00148b8ba8480578efb66a190e9e624a7637470b436fcc6f0c8453dcfe55372db376ccb5cd4b6804921c66d181ef25fb3a9e7cbdf4675280e821885a31d0fdaed79534d6177b03480134b80cf3fca82e257cb190d49041189ebdb345b4ab2979bcde6ee98ad52a5c48025379f5495345ad8d1d4a34b0dbedb9f59608727f01171c696820cc6c8532127807709b44df764dd0f3118b2f6db558920ce488239eb801bef5e21e95f74cb519880fd2af833d762aabedb8eb6f05656cdddcb272ab7bc2bfcd3215808d7d4828ef8800ef6910150ae83571061d5c6711a738926d2e8e5a71d6a56533f7d817cca60e380cc6d7ef4acd1a2c0a9470e55cf171bb5412228749886975050650961be31946e80d50130ca987628db123c56681d3f294a45588c74de0b9b85fda64fdcd47238ed80c1d09e3bbf4449304cbca82f8855286e5b49cbda5359db412610cdf68030d64880aeb7dfbacb5887d8f7c8d6091cfe6bf4805e59882bf83dbf3dcd0306c8c8764c80d5202a65c8f8570e018ccac3c60e66931c0053c0ce7250866d7dd725c57454d980477b084c7136706560ba6656945db44645480472536204ad65732485b51327e48056dc6980c6f62bc8095421eb4c8fcf1c73b6f96fb1f333235007e0f58f3b3704806f76ba93efa7a569fda9e9a277ccd288bb0bb20994f01387b232f7ef0f498ac5
	///     0x80ffff805497739bc143001f5b10bbbd385b2ce85f41aa3ff1e710e7738e719efdad75c880013392b637984ab2420d9f507a0ff03c03538918fa9e62e65525cea2143f8f638011a5a0ec4abb9738f8b6a338e7966f00348f928f674447a3fae4a157dfefb4098090e90d6f0b263c2af78d419cd918a954859078179341cbb22fe8de97238a526680068ed7ff383b08cb3f2c13226ce1a21e2cadc84f4877495ce87d3f37ec8dd8c080fa06a4717c79f9622339ee7b2d1e1578bad94aa6815ce63dd15f8bb45b42ec3580945d2e4223217dce5c7a71ea5d9cebc4dafa0aec86cd902d56f51c4e69a5f6f1807529efa8b50423e001ca59ec028c95fc2b97d70c2705193032522d328b13453e80aebc312d86ab860aa04b6511c476467347606176e3bd914b6c1652278e57277080335d735a801bbcb7a3d0a7ef4d8603b7377dc5054f41eef701dadf1bae3846a280daa969029586206915425633d0c4ef7870a72a7347cd24a03d6d540b11305e62807d9ffb29edd828b1ebf4fbecc78eddef1dabb615cc8d5aca057b48267ac57a7f806f1044bbf67024d28a3788a588fd8eb596690d5119613afa325edd2538d35649805146b095169ce4ad2e6b9c09fc8821cd37a5e1cade300ad1c5a64dfed60a191880d80fe2431e18e7bde67d80de4eb041c4815979d739a72bbc8e7c004240364aa880e29e0faf7da715ba5898a039d4d6a4213f69fdb9860776bab1a21fd2cb0ec1f3
	///     0x80ffff80881e1bd89e48f3b556153687c69f7653c418235d9eb5a3b5c99de51735ca1ea180c143584b9fd2807e70e31a7d7baf898c2a67baf8f7e0cdd47641c6b5e469387880e2158826bdcd01c0f91553067aca089657485f30861d1eaa6263b564cd064fda80e49f32a8aab93d0c7b2463200979f08ae9f1def992662f14232457c556bf633a805f3b1b5930656cda7f039d95404959abf09c86aab1b9101c06789c4ffdfb11ec8080af541b5bf4c515f11757d0acda25c356571dac625228f64ab94d9fc0e575188071ebd553608b79ff52d6e8108254797b0ae85a861f1301743fa757b5489c9c488061a47891c8e383000001b518cc263caf8ad0ab31dda03d3955fe7e13fab11cfb80867e7deec5dae9c71dfbb3b9b7b23915f67facb5b39cd34a9048b5c921a205e48042e515d0900f7927b7a23c66274c323367faac300785ea2ef21912decfa517ca808199d0ea11be6dd7e9e84d3613adde930099aed1a39bf22c452d680f8f07806280494be8fd480fce05e24ab9fe6f142fff9ae96868ee1bb9f8b00409d641346f0b8089e50948ec86f9c1ed08ab17d71f030ee90598fd7b10b34c4f605671e0e19fac809b6e6dd45fc385fee9546463a330d2259cdf35d50e6470acc1e56405c658be7880171ec0a517fcbe64e845650d6e8c31f67c47cb24adadad1c2697a4e40a737f3980c23c2190b9997a331e6626ee0d1727dd8253c0eaa2180250d1ed5b24353f2f1f
	///     0x8106800480c1756f24b4a1226ff188ea7c1dde613044f050aa3913d0677d17b7c5d45dcfdf80991cecd1b45ba5f7b3a6933aa9ee1dd480495184ffb7fde29c8dc0a58ff10f16
	///     0x9d0a394eea5630e07c48ae0c9558cef7398f80bcda578efc67e66167154b78a937b0490fb9dc5bd82914918f8d7f0328a000f180e54a8aaef753bd15a917ffbf7990cfc3a4732e120ea716b4c2ab5c8f382471b7505f0e7b9012096b41c4eb3aaf947f6ea4290800004c5f0684a022a34dd8bfa2baaf44f172b710040180cb6c48716e3157314974172b93a043786dd4d66164e657e8b8c8e5fb41b61cc680aa1ef966dd7c07a56b7f0d348f269d628c5277af2533b3b66cd4c9136e17559c80408a52637ef2f653eee1412458abbaa1f2099f7c425b7302e8d7b866eb25916780150712d47f5e577735405ce5666ae06cca3d606d4caf43682e4915bf6899118d7c5f09cce9c888469bb1a0dceaa129672ef834be123e0020776573746d696e74
	///     0x9e261276cc9d1f8598ea4b6a74b15c2f3201801150b96d31c6b046eccab34257faa7b614006927c375cad165518ac0bb08ee57505f0e7b9012096b41c4eb3aaf947f6ea429080100801d509643da0eeb8601df78a957e1b50a9b69ef035cc97c4e9612df24ad0c846c8025a2c7363116c9a9c75890b57156a02ea08827057ef36b3904bcd9f94f8d1361
	///     0x9f099d880ec681799c0cf30e8886371da9ffff8004e32281670d1e60eb9972ad429206eba61af1c96aaac1bbdc1177e39a02908580b73adc8a4fde1b9c0d6c427d7c332bd2b5034b98686593f186fd7a481e64f43080f0c4d7c11a916daa2286eac94c977171953935270920f1f5abc6a4e78c4a3d938041034b4c6798fea37aae92ba80d0be65092120a31075ab3b7dda2505d78a780280bc42a9cdd47ff9a74290ebb6f811360228e908b8121a3c0492b35ebc8b11f64680af7e0c101ba329e3d289d49a26beef939117ce1dfa030f3a3555547c90f7117e80d3cbee33cdef01d03eaf93976ceeb574f9ba268fd8deccb2884892b770f0366080c38b39bc69961af6af12d02fb086c8530de05b091fe29cbc22ae221982d2d722804d62634bdc5e2b48874c061781a285cdeb57acae411377064e5a298df9191e6080e85b31c97cbf72c806c04d688bf2c97e63bf465094eda5c8f56300782353970780df12cc548109c2c505652e28a58aee6706b2f7e1e36128e4a04c3fce7992173280e848153205bd1d3e9bdac355859f17142876718fe22e9b8da4c2f7dc5f656d73809dd6ecb97e421388dcc33c307ef54ee811e531f168458684ef7493e911c6441e80eebcd8b73ac6208c3d7a886867c27f1411b861b6d806a8278e907de4a22072cc80147877616eb7c31f516685db05f253055e12bad5afd3cb15703d54d064814f88801b180fba88a8e432f13a7205cf2c8d13d8d8e9fc7e3ad8ba08d00a22d7ef07a1
	///   ]
	/// }
	pub fn sample_voting_proof() -> (
		parachains_common::Header,
		RawStorageProof,
		(&'static str, &'static str),
		Vec<u8>,
		Vec<u8>,
		Vec<u8>,
	) {
		use cumulus_primitives_core::rpsr_digest::RPSR_CONSENSUS_ID;
		use hex_literal::hex;
		use sp_consensus_aura::AURA_ENGINE_ID;
		use sp_runtime::{traits::Header, Digest, DigestItem};

		let header = parachains_common::Header::new(
			10_990_425,
			hex!("32bb8ded17ded4b9b73d6efef7fd7e533a647fa868e60e2dff2d288a7300456f").into(),
			hex!("b61ad16ff3226be01a583fdb83daad568f690d540bf78770bca841c6099fce8a").into(),
			hex!("347c2e995b0bbd470f083077529a9f8fcc8b18287195cd778ba1023b7fcc1fb3").into(),
			Digest {
				logs: vec![
					DigestItem::PreRuntime(AURA_ENGINE_ID, hex!("a74c4a1100000000").into()),
					DigestItem::Consensus(RPSR_CONSENSUS_ID, hex!("eff55cbcba39a6fb903bf960fc0036e731b439dd4275d4816ceeb386f1932fbf92bdf005").into()),
					DigestItem::Seal(AURA_ENGINE_ID, hex!("aac09b8f89fb7b22a3a03a4dfb75485bec4bc1fd31db5f3afaf20157642d5738d74aadc81893e2231ef4eb2ba98febfc4e2e94984ae9da87604b0bb656d6138e").into()),
				]
			}
		);
		assert_eq!(
			header.hash(),
			hex!("bad834d093eae042d175d304b1850c37c63e386e9f315b81a46af4867a78625c").into()
		);
		assert_eq!(
			*header.state_root(),
			hex!("b61ad16ff3226be01a583fdb83daad568f690d540bf78770bca841c6099fce8a").into()
		);
		assert_eq!(*header.number(), 10_990_425);

		let proof: RawStorageProof = vec![
			hex!("5f07c875e4cff74148e4628f264b974c8040911f81a6f7973b010000000000000000").to_vec(),
			hex!("5f0ccde6872881f893a21de93dfe970cd54000000000000000000000000000000000").to_vec(),
			hex!("7f1c0479328ac3d026b2c4cced7e2508f0709f5078496e42469e70752483d4f820702bb01e335d9e03bad0b54f7292514101050000000000000001000000000000000c274a38f05a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").to_vec(),
			hex!("80046080e9bdbd6c203a096900e1dd2f2d47d45946a067e4cd551196eab26d36051ecba380cc9b5db56b13ff27c4d6e0095813bd91f530b17f2f2a910031c53a1b383bd6218016acd44501b41bb36fd22a8909b61f9d7bb7f45cb6130438299eec917f1b939f").to_vec(),
			hex!("80c4f580767b3186e7d27b3ceaa7e246378683297760c8f42153788d52809d434d312a4980fc5adff30326ec3f500b34717369d33df240cbd92c963293ab82ba729e356a3b80a339a34e79ef509d9b58915488cb1f77e1cd1c0a5b04c19005de49d0095767ab80c8d419ae09dd82730fb6268c2bf550275b4fd049df142cbd2f15d7f683e140c78012c401e3cc61c9737c3e885d04dbd0b5fd621dc799a04b13c7a6237b8aed95e180dc012813ac654f8f5969cb4f574789cfd3ce9ae3499f56054030e2c45b5c2e5580eb552b0c46b7bb06f1dac22994130f43808b7e5f7da2de5093e677c52ab6819b805f55510441212c67f0a7104c4e184a8e6e3147a318f4c4d30fbd1a52ac3abe6b80c78706c96a78fd90cc65f656254b03b65f4172983c44f323b676d4087d898aa2").to_vec(),
			hex!("80fff9807fa50eeb70098a437a761aeb66827da80cd8207d35c2d7de6758ce5c3653b326802bd1003353af06e985b5f5575a1031fb9aef8e56448e824a9d636bd6121c1792803329646206fb88c84de424a6873feec85f582dafa4611cc9455c30b5fae7dc5580c46863f8caedf652f2e017a8ae1b1759c168f61b9a796707c6555303bcbad9a180c856363121a92532705af64424520359bca2e0553bef10ad9e0d46f0337865ab80e3939781e5587eb15d24b1003b0cddc779df202f6501a5af2fb945f7bc2efbbe80de0e104d48fdc81c1d8ae11fd7782e2e0633f9dd9c1084d39815baaf321307b58094ecfe19be5055ae905d7c5289849ec30cfc244fa4f0ea125c78781be31308ae80a2b681af74242c25cf4ccb90882422d594627ab8eea6edde4e17c6cd4a1b547880ecacdf555569b8fb9c07c88a53a36c20b14cc43e17932c6c3285ae43a804e4b080759522955003da04b57df9a6b207187b27f8094c2d539d4aeb89e2699baa0fc98085ba31b59e40c53c4a00a0c0fbe984b967dbe21550d448db14547f644946cfeb808c98be96ecc63f5feebcfa9845ffdb0bb55a03c22963a6e22f154fd3a699c9aa80a4106285a651e841f3c87183aef50c2539137065e0474e4b76b4106225c13e3b").to_vec(),
			hex!("80ffff8053802bea8e47ef2040edd0f45b3c204cb5d5d73519bf2133e3dec00148b8ba8480578efb66a190e9e624a7637470b436fcc6f0c8453dcfe55372db376ccb5cd4b6804921c66d181ef25fb3a9e7cbdf4675280e821885a31d0fdaed79534d6177b03480134b80cf3fca82e257cb190d49041189ebdb345b4ab2979bcde6ee98ad52a5c48025379f5495345ad8d1d4a34b0dbedb9f59608727f01171c696820cc6c8532127807709b44df764dd0f3118b2f6db558920ce488239eb801bef5e21e95f74cb519880fd2af833d762aabedb8eb6f05656cdddcb272ab7bc2bfcd3215808d7d4828ef8800ef6910150ae83571061d5c6711a738926d2e8e5a71d6a56533f7d817cca60e380cc6d7ef4acd1a2c0a9470e55cf171bb5412228749886975050650961be31946e80d50130ca987628db123c56681d3f294a45588c74de0b9b85fda64fdcd47238ed80c1d09e3bbf4449304cbca82f8855286e5b49cbda5359db412610cdf68030d64880aeb7dfbacb5887d8f7c8d6091cfe6bf4805e59882bf83dbf3dcd0306c8c8764c80d5202a65c8f8570e018ccac3c60e66931c0053c0ce7250866d7dd725c57454d980477b084c7136706560ba6656945db44645480472536204ad65732485b51327e48056dc6980c6f62bc8095421eb4c8fcf1c73b6f96fb1f333235007e0f58f3b3704806f76ba93efa7a569fda9e9a277ccd288bb0bb20994f01387b232f7ef0f498ac5").to_vec(),
			hex!("80ffff805497739bc143001f5b10bbbd385b2ce85f41aa3ff1e710e7738e719efdad75c880013392b637984ab2420d9f507a0ff03c03538918fa9e62e65525cea2143f8f638011a5a0ec4abb9738f8b6a338e7966f00348f928f674447a3fae4a157dfefb4098090e90d6f0b263c2af78d419cd918a954859078179341cbb22fe8de97238a526680068ed7ff383b08cb3f2c13226ce1a21e2cadc84f4877495ce87d3f37ec8dd8c080fa06a4717c79f9622339ee7b2d1e1578bad94aa6815ce63dd15f8bb45b42ec3580945d2e4223217dce5c7a71ea5d9cebc4dafa0aec86cd902d56f51c4e69a5f6f1807529efa8b50423e001ca59ec028c95fc2b97d70c2705193032522d328b13453e80aebc312d86ab860aa04b6511c476467347606176e3bd914b6c1652278e57277080335d735a801bbcb7a3d0a7ef4d8603b7377dc5054f41eef701dadf1bae3846a280daa969029586206915425633d0c4ef7870a72a7347cd24a03d6d540b11305e62807d9ffb29edd828b1ebf4fbecc78eddef1dabb615cc8d5aca057b48267ac57a7f806f1044bbf67024d28a3788a588fd8eb596690d5119613afa325edd2538d35649805146b095169ce4ad2e6b9c09fc8821cd37a5e1cade300ad1c5a64dfed60a191880d80fe2431e18e7bde67d80de4eb041c4815979d739a72bbc8e7c004240364aa880e29e0faf7da715ba5898a039d4d6a4213f69fdb9860776bab1a21fd2cb0ec1f3").to_vec(),
			hex!("80ffff80881e1bd89e48f3b556153687c69f7653c418235d9eb5a3b5c99de51735ca1ea180c143584b9fd2807e70e31a7d7baf898c2a67baf8f7e0cdd47641c6b5e469387880e2158826bdcd01c0f91553067aca089657485f30861d1eaa6263b564cd064fda80e49f32a8aab93d0c7b2463200979f08ae9f1def992662f14232457c556bf633a805f3b1b5930656cda7f039d95404959abf09c86aab1b9101c06789c4ffdfb11ec8080af541b5bf4c515f11757d0acda25c356571dac625228f64ab94d9fc0e575188071ebd553608b79ff52d6e8108254797b0ae85a861f1301743fa757b5489c9c488061a47891c8e383000001b518cc263caf8ad0ab31dda03d3955fe7e13fab11cfb80867e7deec5dae9c71dfbb3b9b7b23915f67facb5b39cd34a9048b5c921a205e48042e515d0900f7927b7a23c66274c323367faac300785ea2ef21912decfa517ca808199d0ea11be6dd7e9e84d3613adde930099aed1a39bf22c452d680f8f07806280494be8fd480fce05e24ab9fe6f142fff9ae96868ee1bb9f8b00409d641346f0b8089e50948ec86f9c1ed08ab17d71f030ee90598fd7b10b34c4f605671e0e19fac809b6e6dd45fc385fee9546463a330d2259cdf35d50e6470acc1e56405c658be7880171ec0a517fcbe64e845650d6e8c31f67c47cb24adadad1c2697a4e40a737f3980c23c2190b9997a331e6626ee0d1727dd8253c0eaa2180250d1ed5b24353f2f1f").to_vec(),
			hex!("8106800480c1756f24b4a1226ff188ea7c1dde613044f050aa3913d0677d17b7c5d45dcfdf80991cecd1b45ba5f7b3a6933aa9ee1dd480495184ffb7fde29c8dc0a58ff10f16").to_vec(),
			hex!("9d0a394eea5630e07c48ae0c9558cef7398f80bcda578efc67e66167154b78a937b0490fb9dc5bd82914918f8d7f0328a000f180e54a8aaef753bd15a917ffbf7990cfc3a4732e120ea716b4c2ab5c8f382471b7505f0e7b9012096b41c4eb3aaf947f6ea4290800004c5f0684a022a34dd8bfa2baaf44f172b710040180cb6c48716e3157314974172b93a043786dd4d66164e657e8b8c8e5fb41b61cc680aa1ef966dd7c07a56b7f0d348f269d628c5277af2533b3b66cd4c9136e17559c80408a52637ef2f653eee1412458abbaa1f2099f7c425b7302e8d7b866eb25916780150712d47f5e577735405ce5666ae06cca3d606d4caf43682e4915bf6899118d7c5f09cce9c888469bb1a0dceaa129672ef834be123e0020776573746d696e74").to_vec(),
			hex!("9e261276cc9d1f8598ea4b6a74b15c2f3201801150b96d31c6b046eccab34257faa7b614006927c375cad165518ac0bb08ee57505f0e7b9012096b41c4eb3aaf947f6ea429080100801d509643da0eeb8601df78a957e1b50a9b69ef035cc97c4e9612df24ad0c846c8025a2c7363116c9a9c75890b57156a02ea08827057ef36b3904bcd9f94f8d1361").to_vec(),
			hex!("9f099d880ec681799c0cf30e8886371da9ffff8004e32281670d1e60eb9972ad429206eba61af1c96aaac1bbdc1177e39a02908580b73adc8a4fde1b9c0d6c427d7c332bd2b5034b98686593f186fd7a481e64f43080f0c4d7c11a916daa2286eac94c977171953935270920f1f5abc6a4e78c4a3d938041034b4c6798fea37aae92ba80d0be65092120a31075ab3b7dda2505d78a780280bc42a9cdd47ff9a74290ebb6f811360228e908b8121a3c0492b35ebc8b11f64680af7e0c101ba329e3d289d49a26beef939117ce1dfa030f3a3555547c90f7117e80d3cbee33cdef01d03eaf93976ceeb574f9ba268fd8deccb2884892b770f0366080c38b39bc69961af6af12d02fb086c8530de05b091fe29cbc22ae221982d2d722804d62634bdc5e2b48874c061781a285cdeb57acae411377064e5a298df9191e6080e85b31c97cbf72c806c04d688bf2c97e63bf465094eda5c8f56300782353970780df12cc548109c2c505652e28a58aee6706b2f7e1e36128e4a04c3fce7992173280e848153205bd1d3e9bdac355859f17142876718fe22e9b8da4c2f7dc5f656d73809dd6ecb97e421388dcc33c307ef54ee811e531f168458684ef7493e911c6441e80eebcd8b73ac6208c3d7a886867c27f1411b861b6d806a8278e907de4a22072cc80147877616eb7c31f516685db05f253055e12bad5afd3cb15703d54d064814f88801b180fba88a8e432f13a7205cf2c8d13d8d8e9fc7e3ad8ba08d00a22d7ef07a1").to_vec()
		];

		let ss58_account = "5HVxofJkZcPs1emaJMWiJqd5aoWfDWobP7RiKBbbNTEDp5yy";
		let ss58_account_secret_key = "culture gadget inquiry ginger innocent pottery abstract reveal train gorilla despair emerge";
		use sp_core::{crypto::Ss58Codec, Pair};
		assert_eq!(
			ss58_account,
			sp_core::sr25519::Pair::from_string(ss58_account_secret_key, None)
				.unwrap()
				.public()
				.to_ss58check_with_version(42_u16.into()),
		);

		let account_balance_key = hex!("26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da9942ba479328ac3d026b2c4cced7e2508f0709f5078496e42469e70752483d4f820702bb01e335d9e03bad0b54f729251").into();
		let total_issuance_key =
			hex!("c2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80").into();
		let inactive_issuance_key =
			hex!("c2261276cc9d1f8598ea4b6a74b15c2f1ccde6872881f893a21de93dfe970cd5").into();
		(
			header,
			proof,
			(ss58_account, ss58_account_secret_key),
			account_balance_key,
			total_issuance_key,
			inactive_issuance_key,
		)
	}

	/// Returns relay chain data for AssetHub block: 0xbad834d093eae042d175d304b1850c37c63e386e9f315b81a46af4867a78625c:
	/// The relay chain `Paras::Heads(1000)` data from block:
	/// - 24_915_815 is Westend relay block: 0x0ebde3924be11018110a486a1b40b48c95ef01ad39f88daed97cbbebc1630321
	///
	/// RelayChain Proof generated by RPC call `state.getReadProof(key, at)`:
	/// - key: 0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3b6ff6f7d467b87a9e8030000
	///   - `Paras::Heads::get(1000)`
	/// - at: 0x0ebde3924be11018110a486a1b40b48c95ef01ad39f88daed97cbbebc1630321
	///
	/// {
	///   at: 0x0ebde3924be11018110a486a1b40b48c95ef01ad39f88daed97cbbebc1630321
	///   proof: [
	///     0x36ff6f7d467b87a9e8030000f94ed01d329eeb911c778babd291a05de2c2dedc09bfbdd49899ea9711030b08
	///     0x80446c80d30a5d60ff1a5157f70f2ee4bfcc5cbc9118bc3e61ad8dcc9089f1811c76074e80911e015f455657fe02ab564bf226adfe0e0e93969e95c8f397eb78b2b950d2e380c5745910b5760c4a4f135943e551953d3dabb94e5279514d6ab6eaf17b91cf02808f83f0d8e6399e9f9d10c3c89b614e318848dfb7ecead89b137ab1fe2fa968f980f005e048263805405e35c45335dddebd255b6d591f55318701db1c86a913d42380056aef2d2e9749427b2e7358d4fd5be472e842eb1ce75520a8b1deaec301168e
	///     0x80cc02802095a27d6c3c874443cfef91109fe351ff0eb1966e89f709d24dc1dddd94c147804180a84ac485f9bd5c693e145628db6a1bcbd922c22aa6b59a9a94c557cecdf680bc39b5834e9d9478203411e834ef51ab82ae0fd6ec769a26e8f81d3a1c2e3f478069bde6739731a31a3aa566767f8512fcafa144860885b9fe1057cfdcef9fe4d3802f8015fc169fba0816b8e290c0efee9b35e9c4a87029c35262e4e1b9cf85d9db
	///    0x80ffff806d3d9c3287d426e35d9017e46f30694858875a7a24e6c56ccb4f627c9e16224b80972538b6e68498b933b8740eaecd4d4b044d80267302203e67ebc3d09d2eeb20804266aab1435b35ec4745cdd96f345bc22b1fd94e772700a3496d45737517c44580c4ce606bb2dfa942695f4c5f6e9bf70ffbb1611eb25c4562ed18251102521b6e80e3e507f0515e722e16d7997c476574392ba14fb8409a25bc9a166eb944254ab880f4f61c299819b6260a2955f6f4620f1f3b90ce20e1339545738886c1fbdfe9b1801c3de84e18d8e5ca84cac1af8db8146c0560c3900e977b21f943be9ef3c9c0d0804f71142ab23511866821870959f09df29f54662afc3f01150024d005dfb06fea804da622a45b8dfd0421b406f26c0649249fd5492622f76f16f82ad5aea61230e780f3692189b82fc927b0182db509ce127f87e1e06d2f8bc2e1b521c8696df8cdec80e91b10493b5e111cec37806c14a6bafff4f321c000c9cf78a3e465bdbe7e6e238093fa7a12dd40ea90b2f2be12e06ec90310201c3d4200b193e9e0e80d027ef458809a90a345fc444bc45abc68eabb3b211988d9ddeee52fd8176ed9d33ad10f1b2f80a3a79c4f72559085c4a93e55ae99bd281337d950294bacc524ad776ef7e85cf880a621852ccdaddb5f3b14ecad82b7e5dff64a26aa580849e5123dfecf5444f643800615eaa6f66330a73191379096dc94b4d498f468a11da2f256d59f3c3b3f9db7
	///     0x9103347c2e995b0bbd470f083077529a9f8fcc8b18287195cd778ba1023b7fcc1fb366cd9e02b61ad16ff3226be01a583fdb83daad568f690d540bf78770bca841c6099fce8a32bb8ded17ded4b9b73d6efef7fd7e533a647fa868e60e2dff2d288a7300456f0c066175726120a74c4a1100000000045250535290eff55cbcba39a6fb903bf960fc0036e731b439dd4275d4816ceeb386f1932fbf92bdf00505617572610101aac09b8f89fb7b22a3a03a4dfb75485bec4bc1fd31db5f3afaf20157642d5738d74aadc81893e2231ef4eb2ba98febfc4e2e94984ae9da87604b0bb656d6138e
	///     0x9e710b30bd2eab0352ddcc26417aa1945fd380044fb0cba77b719a6b48d4ba365d10b941f5d257356af79055365a7ec37ded3380a2e2aa3ab427b2f44de51be05f46c8e6d1b2dbbda0d754b784f613804f47b91880a9c16ad38139f03e5bb228b7bcee2f7ac14961d58a14f568f2818bef7322eb628069713aa3ecd4549d37524985804ff60f367fc91c1bd229aa181e238657e9827980b410d238b5ea7604f4adbf263a38dbfa89c95d81628847f95eb667e8d0e935914c5f03c716fb8fff3de61a883bb76adb34a2040080cad92adb5641d6caaf11d7fe27dbea1e9a096ae85c0daebd59b1e25eaa4c7fa080b51f79412da4b52f974b1863428a781abdf82fcfe5303f0ee9f6055163e05cb880d2313d845e5c3d686852cf3f674db12f44774e297fa99264d5372698f060e9eb805757863d56669fd7a4f1b679b07e14cd1394e75b9aded6c617ee2c6557489c5480aa0dfbf86b38d60c5f5709e7bc57cbdf27fee6376a27949cf99d6f6b223642a5
	///     0x9f0b3c252fcb29d88eff4f3de5de4476c3ffff8050d90198bbf379b2f188dcf1ab68acdc1915e987b49047e394a4885f97d4313880ca8d7733aa29d9ff3ab2ae50a3c0ded0cae0b7237890a34967dc802dc59dc73280a40b94893db95917e44fd1bbba48148848cce9b13a146315951e2a975dda11cd801216188d9911f9999954db02f6d8090ec708e33d2b144b86835f0990f06033ef80a31aac84bfca357e96dd385d2d7cd1c5f90d67c238e904834587c42b3fe0a2ae8027c09a7dd9d5104b58d126a8f2e2d40c3a85d77178e8b0c06fafbd3b732e3bea8088b83f4017190bc2ed83e23663a6df793498d4da96bae824c9e8930575e85b7580e2afffcb5e2c49b0ccaf02326fd373c71ed829f774e017b8db12437947e35e4180ba965772350eecfc339659e689fa5b8e276037ef41def1caea15114051b66c48806df11e7a509e20680981f61d66462852287cf2e828ab20f1540c8c6fca69d3dd80410fb3228618850e8993b2f33927f60db725c4229552d44f3f14104aa0823dcc805fc4cfbcb17e68b9c0394589b1ce6a7c6f91916b46344acf791dae9d2e44302c80041f7a67266ba006ad56d34a9310035e73870a82bac0867a0afe6be4960325f080333354441303e9b7e7eb950e72738237977864e44ae1d87127024077fe6c64a380bece536c3fbd7b98bbb6e2bd43b5e31d09867858ce633dbd1dc4d8ebd735dac780cbdd8fb7c5f5b1ba3389b5770e3277b63c2e11032ce0ca525613f614e4d5d8f1
	///   ]
	/// }
	pub fn sample_relay_chain_proof() -> (RelayChainBlockNumber, RelayChainHash, RawStorageProof) {
		use super::AssetHubProver;
		use hex_literal::hex;

		let relay_chain_block_number = 24_915_815;
		let relay_chain_state_root: RelayChainHash =
			hex!("f1bbc8ceeb2e2df4c1486d6b8225181ca488d38ec570a9be25e1024b67ce2cbe").into();

		let relay_chain_proof: RawStorageProof = vec![
            hex!("36ff6f7d467b87a9e8030000f94ed01d329eeb911c778babd291a05de2c2dedc09bfbdd49899ea9711030b08").to_vec(),
                hex!("80446c80d30a5d60ff1a5157f70f2ee4bfcc5cbc9118bc3e61ad8dcc9089f1811c76074e80911e015f455657fe02ab564bf226adfe0e0e93969e95c8f397eb78b2b950d2e380c5745910b5760c4a4f135943e551953d3dabb94e5279514d6ab6eaf17b91cf02808f83f0d8e6399e9f9d10c3c89b614e318848dfb7ecead89b137ab1fe2fa968f980f005e048263805405e35c45335dddebd255b6d591f55318701db1c86a913d42380056aef2d2e9749427b2e7358d4fd5be472e842eb1ce75520a8b1deaec301168e").to_vec(),
                hex!("80cc02802095a27d6c3c874443cfef91109fe351ff0eb1966e89f709d24dc1dddd94c147804180a84ac485f9bd5c693e145628db6a1bcbd922c22aa6b59a9a94c557cecdf680bc39b5834e9d9478203411e834ef51ab82ae0fd6ec769a26e8f81d3a1c2e3f478069bde6739731a31a3aa566767f8512fcafa144860885b9fe1057cfdcef9fe4d3802f8015fc169fba0816b8e290c0efee9b35e9c4a87029c35262e4e1b9cf85d9db").to_vec(),
                hex!("80ffff806d3d9c3287d426e35d9017e46f30694858875a7a24e6c56ccb4f627c9e16224b80972538b6e68498b933b8740eaecd4d4b044d80267302203e67ebc3d09d2eeb20804266aab1435b35ec4745cdd96f345bc22b1fd94e772700a3496d45737517c44580c4ce606bb2dfa942695f4c5f6e9bf70ffbb1611eb25c4562ed18251102521b6e80e3e507f0515e722e16d7997c476574392ba14fb8409a25bc9a166eb944254ab880f4f61c299819b6260a2955f6f4620f1f3b90ce20e1339545738886c1fbdfe9b1801c3de84e18d8e5ca84cac1af8db8146c0560c3900e977b21f943be9ef3c9c0d0804f71142ab23511866821870959f09df29f54662afc3f01150024d005dfb06fea804da622a45b8dfd0421b406f26c0649249fd5492622f76f16f82ad5aea61230e780f3692189b82fc927b0182db509ce127f87e1e06d2f8bc2e1b521c8696df8cdec80e91b10493b5e111cec37806c14a6bafff4f321c000c9cf78a3e465bdbe7e6e238093fa7a12dd40ea90b2f2be12e06ec90310201c3d4200b193e9e0e80d027ef458809a90a345fc444bc45abc68eabb3b211988d9ddeee52fd8176ed9d33ad10f1b2f80a3a79c4f72559085c4a93e55ae99bd281337d950294bacc524ad776ef7e85cf880a621852ccdaddb5f3b14ecad82b7e5dff64a26aa580849e5123dfecf5444f643800615eaa6f66330a73191379096dc94b4d498f468a11da2f256d59f3c3b3f9db7").to_vec(),
                hex!("9103347c2e995b0bbd470f083077529a9f8fcc8b18287195cd778ba1023b7fcc1fb366cd9e02b61ad16ff3226be01a583fdb83daad568f690d540bf78770bca841c6099fce8a32bb8ded17ded4b9b73d6efef7fd7e533a647fa868e60e2dff2d288a7300456f0c066175726120a74c4a1100000000045250535290eff55cbcba39a6fb903bf960fc0036e731b439dd4275d4816ceeb386f1932fbf92bdf00505617572610101aac09b8f89fb7b22a3a03a4dfb75485bec4bc1fd31db5f3afaf20157642d5738d74aadc81893e2231ef4eb2ba98febfc4e2e94984ae9da87604b0bb656d6138e").to_vec(),
                hex!("9e710b30bd2eab0352ddcc26417aa1945fd380044fb0cba77b719a6b48d4ba365d10b941f5d257356af79055365a7ec37ded3380a2e2aa3ab427b2f44de51be05f46c8e6d1b2dbbda0d754b784f613804f47b91880a9c16ad38139f03e5bb228b7bcee2f7ac14961d58a14f568f2818bef7322eb628069713aa3ecd4549d37524985804ff60f367fc91c1bd229aa181e238657e9827980b410d238b5ea7604f4adbf263a38dbfa89c95d81628847f95eb667e8d0e935914c5f03c716fb8fff3de61a883bb76adb34a2040080cad92adb5641d6caaf11d7fe27dbea1e9a096ae85c0daebd59b1e25eaa4c7fa080b51f79412da4b52f974b1863428a781abdf82fcfe5303f0ee9f6055163e05cb880d2313d845e5c3d686852cf3f674db12f44774e297fa99264d5372698f060e9eb805757863d56669fd7a4f1b679b07e14cd1394e75b9aded6c617ee2c6557489c5480aa0dfbf86b38d60c5f5709e7bc57cbdf27fee6376a27949cf99d6f6b223642a5").to_vec(),
                hex!("9f0b3c252fcb29d88eff4f3de5de4476c3ffff8050d90198bbf379b2f188dcf1ab68acdc1915e987b49047e394a4885f97d4313880ca8d7733aa29d9ff3ab2ae50a3c0ded0cae0b7237890a34967dc802dc59dc73280a40b94893db95917e44fd1bbba48148848cce9b13a146315951e2a975dda11cd801216188d9911f9999954db02f6d8090ec708e33d2b144b86835f0990f06033ef80a31aac84bfca357e96dd385d2d7cd1c5f90d67c238e904834587c42b3fe0a2ae8027c09a7dd9d5104b58d126a8f2e2d40c3a85d77178e8b0c06fafbd3b732e3bea8088b83f4017190bc2ed83e23663a6df793498d4da96bae824c9e8930575e85b7580e2afffcb5e2c49b0ccaf02326fd373c71ed829f774e017b8db12437947e35e4180ba965772350eecfc339659e689fa5b8e276037ef41def1caea15114051b66c48806df11e7a509e20680981f61d66462852287cf2e828ab20f1540c8c6fca69d3dd80410fb3228618850e8993b2f33927f60db725c4229552d44f3f14104aa0823dcc805fc4cfbcb17e68b9c0394589b1ce6a7c6f91916b46344acf791dae9d2e44302c80041f7a67266ba006ad56d34a9310035e73870a82bac0867a0afe6be4960325f080333354441303e9b7e7eb950e72738237977864e44ae1d87127024077fe6c64a380bece536c3fbd7b98bbb6e2bd43b5e31d09867858ce633dbd1dc4d8ebd735dac780cbdd8fb7c5f5b1ba3389b5770e3277b63c2e11032ce0ca525613f614e4d5d8f1").to_vec(),
        ];

		assert_eq!(
		    AssetHubProver::paras_heads_storage_key(1000.into()),
		    hex!("cd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3b6ff6f7d467b87a9e8030000")
		);

		(relay_chain_block_number, relay_chain_state_root, relay_chain_proof)
	}

	#[test]
	fn verify_proof_root_works() {
		use super::{AssetHubProver, DDayProofRootStore, ProofInterface};
		use sp_runtime::BoundedVec;

		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// proofs
			let (asset_hub_header, ..) = sample_voting_proof();
			let (relay_chain_block_number, relay_chain_state_root, relay_chain_proof) =
				sample_relay_chain_proof();

			// Sync some relay chain data.
			DDayProofRootStore::do_note_new_roots(BoundedVec::truncate_from(vec![(
				relay_chain_block_number,
				relay_chain_state_root,
			)]));

			// check
			assert_eq!(
				AssetHubProver::verify_proof_root((
					relay_chain_block_number,
					asset_hub_header.number,
					relay_chain_proof
				)),
				Some(ProofRoot {
					at_block: asset_hub_header.number,
					proof_root: asset_hub_header.state_root,
				})
			);
		})
	}

	#[test]
	fn asset_hub_account_prover_works() {
		use super::{AssetHubProver, ProofInterface};
		use pallet_dday_voting::VotingPower;
		use parachains_common::AccountId;
		use sp_core::crypto::Ss58Codec;

		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// prepare proof
			let (
				asset_hub_header,
				proof,
				(ss58_account, _),
				account_balance_key,
				total_issuance_key,
				inactive_issuance_key,
			) = sample_voting_proof();
			let state_root = asset_hub_header.state_root;

			// check key generation for an account id
			let who = AccountId::from_ss58check(ss58_account).expect("valid accountId");
			assert_eq!(AssetHubProver::account_balance_storage_key(&who), account_balance_key,);
			assert_eq!(AssetHubProver::total_issuance_storage_key(), total_issuance_key,);
			assert_eq!(AssetHubProver::inactive_issuance_storage_key(), inactive_issuance_key,);

			// Ok - check `AssetHubProver` itself
			assert_eq!(
				AssetHubProver::query_voting_power_for(&who, state_root.clone(), proof.clone(),),
				Some(VotingPower {
					account_power: 99_987_783_034_636_u128,
					total: 88_831_707_570_053_009_u128,
				})
			);

			// None for an invalid account
			let random_account_1 = AccountId::from([2; 32]);
			assert_eq!(
				AssetHubProver::query_voting_power_for(
					&random_account_1,
					state_root.clone(),
					proof.clone(),
				),
				None,
			);

			// None for invalid state_root
			let invalid_state_root = parachains_common::Hash::default();
			assert_eq!(
				AssetHubProver::query_voting_power_for(&who, invalid_state_root, proof.clone(),),
				None,
			);

			// None for invalid proof
			let invalid_proof = {
				let mut ip = proof.clone();
				ip.remove(0);
				ip
			};
			assert_eq!(
				AssetHubProver::query_voting_power_for(&who, state_root, invalid_proof,),
				None,
			);
		})
	}
}
