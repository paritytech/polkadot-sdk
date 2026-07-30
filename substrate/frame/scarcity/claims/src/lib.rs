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

#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::Encode;
use frame_support::{ensure, traits::Get, weights::Weight};
use pallet_revive::{
	weights::WeightInfo as ReviveWeightInfo, AccountInfo, AddressMapper, ContractResult,
	ExecConfig, TransactionLimits, U256,
};
use sp_core::{sr25519, H256};
use sp_runtime::DispatchError;

pub use pallet::*;

/// Root sequence identifier assigned by the trusted source.
pub type RootId = u32;
/// Unique credit identifier and deterministic collection-selection entropy.
pub type CreditHash = H256;
/// Unix timestamp stored as the Merkle leaf value by the source.
pub type CreditTimestamp = u32;
/// Voucher key admitted by the source and authorized to spend one credit.
pub type VoucherPublic = sr25519::Public;
/// Signature authorizing one destination-chain claim.
pub type VoucherSignature = sr25519::Signature;
/// Exact base-2 proving trie shared with the source-chain root producer.
pub type CreditsTrie = sp_runtime::proving_trie::base2::BasicProvingTrie<
	sp_runtime::traits::BlakeTwo256,
	(VoucherPublic, CreditHash),
	CreditTimestamp,
>;

/// Domain separating Scarcity claim authorizations from every other signature protocol.
pub const CLAIM_DOMAIN: &[u8] = b"pallet-scarcity-claims/v1";

/// Construct the exact SCALE message a voucher key signs.
pub fn authorization_payload<Hash: Encode, AccountId: Encode>(
	genesis_hash: &Hash,
	root_id: RootId,
	credit_hash: CreditHash,
	collection: pallet_scarcity::CollectionId,
	destination: &AccountId,
) -> Vec<u8> {
	(CLAIM_DOMAIN, genesis_hash, root_id, credit_hash, collection, destination).encode()
}

/// Successful output from a collection's item selector.
pub struct Selection {
	/// Existing Scarcity item definition chosen for the credit.
	pub item: pallet_scarcity::ItemIndex,
	/// Weight actually consumed by selection; it must not exceed
	/// [`CollectionSelector::max_weight`].
	pub weight_consumed: Weight,
}

/// Runtime adapter for selecting an item from the current collection owner.
///
/// The claims pallet verifies the credit and owns the atomic state transition. This adapter is
/// deliberately small so runtimes may use the standard Revive implementation or another contract
/// environment without changing claim storage or authorization.
pub trait CollectionSelector<AccountId> {
	/// Worst-case weight reserved before dispatch.
	fn max_weight() -> Weight;

	/// Select an item using the credit hash as deterministic entropy.
	fn select(
		collection_owner: &AccountId,
		collection: pallet_scarcity::CollectionId,
		entropy: CreditHash,
	) -> Result<Selection, DispatchError>;
}

/// Limits used by [`ReviveCollectionSelector`].
pub trait ReviveSelectorConfig: frame_system::Config + pallet_revive::Config {
	/// Metered execution ceiling for one collection contract call.
	///
	/// The generated Revive call base weight is reserved in addition to this limit.
	type SelectorWeightLimit: Get<Weight>;
	/// Storage-deposit ceiling paid by the collection owner for one selector call.
	type SelectorDepositLimit: Get<pallet_revive::BalanceOf<Self>>;
}

/// Standard selector that treats the current Scarcity collection owner as a Revive contract.
pub struct ReviveCollectionSelector<T>(core::marker::PhantomData<T>);

