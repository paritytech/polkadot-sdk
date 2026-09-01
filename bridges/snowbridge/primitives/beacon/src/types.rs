// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{CloneNoBound, DebugNoBound, PartialEqNoBound};
use scale_info::TypeInfo;
use sp_core::{H160, H256, U256};
use sp_std::{boxed::Box, iter::repeat, prelude::*};
use Debug;

use crate::config::{MAX_EXECUTION_HEADER_RLP_SIZE, PUBKEY_SIZE, SIGNATURE_SIZE};

#[cfg(feature = "std")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(feature = "std")]
use crate::serde_utils::HexVisitor;

use crate::ssz::{
	hash_tree_root, SSZBeaconBlockHeader, SSZExecutionPayloadHeader, SSZForkData, SSZSigningData,
	SSZSyncAggregate, SSZSyncCommittee,
};
use frame_support::{traits::ConstU32, BoundedVec};
use sp_io::hashing::keccak_256;
use ssz_rs::SimpleSerializeError;

pub use crate::bits::decompress_sync_committee_bits;

use crate::bls::{prepare_g1_pubkeys, prepare_milagro_pubkey, BlsError};
use milagro_bls::PublicKey as PublicKeyPrepared;

pub type ValidatorIndex = u64;
pub type ForkVersion = [u8; 4];

#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct ForkVersions {
	pub genesis: Fork,
	pub altair: Fork,
	pub bellatrix: Fork,
	pub capella: Fork,
	pub deneb: Fork,
	pub electra: Fork,
	pub fulu: Fork,
	pub gloas: Fork,
}

#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct Fork {
	pub version: [u8; 4],
	pub epoch: u64,
}

#[derive(Copy, Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Debug, TypeInfo)]
pub struct PublicKey(pub [u8; PUBKEY_SIZE]);

impl Default for PublicKey {
	fn default() -> Self {
		PublicKey([0u8; PUBKEY_SIZE])
	}
}

impl From<[u8; PUBKEY_SIZE]> for PublicKey {
	fn from(v: [u8; PUBKEY_SIZE]) -> Self {
		Self(v)
	}
}

impl MaxEncodedLen for PublicKey {
	fn max_encoded_len() -> usize {
		PUBKEY_SIZE
	}
}

#[cfg(feature = "std")]
impl<'de> Deserialize<'de> for PublicKey {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		deserializer.deserialize_str(HexVisitor::<PUBKEY_SIZE>()).map(|v| v.into())
	}
}

#[cfg(feature = "std")]
impl Serialize for PublicKey {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_bytes(&self.0)
	}
}

#[derive(Copy, Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Debug, TypeInfo)]
pub struct Signature(pub [u8; SIGNATURE_SIZE]);

impl Default for Signature {
	fn default() -> Self {
		Signature([0u8; SIGNATURE_SIZE])
	}
}

impl From<[u8; SIGNATURE_SIZE]> for Signature {
	fn from(v: [u8; SIGNATURE_SIZE]) -> Self {
		Self(v)
	}
}

#[cfg(feature = "std")]
impl<'de> Deserialize<'de> for Signature {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		deserializer.deserialize_str(HexVisitor::<SIGNATURE_SIZE>()).map(|v| v.into())
	}
}

#[derive(Copy, Clone, Default, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct FinalizedHeaderState {
	pub beacon_block_root: H256,
	pub beacon_slot: u64,
}

#[derive(Clone, Default, Encode, Decode, PartialEq, Debug)]
pub struct ForkData {
	// 1 or 0 bit, indicates whether a sync committee participated in a vote
	pub current_version: [u8; 4],
	pub genesis_validators_root: [u8; 32],
}

impl ForkData {
	pub fn hash_tree_root(&self) -> Result<H256, SimpleSerializeError> {
		hash_tree_root::<SSZForkData>(self.clone().into())
	}
}

#[derive(Clone, Default, Encode, Decode, PartialEq, Debug)]
pub struct SigningData {
	pub object_root: H256,
	pub domain: H256,
}

impl SigningData {
	pub fn hash_tree_root(&self) -> Result<H256, SimpleSerializeError> {
		hash_tree_root::<SSZSigningData>(self.clone().into())
	}
}

/// Sync committee as it is stored in the runtime storage.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	PartialEqNoBound,
	CloneNoBound,
	DebugNoBound,
	TypeInfo,
	MaxEncodedLen,
)]
#[cfg_attr(
	feature = "std",
	derive(Serialize, Deserialize),
	serde(deny_unknown_fields, bound(serialize = ""), bound(deserialize = ""))
)]
#[codec(mel_bound())]
pub struct SyncCommittee<const COMMITTEE_SIZE: usize> {
	#[cfg_attr(feature = "std", serde(with = "crate::serde_utils::arrays"))]
	pub pubkeys: [PublicKey; COMMITTEE_SIZE],
	pub aggregate_pubkey: PublicKey,
}

impl<const COMMITTEE_SIZE: usize> Default for SyncCommittee<COMMITTEE_SIZE> {
	fn default() -> Self {
		SyncCommittee {
			pubkeys: [Default::default(); COMMITTEE_SIZE],
			aggregate_pubkey: Default::default(),
		}
	}
}

