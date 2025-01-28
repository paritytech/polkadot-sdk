// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Primitives that are used to interact with BEEFY bridge pallet.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

pub use binary_merkle_tree::merkle_root;
pub use pallet_beefy_mmr::BeefyEcdsaToEthereum;
pub use pallet_mmr::{
	primitives::{DataOrHash as MmrDataOrHash, LeafProof as MmrProof},
	verify_leaves_proof as verify_mmr_leaves_proof,
};
pub use sp_consensus_beefy::{
	ecdsa_crypto::{
		AuthorityId as EcdsaValidatorId, AuthoritySignature as EcdsaValidatorSignature,
	},
	known_payloads::MMR_ROOT_ID as MMR_ROOT_PAYLOAD_ID,
	mmr::{BeefyAuthoritySet, MmrLeafVersion},
	BeefyAuthorityId, Commitment, Payload as BeefyPayload, SignedCommitment, ValidatorSet,
	ValidatorSetId, BEEFY_ENGINE_ID,
};

use bp_runtime::{BasicOperatingMode, BlockNumberOf, Chain, HashOf};
use codec::{Decode, Encode};
use frame_support::Parameter;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::{
	traits::{Convert, MaybeSerializeDeserialize},
	RuntimeAppPublic, RuntimeDebug,
};
use sp_std::prelude::*;

/// Substrate-based chain with BEEFY && MMR pallets deployed.
///
/// Both BEEFY and MMR pallets and their clients may be configured to use different
/// primitives. Some of types can be configured in low-level pallets, but are constrained
/// when BEEFY+MMR bundle is used.
pub trait ChainWithBeefy: Chain {
	/// The hashing algorithm used to compute the digest of the BEEFY commitment.
	///
	/// Corresponds to the hashing algorithm, used by `sc_consensus_beefy::BeefyKeystore`.
	type CommitmentHasher: sp_runtime::traits::Hash;

	/// The hashing algorithm used to build the MMR.
	///
	/// The same algorithm is also used to compute merkle roots in BEEFY
	/// (e.g. validator addresses root in leaf data).
	///
	/// Corresponds to the `Hashing` field of the `pallet-mmr` configuration.
	type MmrHashing: sp_runtime::traits::Hash<Output = Self::MmrHash>;

	/// The output type of the hashing algorithm used to build the MMR.
	///
	/// This type is actually stored in the MMR.

	/// Corresponds to the `Hash` field of the `pallet-mmr` configuration.
	type MmrHash: sp_std::hash::Hash
		+ Parameter
		+ Copy
		+ AsRef<[u8]>
		+ Default
		+ MaybeSerializeDeserialize
		+ PartialOrd;

	/// The type expected for the MMR leaf extra data.
	type BeefyMmrLeafExtra: Parameter;

	/// A way to identify a BEEFY validator.
	///
	/// Corresponds to the `BeefyId` field of the `pallet-beefy` configuration.
	type AuthorityId: BeefyAuthorityId<Self::CommitmentHasher> + Parameter;

	/// A way to convert validator id to its raw representation in the BEEFY merkle tree.
	///
	/// Corresponds to the `BeefyAuthorityToMerkleLeaf` field of the `pallet-beefy-mmr`
	/// configuration.
	type AuthorityIdToMerkleLeaf: Convert<Self::AuthorityId, Vec<u8>>;
}

/// BEEFY validator id used by given Substrate chain.
pub type BeefyAuthorityIdOf<C> = <C as ChainWithBeefy>::AuthorityId;
/// BEEFY validator set, containing both validator identifiers and the numeric set id.
pub type BeefyAuthoritySetOf<C> = ValidatorSet<BeefyAuthorityIdOf<C>>;
/// BEEFY authority set, containing both validator identifiers and the numeric set id.
pub type BeefyAuthoritySetInfoOf<C> = sp_consensus_beefy::mmr::BeefyAuthoritySet<MmrHashOf<C>>;
/// BEEFY validator signature used by given Substrate chain.
pub type BeefyValidatorSignatureOf<C> =
	<<C as ChainWithBeefy>::AuthorityId as RuntimeAppPublic>::Signature;
/// Signed BEEFY commitment used by given Substrate chain.
pub type BeefySignedCommitmentOf<C> =
	SignedCommitment<BlockNumberOf<C>, BeefyValidatorSignatureOf<C>>;
/// Hash algorithm, used to compute the digest of the BEEFY commitment before signing it.
pub type BeefyCommitmentHasher<C> = <C as ChainWithBeefy>::CommitmentHasher;
/// Hash algorithm used in Beefy MMR construction by given Substrate chain.
pub type MmrHashingOf<C> = <C as ChainWithBeefy>::MmrHashing;
/// Hash type, used in MMR construction by given Substrate chain.
pub type MmrHashOf<C> = <C as ChainWithBeefy>::MmrHash;
/// BEEFY MMR proof type used by the given Substrate chain.
pub type MmrProofOf<C> = MmrProof<MmrHashOf<C>>;
/// The type of the MMR leaf extra data used by the given Substrate chain.
pub type BeefyMmrLeafExtraOf<C> = <C as ChainWithBeefy>::BeefyMmrLeafExtra;
/// A way to convert a validator id to its raw representation in the BEEFY merkle tree, used by
/// the given Substrate chain.
pub type BeefyAuthorityIdToMerkleLeafOf<C> = <C as ChainWithBeefy>::AuthorityIdToMerkleLeaf;
/// Actual type of leafs in the BEEFY MMR.
pub type BeefyMmrLeafOf<C> = sp_consensus_beefy::mmr::MmrLeaf<
	BlockNumberOf<C>,
	HashOf<C>,
	MmrHashOf<C>,
	BeefyMmrLeafExtraOf<C>,
>;

/// Data required for initializing the BEEFY pallet.
///
/// Provides the initial context that the bridge needs in order to know
/// where to start the sync process from.
#[derive(Encode, Decode, RuntimeDebug, PartialEq, Clone, TypeInfo, Serialize, Deserialize)]
pub struct InitializationData<BlockNumber, Hash> {
	/// Pallet operating mode.
	pub operating_mode: BasicOperatingMode,
	/// Number of the best block, finalized by BEEFY.
	pub best_block_number: BlockNumber,
	/// BEEFY authority set that will be finalizing descendants of the `best_beefy_block_number`
	/// block.
	pub authority_set: BeefyAuthoritySet<Hash>,
}

/// Basic data, stored by the pallet for every imported commitment.
#[derive(Encode, Decode, RuntimeDebug, PartialEq, TypeInfo)]
pub struct ImportedCommitment<BlockNumber, BlockHash, MmrHash> {
	/// Block number and hash of the finalized block parent.
	pub parent_number_and_hash: (BlockNumber, BlockHash),
	/// MMR root at the imported block.
	pub mmr_root: MmrHash,
}