impl<T> CollectionSelector<T::AccountId> for ReviveCollectionSelector<T>
where
	T: Config + ReviveSelectorConfig,
	<T as frame_system::Config>::RuntimeOrigin: From<frame_system::RawOrigin<T::AccountId>>,
{
	fn max_weight() -> Weight {
		<<T as pallet_revive::Config>::WeightInfo as ReviveWeightInfo>::call()
			.saturating_add(T::SelectorWeightLimit::get())
	}

	fn select(
		collection_owner: &T::AccountId,
		collection: pallet_scarcity::CollectionId,
		entropy: CreditHash,
	) -> Result<Selection, DispatchError> {
		let address = T::AddressMapper::to_address(collection_owner);
		ensure!(AccountInfo::<T>::is_contract(&address), Error::<T>::CollectionOwnerNotContract);

		let origin = frame_system::RawOrigin::Signed(collection_owner.clone()).into();
		let ContractResult { result, weight_consumed, .. } = pallet_revive::Pallet::<T>::bare_call(
			origin,
			address,
			U256::zero(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: T::SelectorWeightLimit::get(),
				deposit_limit: T::SelectorDepositLimit::get(),
			},
			selector_call_data(collection, entropy),
			&ExecConfig::new_substrate_tx_without_bump(),
		);

		let return_value = result.map_err(|_| Error::<T>::SelectorCallFailed)?;
		ensure!(!return_value.did_revert(), Error::<T>::SelectorReverted);
		let item =
			decode_selector_item(&return_value.data).ok_or(Error::<T>::InvalidSelectorReturn)?;

		let weight_consumed =
			<<T as pallet_revive::Config>::WeightInfo as ReviveWeightInfo>::call()
				.saturating_add(weight_consumed);
		Ok(Selection { item, weight_consumed })
	}
}

/// ABI-encode `select(uint32,bytes32)`.
fn selector_call_data(collection: pallet_scarcity::CollectionId, entropy: CreditHash) -> Vec<u8> {
	let mut data = Vec::with_capacity(68);
	let selector = sp_io::hashing::keccak_256(b"select(uint32,bytes32)");
	data.extend_from_slice(&selector[..4]);
	data.extend_from_slice(&[0u8; 28]);
	data.extend_from_slice(&collection.to_be_bytes());
	data.extend_from_slice(entropy.as_bytes());
	data
}

/// ABI-decode a single canonical `uint32` return word.
fn decode_selector_item(data: &[u8]) -> Option<pallet_scarcity::ItemIndex> {
	if data.len() != 32 || data[..28] != [0u8; 28] {
		return None;
	}
	Some(u32::from_be_bytes(data[28..].try_into().ok()?))
}

#[cfg(feature = "runtime-benchmarks")]
/// Selector used only in benchmark runtimes.
///
/// Contract execution is independently bounded by `CollectionSelector::max_weight`; pallet
/// benchmarks measure proof verification, authorization, accounting, and Scarcity minting.
pub struct BenchmarkSelector<T>(core::marker::PhantomData<T>);

#[cfg(feature = "runtime-benchmarks")]
impl<T: frame_system::Config> CollectionSelector<T::AccountId> for BenchmarkSelector<T> {
	fn max_weight() -> Weight {
		Weight::zero()
	}