impl<const COMMITTEE_SIZE: usize> SyncCommittee<COMMITTEE_SIZE> {
	pub fn hash_tree_root(&self) -> Result<H256, SimpleSerializeError> {
		hash_tree_root::<SSZSyncCommittee<COMMITTEE_SIZE>>(self.clone().into())
	}
}

/// Prepared G1 public key of sync committee as it is stored in the runtime storage.
#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct SyncCommitteePrepared<const COMMITTEE_SIZE: usize> {
	pub root: H256,
	pub pubkeys: Box<[PublicKeyPrepared; COMMITTEE_SIZE]>,
	pub aggregate_pubkey: PublicKeyPrepared,
}

impl<const COMMITTEE_SIZE: usize> Default for SyncCommitteePrepared<COMMITTEE_SIZE> {
	fn default() -> Self {
		let pubkeys: Vec<PublicKeyPrepared> =
			repeat(PublicKeyPrepared::default()).take(COMMITTEE_SIZE).collect();
		let pubkeys: Box<[PublicKeyPrepared; COMMITTEE_SIZE]> =
			Box::new(pubkeys.try_into().map_err(|_| ()).expect("checked statically; qed"));

		SyncCommitteePrepared {
			root: H256::default(),
			pubkeys,
			aggregate_pubkey: PublicKeyPrepared::default(),
		}
	}
}

impl<const COMMITTEE_SIZE: usize> TryFrom<&SyncCommittee<COMMITTEE_SIZE>>
	for SyncCommitteePrepared<COMMITTEE_SIZE>
{
	type Error = BlsError;

	fn try_from(sync_committee: &SyncCommittee<COMMITTEE_SIZE>) -> Result<Self, Self::Error> {
		let g1_pubkeys = prepare_g1_pubkeys(&sync_committee.pubkeys)?;
		let sync_committee_root = sync_committee.hash_tree_root().expect("checked statically; qed");

		Ok(SyncCommitteePrepared::<COMMITTEE_SIZE> {
			pubkeys: g1_pubkeys.try_into().map_err(|_| ()).expect("checked statically; qed"),
			aggregate_pubkey: prepare_milagro_pubkey(&sync_committee.aggregate_pubkey)?,
			root: sync_committee_root,
		})
	}
}

/// Beacon block header as it is stored in the runtime storage. The block root is the
/// Merkleization of a BeaconHeader.
#[derive(
	Copy,
	Clone,
	Default,
	Encode,
	Decode,
	DecodeWithMemTracking,
	PartialEq,
	Debug,
	TypeInfo,
	MaxEncodedLen,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BeaconHeader {
	// The slot for which this block is created. Must be greater than the slot of the block defined
	// by parent root.
	pub slot: u64,
	// The index of the validator that proposed the block.
	pub proposer_index: ValidatorIndex,
	// The block root of the parent block, forming a block chain.
	pub parent_root: H256,
	// The hash root of the post state of running the state transition through this block.
	pub state_root: H256,
	// The hash root of the beacon block body
	pub body_root: H256,
}

impl BeaconHeader {
	pub fn hash_tree_root(&self) -> Result<H256, SimpleSerializeError> {
		hash_tree_root::<SSZBeaconBlockHeader>((*self).into())
	}
}

#[derive(
	Encode, Decode, DecodeWithMemTracking, CloneNoBound, PartialEqNoBound, DebugNoBound, TypeInfo,
)]
#[cfg_attr(
	feature = "std",
	derive(Deserialize),
	serde(
		try_from = "IntermediateSyncAggregate",
		deny_unknown_fields,
		bound(serialize = ""),
		bound(deserialize = "")
	)
)]
#[codec(mel_bound())]
pub struct SyncAggregate<const COMMITTEE_SIZE: usize, const COMMITTEE_BITS_SIZE: usize> {
	pub sync_committee_bits: [u8; COMMITTEE_BITS_SIZE],
	pub sync_committee_signature: Signature,
}

impl<const COMMITTEE_SIZE: usize, const COMMITTEE_BITS_SIZE: usize> Default
	for SyncAggregate<COMMITTEE_SIZE, COMMITTEE_BITS_SIZE>
{
	fn default() -> Self {
		SyncAggregate {
			sync_committee_bits: [0; COMMITTEE_BITS_SIZE],
			sync_committee_signature: Default::default(),
		}
	}
}

impl<const COMMITTEE_SIZE: usize, const COMMITTEE_BITS_SIZE: usize>
	SyncAggregate<COMMITTEE_SIZE, COMMITTEE_BITS_SIZE>
{
	pub fn hash_tree_root(&self) -> Result<H256, SimpleSerializeError> {
		hash_tree_root::<SSZSyncAggregate<COMMITTEE_SIZE>>(self.clone().into())
	}
}

/// Serde deserialization helper for SyncAggregate
#[cfg(feature = "std")]
#[derive(Deserialize)]
struct IntermediateSyncAggregate {
	#[cfg_attr(feature = "std", serde(deserialize_with = "crate::serde_utils::from_hex_to_bytes"))]
	pub sync_committee_bits: Vec<u8>,
	pub sync_committee_signature: Signature,
}

#[cfg(feature = "std")]
impl<const COMMITTEE_SIZE: usize, const COMMITTEE_BITS_SIZE: usize>
	TryFrom<IntermediateSyncAggregate> for SyncAggregate<COMMITTEE_SIZE, COMMITTEE_BITS_SIZE>
{
	type Error = String;

	fn try_from(other: IntermediateSyncAggregate) -> Result<Self, Self::Error> {
		Ok(Self {
			sync_committee_bits: other
				.sync_committee_bits
				.try_into()
				.map_err(|_| "unexpected length".to_owned())?,
			sync_committee_signature: other.sync_committee_signature,
		})
	}
}

/// ExecutionPayloadHeader
/// <https://github.com/ethereum/annotated-spec/blob/master/capella/beacon-chain.md#executionpayloadheader>
#[derive(
	Default,
	Encode,
	Decode,
	DecodeWithMemTracking,
	CloneNoBound,
	PartialEqNoBound,
	DebugNoBound,
	TypeInfo,
)]
#[cfg_attr(
	feature = "std",
	derive(Serialize, Deserialize),
	serde(deny_unknown_fields, bound(serialize = ""), bound(deserialize = ""))
)]
#[codec(mel_bound())]
pub struct ExecutionPayloadHeader {
	pub parent_hash: H256,
	pub fee_recipient: H160,
	pub state_root: H256,
	pub receipts_root: H256,
	#[cfg_attr(feature = "std", serde(deserialize_with = "crate::serde_utils::from_hex_to_bytes"))]
	pub logs_bloom: Vec<u8>,
	pub prev_randao: H256,
	pub block_number: u64,
	pub gas_limit: u64,
	pub gas_used: u64,
	pub timestamp: u64,
	#[cfg_attr(feature = "std", serde(deserialize_with = "crate::serde_utils::from_hex_to_bytes"))]
	pub extra_data: Vec<u8>,
	#[cfg_attr(feature = "std", serde(deserialize_with = "crate::serde_utils::from_int_to_u256"))]
	pub base_fee_per_gas: U256,
	pub block_hash: H256,
	pub transactions_root: H256,
	pub withdrawals_root: H256,
}

impl ExecutionPayloadHeader {
	pub fn hash_tree_root(&self) -> Result<H256, SimpleSerializeError> {
		hash_tree_root::<SSZExecutionPayloadHeader>(self.clone().try_into()?)
	}
}

#[derive(
	Default, Encode, Decode, Copy, Clone, PartialEqNoBound, DebugNoBound, TypeInfo, MaxEncodedLen,
)]
pub struct CompactBeaconState {
	#[codec(compact)]
	pub slot: u64,
	pub block_roots_root: H256,
}

/// VersionedExecutionPayloadHeader
#[derive(
	Encode, Decode, DecodeWithMemTracking, CloneNoBound, PartialEqNoBound, DebugNoBound, TypeInfo,
)]
#[cfg_attr(
	feature = "std",
	derive(Serialize, Deserialize),
	serde(deny_unknown_fields, bound(serialize = ""), bound(deserialize = ""))
)]
#[codec(mel_bound())]
pub enum VersionedExecutionPayloadHeader {
	Capella(ExecutionPayloadHeader),
	Deneb(deneb::ExecutionPayloadHeader),
	/// [New in Gloas:EIP7732] Canonical RLP bytes of the Ethereum execution header.
	///
	/// Gloas removes `execution_payload` from `BeaconBlockBody`, so the beacon block no
	/// longer commits to an SSZ `ExecutionPayloadHeader` and therefore no longer commits
	/// to a `receipts_root`. It commits only to an execution block hash. These bytes are
	/// authenticated by `keccak256(bytes) == <committed block hash>`; the `receipts_root`
	/// is then read out of the authenticated encoding.
	///
	/// The bytes must be hashed exactly as submitted. An Ethereum block hash is the Keccak
	/// hash of the canonical encoded header, so decoding and re-encoding before hashing
	/// would be wrong.
	Gloas(BoundedVec<u8, ConstU32<MAX_EXECUTION_HEADER_RLP_SIZE>>),
}

/// Which commitment scheme a proof uses. The two are proven at different generalized
/// indices against different leaves, so this is not merely an encoding difference.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum CommitmentScheme {
	/// Pre-Gloas: the leaf is the SSZ hash-tree root of the full execution payload header,
	/// committed at `BeaconBlockBody.execution_payload`.
	PayloadHeaderRoot,
	/// Gloas (EIP-7732): the leaf is the execution block hash, committed at
	/// `BeaconBlockBody.signed_execution_payload_bid.message.parent_block_hash`.
	BlockHash,
}

/// Why a commitment could not be derived from a submitted execution header.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum CommitmentError {
	/// The pre-Gloas payload header could not be merkleized.
	Merkleization,
	/// The Gloas bytes are not one canonical Ethereum execution header.
	MalformedExecutionHeader,
}