	fn select(
		_collection_owner: &T::AccountId,
		_collection: pallet_scarcity::CollectionId,
		_entropy: CreditHash,
	) -> Result<Selection, DispatchError> {
		Ok(Selection { item: 0, weight_consumed: Weight::zero() })
	}
}

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::weights::WeightInfo;
	use binary_merkle_tree::MerkleProof;
	use codec::{DecodeAll, Encode};
	use frame_support::{pallet_prelude::*, traits::EnsureOrigin, transactional};
	use frame_system::pallet_prelude::*;
	use pallet_scarcity::MintWithoutDeposit;
	#[cfg(any(test, feature = "try-runtime"))]
	use sp_runtime::TryRuntimeError;
	use sp_runtime::{proving_trie::ProvingTrie, traits::Zero};

	/// Root commitment and completion accounting.
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
	)]
	pub struct RootInfo {
		/// Blake2-256 root of the committed credit trie.
		pub root: H256,
		/// Exact number of unique leaves committed by the source.
		pub claim_count: u32,
		/// Number of credits successfully converted on this chain.
		pub claimed_count: u32,
	}

	/// Permanent state of a globally unique credit.
	#[derive(
		CloneNoBound,
		PartialEqNoBound,
		EqNoBound,
		DebugNoBound,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub enum ClaimState<AccountId: Member> {
		/// Provisional marker written before contract execution to prevent reentrancy.
		Claiming { root_id: RootId },
		/// Successful conversion record.
		Claimed {
			root_id: RootId,
			collection: pallet_scarcity::CollectionId,
			item: pallet_scarcity::ItemIndex,
			instance: pallet_scarcity::InstanceId,
			destination: AccountId,
		},
	}

	#[pallet::config]
	pub trait Config:
		frame_system::Config<RuntimeEvent: From<Event<Self>>> + pallet_scarcity::Config
	{
		/// Origin authenticated as the trusted source-chain root producer.
		type RootOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Contract adapter used to select an item from the live collection owner.
		type CollectionSelector: CollectionSelector<Self::AccountId>;

		/// Maximum encoded Merkle proof size accepted by `claim`.
		#[pallet::constant]
		type MaxProofLen: Get<u32>;

		/// Maximum number of sibling hashes accepted in one Merkle proof.
		///
		/// This cannot usefully exceed 32 because the committed leaf count is a `u32`.
		#[pallet::constant]
		type MaxProofDepth: Get<u32>;

		/// Weights for this pallet's storage and cryptographic work.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Trusted roots indexed by their monotonically increasing source identifier.
	#[pallet::storage]
	pub type Roots<T> = StorageMap<_, Twox64Concat, RootId, RootInfo>;

	/// Greatest root identifier accepted so far.
	#[pallet::storage]
	pub type LatestRootId<T> = StorageValue<_, RootId>;

	/// Permanent, global one-time-use record keyed by credit hash.
	#[pallet::storage]
	pub type Claims<T: Config> =
		StorageMap<_, Blake2_128Concat, CreditHash, ClaimState<T::AccountId>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new trusted root was accepted.
		RootIngested { root_id: RootId, root: H256, claim_count: u32 },
		/// Every leaf in a root has been successfully claimed.
		RootCompleted { root_id: RootId },
		/// One credit was atomically converted into a Scarcity NFT.
		Claimed {
			root_id: RootId,
			credit_hash: CreditHash,
			collection: pallet_scarcity::CollectionId,
			item: pallet_scarcity::ItemIndex,
			instance: pallet_scarcity::InstanceId,
			destination: T::AccountId,
			submitter: T::AccountId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Root commitments must contain at least one leaf.
		EmptyRoot,
		/// A zero root is not a valid non-empty commitment.
		InvalidRoot,
		/// A new root identifier was not greater than the latest accepted identifier.
		StaleRoot,
		/// The root identifier already exists with different commitment data.
		ConflictingRoot,
		/// No trusted commitment exists for the requested root identifier.
		UnknownRoot,
		/// Every credit declared by this root has already been claimed.
		RootComplete,
		/// The proof could not be decoded or did not prove the submitted leaf.
		InvalidProof,
		/// The proof's embedded leaf count differs from the trusted root record.
		WrongLeafCount,
		/// The proof exceeds the runtime's configured maximum depth.
		ProofTooDeep,
		/// The credit hash has already been claimed or is being claimed reentrantly.
		AlreadyClaimed,
		/// The voucher did not authorize this chain, root, credit, collection, and destination.
		InvalidVoucherSignature,
		/// The current Scarcity collection owner is not a deployed Revive contract.
		CollectionOwnerNotContract,
		/// Contract execution trapped, exceeded its limits, or otherwise failed.
		SelectorCallFailed,
		/// The collection contract explicitly reverted selection.
		SelectorReverted,
		/// The collection contract did not return one canonical ABI `uint32`.
		InvalidSelectorReturn,
		/// A root's successful claim counter overflowed.
		ClaimCountOverflow,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Accept a trusted source-chain credit root.
		///
		/// Delivery is idempotent when all fields match an existing record. New identifiers must
		/// be strictly monotonic; roots are retained and can never be replaced or resurrected.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::ingest_root())]
		pub fn ingest_root(
			origin: OriginFor<T>,
			root_id: RootId,
			root: H256,
			claim_count: u32,
		) -> DispatchResult {
			T::RootOrigin::ensure_origin(origin)?;
			ensure!(claim_count > 0, Error::<T>::EmptyRoot);
			ensure!(root != H256::zero(), Error::<T>::InvalidRoot);

			if let Some(existing) = Roots::<T>::get(root_id) {
				ensure!(
					existing.root == root && existing.claim_count == claim_count,
					Error::<T>::ConflictingRoot
				);
				return Ok(());
			}

			if let Some(latest) = LatestRootId::<T>::get() {
				ensure!(root_id > latest, Error::<T>::StaleRoot);
			}

			Roots::<T>::insert(root_id, RootInfo { root, claim_count, claimed_count: 0 });
			LatestRootId::<T>::put(root_id);
			Self::deposit_event(Event::RootIngested { root_id, root, claim_count });
			Ok(())
		}

		/// Convert one source credit into a contract-selected Scarcity NFT.
		///
		/// Anyone may submit the transaction. The voucher signature, rather than the submitter,
		/// authorizes the destination and collection. The proof value is the source timestamp.
		#[pallet::call_index(1)]
		#[pallet::weight(
			<T as Config>::WeightInfo::claim(T::MaxProofDepth::get())
				.saturating_add(T::CollectionSelector::max_weight())
		)]
		#[transactional]
		pub fn claim(
			origin: OriginFor<T>,
			root_id: RootId,
			voucher: VoucherPublic,
			credit_hash: CreditHash,
			timestamp: CreditTimestamp,
			proof: BoundedVec<u8, T::MaxProofLen>,
			collection: pallet_scarcity::CollectionId,
			destination: T::AccountId,
			signature: VoucherSignature,
		) -> DispatchResultWithPostInfo {
			let submitter = ensure_signed(origin)?;
			ensure!(!Claims::<T>::contains_key(credit_hash), Error::<T>::AlreadyClaimed);

			let root_info = Roots::<T>::get(root_id).ok_or(Error::<T>::UnknownRoot)?;
			ensure!(root_info.claimed_count < root_info.claim_count, Error::<T>::RootComplete);

			let decoded = MerkleProof::<H256, Vec<u8>>::decode_all(&mut proof.as_slice())
				.map_err(|_| Error::<T>::InvalidProof)?;
			let proof_depth =
				u32::try_from(decoded.proof.len()).map_err(|_| Error::<T>::ProofTooDeep)?;
			ensure!(proof_depth <= T::MaxProofDepth::get(), Error::<T>::ProofTooDeep);
			ensure!(decoded.number_of_leaves == root_info.claim_count, Error::<T>::WrongLeafCount);
			CreditsTrie::verify_proof(
				&root_info.root,
				proof.as_slice(),
				&(voucher, credit_hash),
				&timestamp,
			)
			.map_err(|_| Error::<T>::InvalidProof)?;

			let genesis_hash = frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::zero());
			let payload = authorization_payload(
				&genesis_hash,
				root_id,
				credit_hash,
				collection,
				&destination,
			);
			ensure!(
				sp_io::crypto::sr25519_verify(&signature, &payload, &voucher),
				Error::<T>::InvalidVoucherSignature
			);

			let collection_info = pallet_scarcity::Collections::<T>::get(collection)
				.ok_or(pallet_scarcity::Error::<T>::UnknownCollection)?;

			// This marker is visible to any reentrant runtime call made by the selector. The
			// enclosing storage transaction removes it on every later failure.
			Claims::<T>::insert(credit_hash, ClaimState::Claiming { root_id });

			let selection =
				T::CollectionSelector::select(&collection_info.owner, collection, credit_hash)?;
			let instance =
				<pallet_scarcity::Pallet<T> as MintWithoutDeposit<T::AccountId>>::mint_without_deposit(
					collection,
					selection.item,
					destination.clone(),
					Vec::new(),
				)?;

			// Re-read the count after contract execution. A selector may reenter this pallet with
			// a different credit from the same root; mutating the pre-call snapshot here would
			// overwrite the nested claim's increment.
			let root_completed =
				Roots::<T>::try_mutate(root_id, |maybe_root| -> Result<bool, DispatchError> {
					let root = maybe_root.as_mut().ok_or(Error::<T>::UnknownRoot)?;
					ensure!(root.claimed_count < root.claim_count, Error::<T>::RootComplete);
					root.claimed_count =
						root.claimed_count.checked_add(1).ok_or(Error::<T>::ClaimCountOverflow)?;
					Ok(root.claimed_count == root.claim_count)
				})?;
			Claims::<T>::insert(
				credit_hash,
				ClaimState::Claimed {
					root_id,
					collection,
					item: selection.item,
					instance,
					destination: destination.clone(),
				},
			);

			Self::deposit_event(Event::Claimed {
				root_id,
				credit_hash,
				collection,
				item: selection.item,
				instance,
				destination,
				submitter,
			});
			if root_completed {
				Self::deposit_event(Event::RootCompleted { root_id });
			}

			let actual_weight = <T as Config>::WeightInfo::claim(proof_depth)
				.saturating_add(selection.weight_consumed);
			Ok(Some(actual_weight).into())
		}
	}

	impl<T: Config> Pallet<T> {
		#[cfg(any(test, feature = "try-runtime"))]
		pub(crate) fn do_try_state() -> Result<(), TryRuntimeError> {
			let latest = LatestRootId::<T>::get();
			let mut greatest_root = None;
			let mut actual_claims = alloc::collections::BTreeMap::<RootId, u32>::new();

			for (root_id, info) in Roots::<T>::iter() {
				if info.claim_count == 0 {
					return Err(TryRuntimeError::Other("claim root has zero leaves"));
				}
				if info.root == H256::zero() {
					return Err(TryRuntimeError::Other("claim root has a zero commitment"));
				}
				if info.claimed_count > info.claim_count {
					return Err(TryRuntimeError::Other("root claimed count exceeds leaf count"));
				}
				greatest_root =
					Some(greatest_root.map_or(root_id, |greatest: RootId| greatest.max(root_id)));
			}

			if latest != greatest_root {
				return Err(TryRuntimeError::Other(
					"latest root identifier does not match greatest root",
				));
			}

			for (_, state) in Claims::<T>::iter() {
				let ClaimState::Claimed { root_id, .. } = state else {
					return Err(TryRuntimeError::Other("provisional claim escaped dispatch"));
				};
				if !Roots::<T>::contains_key(root_id) {
					return Err(TryRuntimeError::Other("claim references an unknown root"));
				}
				let count = actual_claims.entry(root_id).or_default();
				*count = count
					.checked_add(1)
					.ok_or(TryRuntimeError::Other("root claim count overflowed"))?;
			}

			for (root_id, info) in Roots::<T>::iter() {
				let actual = actual_claims.get(&root_id).copied().unwrap_or_default();
				if actual != info.claimed_count {
					return Err(TryRuntimeError::Other(
						"root claimed count does not match claim records",
					));
				}
			}
			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), TryRuntimeError> {
			Self::do_try_state()
		}
	}
}

#[cfg(test)]
mod abi_tests {
	use super::*;

	#[test]
	fn selector_abi_is_canonical() {
		let entropy = H256::repeat_byte(0x42);
		let data = selector_call_data(0x0102_0304, entropy);
		assert_eq!(data.len(), 68);
		assert_eq!(&data[..4], &sp_io::hashing::keccak_256(b"select(uint32,bytes32)")[..4]);
		assert_eq!(&data[4..32], &[0u8; 28]);
		assert_eq!(&data[32..36], &0x0102_0304u32.to_be_bytes());
		assert_eq!(&data[36..], entropy.as_bytes());
	}

	#[test]
	fn selector_return_requires_one_canonical_u32_word() {
		let mut valid = [0u8; 32];
		valid[28..].copy_from_slice(&42u32.to_be_bytes());
		assert_eq!(decode_selector_item(&valid), Some(42));
		assert_eq!(decode_selector_item(&valid[..31]), None);
		valid[0] = 1;
		assert_eq!(decode_selector_item(&valid), None);
	}
}