/// What `ExecutionProof::execution_branch` must prove, and what proving it establishes.
///
/// Carries the leaf to verify against `BeaconHeader::body_root` together with the receipts
/// root that leaf authenticates. The receipts root is private and reachable only through
/// [`Self::receipts_root_once_proven`], whose name states its precondition. That is the
/// point of the type: a submitter-supplied receipts root should be unreachable from a proof
/// nobody verified.
#[must_use]
#[derive(Clone, PartialEq, Debug)]
pub struct ExecutionCommitment {
	leaf: H256,
	scheme: CommitmentScheme,
	receipts_root: H256,
}

impl ExecutionCommitment {
	/// The leaf to verify against `BeaconHeader::body_root`.
	pub fn leaf(&self) -> H256 {
		self.leaf
	}

	pub fn scheme(&self) -> CommitmentScheme {
		self.scheme
	}

	/// Whether this proof uses the Gloas scheme. Cross-check this against the fork era of
	/// the beacon header's slot, so a proof cannot select its own verification path.
	pub fn is_gloas(&self) -> bool {
		matches!(self.scheme, CommitmentScheme::BlockHash)
	}

	/// The receipts root this commitment authenticates.
	///
	/// Call only once [`Self::leaf`] has been proven into a finalized beacon block.
	/// Consumes the commitment so the leaf cannot be reused afterwards.
	pub fn receipts_root_once_proven(self) -> H256 {
		self.receipts_root
	}
}

impl VersionedExecutionPayloadHeader {
	/// Derive what the execution branch must prove, and what proving it establishes.
	///
	/// This is the only way to obtain a receipts root. For Gloas that means parsing
	/// submitter-supplied bytes, which happens here — before verification, deliberately, so
	/// malformed input is rejected without paying for a merkle check.
	pub fn commitment(&self) -> Result<ExecutionCommitment, CommitmentError> {
		Ok(match self {
			VersionedExecutionPayloadHeader::Capella(header) => ExecutionCommitment {
				leaf: hash_tree_root::<SSZExecutionPayloadHeader>(
					header.clone().try_into().map_err(|_| CommitmentError::Merkleization)?,
				)
				.map_err(|_| CommitmentError::Merkleization)?,
				scheme: CommitmentScheme::PayloadHeaderRoot,
				receipts_root: header.receipts_root,
			},
			VersionedExecutionPayloadHeader::Deneb(header) => ExecutionCommitment {
				leaf: hash_tree_root::<crate::ssz::deneb::SSZExecutionPayloadHeader>(
					header.clone().try_into().map_err(|_| CommitmentError::Merkleization)?,
				)
				.map_err(|_| CommitmentError::Merkleization)?,
				scheme: CommitmentScheme::PayloadHeaderRoot,
				receipts_root: header.receipts_root,
			},
			// The leaf is the execution block hash, which is by definition the Keccak hash
			// of the canonical header encoding. Hash the bytes exactly as submitted.
			VersionedExecutionPayloadHeader::Gloas(rlp) => ExecutionCommitment {
				leaf: keccak_256(rlp).into(),
				scheme: CommitmentScheme::BlockHash,
				receipts_root: receipts_root_from_rlp(rlp)
					.ok_or(CommitmentError::MalformedExecutionHeader)?,
			},
		})
	}
}

/// Index of `receipts_root` in the canonical Ethereum execution header RLP list.
const HEADER_RECEIPTS_ROOT_INDEX: usize = 5;
/// A canonical Ethereum execution header has had at least this many fields since frontier.
const HEADER_MIN_FIELDS: usize = 15;

/// Reads `receipts_root` out of canonical Ethereum execution header RLP.
///
/// Strict on purpose: the bytes are only trustworthy because their Keccak hash matched a
/// beacon-committed block hash, so anything that is not exactly one canonical top-level
/// list is rejected rather than interpreted.
fn receipts_root_from_rlp(bytes: &[u8]) -> Option<H256> {
	let mut buf = bytes;
	let header = alloy_rlp::Header::decode(&mut buf).ok()?;
	if !header.list || buf.len() != header.payload_length {
		// Not a list, or trailing bytes after the list payload.
		return None;
	}

	let mut fields = 0usize;
	let mut receipts_root = None;
	while !buf.is_empty() {
		let item = alloy_rlp::Header::decode(&mut buf).ok()?;
		if item.payload_length > buf.len() {
			return None;
		}
		let (payload, rest) = buf.split_at(item.payload_length);
		buf = rest;
		if fields == HEADER_RECEIPTS_ROOT_INDEX {
			if item.list || payload.len() != 32 {
				return None;
			}
			receipts_root = Some(H256::from_slice(payload));
		}
		fields = fields.checked_add(1)?;
	}

	if fields < HEADER_MIN_FIELDS {
		return None;
	}
	receipts_root
}

#[derive(
	Encode, Decode, DecodeWithMemTracking, CloneNoBound, PartialEqNoBound, DebugNoBound, TypeInfo,
)]
#[cfg_attr(
	feature = "std",
	derive(serde::Deserialize),
	serde(deny_unknown_fields, bound(serialize = ""), bound(deserialize = ""))
)]
pub struct ExecutionProof {
	/// Header for the beacon block containing the execution payload
	pub header: BeaconHeader,
	/// Proof that `header` is an ancestor of a finalized header
	pub ancestry_proof: Option<AncestryProof>,
	/// The execution header to be verified
	pub execution_header: VersionedExecutionPayloadHeader,
	/// Merkle proof that execution payload is contained within `header`
	pub execution_branch: Vec<H256>,
}

#[derive(
	Encode, Decode, DecodeWithMemTracking, CloneNoBound, PartialEqNoBound, DebugNoBound, TypeInfo,
)]
#[cfg_attr(
	feature = "std",
	derive(serde::Deserialize),
	serde(deny_unknown_fields, bound(serialize = ""), bound(deserialize = ""))
)]
pub struct AncestryProof {
	/// Merkle proof that `header` is an ancestor of `finalized_header`
	pub header_branch: Vec<H256>,
	/// Root of a finalized block that has already been imported into the light client
	pub finalized_block_root: H256,
}

#[cfg(test)]
mod tests {
	use super::*;
	use hex_literal::hex;

	#[test]
	pub fn test_hash_beacon_header1() {
		let hash_root = BeaconHeader {
			slot: 3,
			proposer_index: 2,
			parent_root: hex!("796ea53efb534eab7777809cc5ee2d84e7f25024b9d0c4d7e5bcaab657e4bdbd")
				.into(),
			state_root: hex!("ba3ff080912be5c9c158b2e962c1b39a91bc0615762ba6fa2ecacafa94e9ae0a")
				.into(),
			body_root: hex!("a18d7fcefbb74a177c959160e0ee89c23546482154e6831237710414465dcae5")
				.into(),
		}
		.hash_tree_root();

		assert!(hash_root.is_ok());
		assert_eq!(
			hash_root.unwrap(),
			hex!("7d42595818709e805dd2fa710a2d2c1f62576ef1ab7273941ac9130fb94b91f7").into()
		);
	}

	#[test]
	pub fn test_hash_beacon_header2() {
		let hash_root = BeaconHeader {
			slot: 3476424,
			proposer_index: 314905,
			parent_root: hex!("c069d7b49cffd2b815b0fb8007eb9ca91202ea548df6f3db60000f29b2489f28")
				.into(),
			state_root: hex!("444d293e4533501ee508ad608783a7d677c3c566f001313e8a02ce08adf590a3")
				.into(),
			body_root: hex!("6508a0241047f21ba88f05d05b15534156ab6a6f8e029a9a5423da429834e04a")
				.into(),
		}
		.hash_tree_root();

		assert!(hash_root.is_ok());
		assert_eq!(
			hash_root.unwrap(),
			hex!("0aa41166ff01e58e111ac8c42309a738ab453cf8d7285ed8477b1c484acb123e").into()
		);
	}

	#[test]
	pub fn test_hash_fork_data() {
		let hash_root = ForkData {
			current_version: hex!("83f38a34"),
			genesis_validators_root: hex!(
				"22370bbbb358800f5711a10ea9845284272d8493bed0348cab87b8ab1e127930"
			),
		}
		.hash_tree_root();

		assert!(hash_root.is_ok());
		assert_eq!(
			hash_root.unwrap(),
			hex!("57c12c4246bc7152b174b51920506bf943eff9c7ffa50b9533708e9cc1f680fc").into()
		);
	}

	#[test]
	pub fn test_hash_signing_data() {
		let hash_root = SigningData {
			object_root: hex!("63654cbe64fc07853f1198c165dd3d49c54fc53bc417989bbcc66da15f850c54")
				.into(),
			domain: hex!("037da907d1c3a03c0091b2254e1480d9b1783476e228ab29adaaa8f133e08f7a").into(),
		}
		.hash_tree_root();

		assert!(hash_root.is_ok());
		assert_eq!(
			hash_root.unwrap(),
			hex!("b9eb2caf2d691b183c2d57f322afe505c078cd08101324f61c3641714789a54e").into()
		);
	}

	#[test]
	pub fn test_hash_sync_aggregate() {
		let hash_root = SyncAggregate::<512, 64>{
				sync_committee_bits: hex!("cefffffefffffff767fffbedffffeffffeeffdffffdebffffff7f7dbdf7fffdffffbffcfffdff79dfffbbfefff2ffffff7ddeff7ffffc98ff7fbfffffffffff7"),
				sync_committee_signature: hex!("8af1a8577bba419fe054ee49b16ed28e081dda6d3ba41651634685e890992a0b675e20f8d9f2ec137fe9eb50e838aa6117f9f5410e2e1024c4b4f0e098e55144843ce90b7acde52fe7b94f2a1037342c951dc59f501c92acf7ed944cb6d2b5f7").into(),
		}.hash_tree_root();

		assert!(hash_root.is_ok());
		assert_eq!(
			hash_root.unwrap(),
			hex!("e6dcad4f60ce9ff8a587b110facbaf94721f06cd810b6d8bf6cffa641272808d").into()
		);
	}

	#[test]
	pub fn test_hash_execution_payload() {
		let hash_root =
            ExecutionPayloadHeader{
                parent_hash: hex!("eadee5ab098dde64e9fd02ae5858064bad67064070679625b09f8d82dec183f7").into(),
                fee_recipient: hex!("f97e180c050e5ab072211ad2c213eb5aee4df134").into(),
                state_root: hex!("564fa064c2a324c2b5978d7fdfc5d4224d4f421a45388af1ed405a399c845dff").into(),
                receipts_root: hex!("56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421").into(),
                logs_bloom: hex!("00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").to_vec(),
                prev_randao: hex!("6bf538bdfbdf1c96ff528726a40658a91d0bda0f1351448c4c4f3604db2a0ccf").into(),
                block_number: 477434,
                gas_limit: 8154925,
                gas_used: 0,
                timestamp: 1652816940,
                extra_data: vec![],
                base_fee_per_gas: U256::from(7_i16),
                block_hash: hex!("cd8df91b4503adb8f2f1c7a4f60e07a1f1a2cbdfa2a95bceba581f3ff65c1968").into(),
                transactions_root: hex!("7ffe241ea60187fdb0187bfa22de35d1f9bed7ab061d9401fd47e34a54fbede1").into(),
				withdrawals_root: hex!("28ba1834a3a7b657460ce79fa3a1d909ab8828fd557659d4d0554a9bdbc0ec30").into(),
			}.hash_tree_root();
		assert!(hash_root.is_ok());
	}
}

/// Operating modes for beacon client
#[derive(Encode, Decode, Copy, Clone, PartialEq, Debug, TypeInfo)]
pub enum Mode {
	Active,
	Blocked,
}

pub mod deneb {
	use codec::{Decode, DecodeWithMemTracking, Encode};
	use frame_support::{CloneNoBound, DebugNoBound, PartialEqNoBound};
	use scale_info::TypeInfo;
	#[cfg(feature = "std")]
	use serde::{Deserialize, Serialize};
	use sp_core::{H160, H256, U256};
	use sp_std::prelude::*;

	/// ExecutionPayloadHeader
	/// <https://github.com/ethereum/consensus-specs/blob/master/specs/deneb/beacon-chain.md#executionpayloadheader>
	#[derive(
		Default,
		Encode,
		Decode,
		DecodeWithMemTracking,
		CloneNoBound,
		PartialEqNoBound,
		DebugNoBound,
		TypeInfo,
	)]
	#[cfg_attr(
		feature = "std",
		derive(Serialize, Deserialize),
		serde(deny_unknown_fields, bound(serialize = ""), bound(deserialize = ""))
	)]
	#[codec(mel_bound())]
	pub struct ExecutionPayloadHeader {
		pub parent_hash: H256,
		pub fee_recipient: H160,
		pub state_root: H256,
		pub receipts_root: H256,
		#[cfg_attr(
			feature = "std",
			serde(deserialize_with = "crate::serde_utils::from_hex_to_bytes")
		)]
		pub logs_bloom: Vec<u8>,
		pub prev_randao: H256,
		pub block_number: u64,
		pub gas_limit: u64,
		pub gas_used: u64,
		pub timestamp: u64,
		#[cfg_attr(
			feature = "std",
			serde(deserialize_with = "crate::serde_utils::from_hex_to_bytes")
		)]
		pub extra_data: Vec<u8>,
		#[cfg_attr(
			feature = "std",
			serde(deserialize_with = "crate::serde_utils::from_int_to_u256")
		)]
		pub base_fee_per_gas: U256,
		pub block_hash: H256,
		pub transactions_root: H256,
		pub withdrawals_root: H256,
		pub blob_gas_used: u64,   // [New in Deneb:EIP4844]
		pub excess_blob_gas: u64, // [New in Deneb:EIP4844]
	}
}

#[cfg(test)]
mod gloas_execution_header_tests {
	use super::*;
	use hex_literal::hex;

	/// Ethereum mainnet block 22020096, fetched from a public node and RLP-encoded.
	///
	/// Ground truth, not a synthetic fixture: `keccak256` of these bytes equals the real
	/// block hash below, which `block_hash_matches_mainnet` asserts. A parser bug and an
	/// encoder bug cannot cancel out here, because the chain fixed the hash.
	const HEADER_RLP: [u8; 604] = hex!(
		"f90259a0f22a42f6854bb46481bb54471991a515518ff7bc1de393e156348e13"
		"f0041794a01dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142"
		"fd40d493479495222290dd7278aa3ddd389cc1e1d165cc4bafe5a0b49b609c8a"
		"1fa32ea876c75277228cc49e565208e046ddd5dd489693fc8da7d3a0d93270f6"
		"bac2141b5ef4a4cab2c53841a33ca6c97805083c5bdfaca87dc975d6a0676ddf"
		"6e20e71df49c805f4d5592a53290c8bbdb62b65ceb95ce27d720d1e453b90100"
		"ffbfff7fe7fffffeb7fbdf6fe6dbfd7dffffffcffd5ffbfcffdef3ff9fd77fff"
		"fe4fdfffffeffb8cfffbffff3ffffffffb37f0fdfdbfffff7ffeff5ffdffffff"
		"e7bfff78f7effffdebe7f5fffefdfeff7eb73ff7fbffdffefbf7fffbf7fcffbf"
		"fdfffeffffff5bbeffffff57fffefffffffefffdcf7ffebff7ffffffdf7ffdfa"
		"fefffffffffffef7eddfef7ffffff7befb77f7fffffbffffffeffbffbfffeddf"
		"f7ff5feefff7fffffefedfeffddffdedffff9ffffefff5befffbb7fffffff8ff"
		"dbfddddbfff9fdfffff78fffeede7fffeffd7f7effffefffdf7fef7ff7f5ffff"
		"fbffffdfffffffff8ffffff7ff7e3bf7dffdfbffefefffffe5fffdffff7dffbf"
		"80840150000084021ff63383e50a3c8467cf85db8f6265617665726275696c64"
		"2e6f7267a0f3e1b797664817853ef750acf790fb97cf4d355213a347ad8cc7c7"
		"5fc757d2e988000000000000000085020060d51ca067941095f427d953a4a028"
		"b48c88945a533acbf8f022e520ef5e1ee38af120fe808403860000a0f5c72aed"
		"88b42fd7e07063dbdc1f0303a866bac45cc45d968ccc9b84b74d9748"	);
	const BLOCK_HASH: [u8; 32] =
		hex!("b634e83cfc769ae4ce0808ca48f4ae2b564a3e27e7cd87cc6b0a3d4f66b494d2");
	const RECEIPTS_ROOT: [u8; 32] =
		hex!("676ddf6e20e71df49c805f4d5592a53290c8bbdb62b65ceb95ce27d720d1e453");

	fn gloas(bytes: &[u8]) -> VersionedExecutionPayloadHeader {
		VersionedExecutionPayloadHeader::Gloas(
			bytes.to_vec().try_into().expect("fits the bound; qed"),
		)
	}

	/// Anchors the fixture: these bytes really are that block's header.
	#[test]
	fn block_hash_matches_mainnet() {
		assert_eq!(keccak_256(&HEADER_RLP), BLOCK_HASH);
	}

	#[test]
	fn extracts_receipts_root_from_a_real_header() {
		assert_eq!(receipts_root_from_rlp(&HEADER_RLP), Some(H256::from(RECEIPTS_ROOT)));
	}

	#[test]
	fn commitment_leaf_is_the_block_hash() {
		let commitment = gloas(&HEADER_RLP).commitment().unwrap();
		assert_eq!(commitment.leaf(), H256::from(BLOCK_HASH));
		assert_eq!(commitment.scheme(), CommitmentScheme::BlockHash);
		assert!(commitment.is_gloas());
		assert_eq!(commitment.receipts_root_once_proven(), H256::from(RECEIPTS_ROOT));
	}

	/// Altering any byte changes the leaf, so an altered header cannot be proven even
	/// though it still parses. This is what makes the parse safe to do before verifying.
	#[test]
	fn altering_a_byte_changes_the_leaf() {
		let mut altered = HEADER_RLP.to_vec();
		let last = altered.len() - 1;
		altered[last] ^= 0x01;
		assert_ne!(gloas(&altered).commitment().unwrap().leaf(), H256::from(BLOCK_HASH));
	}

	#[test]
	fn rejects_trailing_bytes() {
		let mut trailing = HEADER_RLP.to_vec();
		trailing.push(0x00);
		assert_eq!(receipts_root_from_rlp(&trailing), None);
	}

	#[test]
	fn rejects_truncated_header() {
		assert_eq!(receipts_root_from_rlp(&HEADER_RLP[..HEADER_RLP.len() - 1]), None);
	}

	#[test]
	fn rejects_non_list() {
		// A 32-byte string, not a list.
		let mut bytes = vec![0xa0];
		bytes.extend_from_slice(&RECEIPTS_ROOT);
		assert_eq!(receipts_root_from_rlp(&bytes), None);
	}

	#[test]
	fn rejects_empty_input() {
		assert_eq!(receipts_root_from_rlp(&[]), None);
	}

	/// Builds a header-shaped list so the field walk can be exercised directly.
	fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
		fn header(len: usize, offset: u8) -> Vec<u8> {
			if len < 56 {
				return vec![offset + len as u8];
			}
			let be = len.to_be_bytes();
			let first = be.iter().position(|b| *b != 0).unwrap();
			let mut out = vec![offset + 55 + (be.len() - first) as u8];
			out.extend_from_slice(&be[first..]);
			out
		}
		fn item(bytes: &[u8]) -> Vec<u8> {
			if bytes.len() == 1 && bytes[0] < 0x80 {
				return bytes.to_vec();
			}
			let mut out = header(bytes.len(), 0x80);
			out.extend_from_slice(bytes);
			out
		}
		let payload: Vec<u8> = items.iter().flat_map(|i| item(i)).collect();
		let mut out = header(payload.len(), 0xc0);
		out.extend_from_slice(&payload);
		out
	}

	fn header_fields() -> Vec<Vec<u8>> {
		let mut fields: Vec<Vec<u8>> = (0..5).map(|_| vec![0x11; 32]).collect();
		fields.push(RECEIPTS_ROOT.to_vec());
		fields
	}

	/// Fields after the receipts root are frequently small integers, which RLP encodes as a
	/// bare byte carrying no length prefix. If the walk mishandled that, the field count
	/// would desync and the minimum-field check would pass or fail for the wrong reason.
	#[test]
	fn walks_single_byte_and_empty_items() {
		let mut fields = header_fields();
		// difficulty = 0 (empty string), then small integers, then a zero byte.
		fields.push(vec![]);
		for n in 1u8..=8 {
			fields.push(vec![n]);
		}
		fields.push(vec![0x00]);
		assert_eq!(fields.len(), 16);
		assert_eq!(receipts_root_from_rlp(&rlp_list(&fields)), Some(H256::from(RECEIPTS_ROOT)));
	}

	#[test]
	fn rejects_too_few_fields() {
		let fields = header_fields();
		assert_eq!(fields.len(), 6);
		assert_eq!(receipts_root_from_rlp(&rlp_list(&fields)), None);
	}

	#[test]
	fn rejects_receipts_root_of_wrong_length() {
		let mut fields = header_fields();
		fields[5] = vec![0x22; 31];
		fields.extend((0..10).map(|_| vec![0x01]));
		assert_eq!(receipts_root_from_rlp(&rlp_list(&fields)), None);
	}

	#[test]
	fn rejects_receipts_root_that_is_a_list() {
		// Element 5 is a nested list rather than a 32-byte string.
		let mut fields = header_fields();
		fields.extend((0..10).map(|_| vec![0x01]));
		let mut encoded = rlp_list(&fields);
		// Locate element 5 by its contents, then turn its 32-byte string prefix into a
		// nested-list prefix of the same length.
		let at = encoded
			.windows(RECEIPTS_ROOT.len())
			.position(|w| w == RECEIPTS_ROOT)
			.expect("receipts root is present; qed") -
			1;
		assert_eq!(encoded[at], 0xa0);
		encoded[at] = 0xe0;
		assert_eq!(receipts_root_from_rlp(&encoded), None);
	}

	/// Wraps already-encoded items in a list header, so a test can hand-craft item
	/// encodings that `rlp_list` would never produce.
	fn rlp_list_raw(payload: &[u8]) -> Vec<u8> {
		let mut out = if payload.len() < 56 {
			vec![0xc0 + payload.len() as u8]
		} else {
			let be = payload.len().to_be_bytes();
			let first = be.iter().position(|b| *b != 0).expect("non-empty; qed");
			let mut h = vec![0xf7 + (be.len() - first) as u8];
			h.extend_from_slice(&be[first..]);
			h
		};
		out.extend_from_slice(payload);
		out
	}

	/// Six canonical leading items, with the receipts root at index 5.
	fn canonical_prefix() -> Vec<u8> {
		let mut payload = Vec::new();
		for _ in 0..5 {
			payload.push(0xa0);
			payload.extend_from_slice(&[0x11; 32]);
		}
		payload.push(0xa0);
		payload.extend_from_slice(&RECEIPTS_ROOT);
		payload
	}

	/// The parser leans on `alloy_rlp` to reject non-canonical encodings rather than
	/// checking them itself, so that reliance is worth pinning.
	#[test]
	fn rejects_non_canonical_item_encodings() {
		// 0x81 0x01 is a non-canonical encoding of the single byte 0x01.
		let mut bad = canonical_prefix();
		bad.extend_from_slice(&[0x81, 0x01]);
		bad.extend(core::iter::repeat(0x01).take(9));
		assert_eq!(receipts_root_from_rlp(&rlp_list_raw(&bad)), None);

		// 0xb8 0x03 is the long form for a 3-byte string, which must use 0x83.
		let mut bad = canonical_prefix();
		bad.extend_from_slice(&[0xb8, 0x03, 0xaa, 0xbb, 0xcc]);
		bad.extend(core::iter::repeat(0x01).take(9));
		assert_eq!(receipts_root_from_rlp(&rlp_list_raw(&bad)), None);

		// The same shapes, canonically encoded, parse.
		let mut good = canonical_prefix();
		good.push(0x01);
		good.extend_from_slice(&[0x83, 0xaa, 0xbb, 0xcc]);
		good.extend(core::iter::repeat(0x01).take(8));
		assert_eq!(receipts_root_from_rlp(&rlp_list_raw(&good)), Some(H256::from(RECEIPTS_ROOT)));
	}

	/// An empty list terminates the walk immediately and must not be read as a header.
	#[test]
	fn rejects_empty_list() {
		assert_eq!(receipts_root_from_rlp(&[0xc0]), None);
	}

	/// Every item form consumes at least one byte, so the walk always terminates. Empty
	/// strings (0x80) and empty lists (0xc0) are the degenerate cases.
	#[test]
	fn walk_terminates_on_zero_length_items() {
		let mut payload = canonical_prefix();
		payload.extend(core::iter::repeat(0x80).take(5)); // empty strings
		payload.extend(core::iter::repeat(0xc0).take(5)); // empty lists
		assert_eq!(
			receipts_root_from_rlp(&rlp_list_raw(&payload)),
			Some(H256::from(RECEIPTS_ROOT))
		);
	}
}
