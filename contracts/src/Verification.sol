// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pragma solidity 0.8.23;

import {SubstrateMerkleProof} from "./utils/SubstrateMerkleProof.sol";
import {BeefyClient} from "./BeefyClient.sol";
import {ScaleCodec} from "./utils/ScaleCodec.sol";
import {SubstrateTypes} from "./SubstrateTypes.sol";

library Verification {
    /// @dev Merkle proof for parachain header finalized by BEEFY
    /// Reference: https://github.com/paritytech/polkadot/blob/09b61286da11921a3dda0a8e4015ceb9ef9cffca/runtime/rococo/src/lib.rs#L1312
    struct HeadProof {
        // The leaf index of the parachain being proven
        uint256 pos;
        // The number of leaves in the merkle tree
        uint256 width;
        // The proof items
        bytes32[] proof;
    }

    /// @dev An MMRLeaf without the `leaf_extra` field.
    /// Reference: https://github.com/paritytech/substrate/blob/14e0a0b628f9154c5a2c870062c3aac7df8983ed/primitives/consensus/beefy/src/mmr.rs#L52
    struct MMRLeafPartial {
        uint8 version;
        uint32 parentNumber;
        bytes32 parentHash;
        uint64 nextAuthoritySetID;
        uint32 nextAuthoritySetLen;
        bytes32 nextAuthoritySetRoot;
    }

    /// @dev Parachain header
    /// References:
    /// * https://paritytech.github.io/substrate/master/sp_runtime/generic/struct.Header.html
    /// * https://github.com/paritytech/substrate/blob/14e0a0b628f9154c5a2c870062c3aac7df8983ed/primitives/runtime/src/generic/header.rs#L41
    struct ParachainHeader {
        bytes32 parentHash;
        uint256 number;
        bytes32 stateRoot;
        bytes32 extrinsicsRoot;
        DigestItem[] digestItems;
    }

    /// @dev Represents a digest item within a parachain header.
    /// References:
    /// * https://paritytech.github.io/substrate/master/sp_runtime/generic/enum.DigestItem.html
    /// * https://github.com/paritytech/substrate/blob/14e0a0b628f9154c5a2c870062c3aac7df8983ed/primitives/runtime/src/generic/digest.rs#L75
    struct DigestItem {
        uint256 kind;
        bytes4 consensusEngineID;
        bytes data;
    }

    /// @dev A chain of proofs
    struct Proof {
        // The parachain header containing the message commitment as a digest item
        ParachainHeader header;
        // The proof used to generate a merkle root of parachain heads
        HeadProof headProof;
        // The MMR leaf to be proven
        MMRLeafPartial leafPartial;
        // The MMR leaf prove
        bytes32[] leafProof;
        // The order in which proof items should be combined
        uint256 leafProofOrder;
    }

    error InvalidParachainHeader();

    /// @dev IDs of enum variants of DigestItem
    /// Reference: https://github.com/paritytech/substrate/blob/14e0a0b628f9154c5a2c870062c3aac7df8983ed/primitives/runtime/src/generic/digest.rs#L201
    uint256 public constant DIGEST_ITEM_OTHER = 0;
    uint256 public constant DIGEST_ITEM_CONSENSUS = 4;
    uint256 public constant DIGEST_ITEM_SEAL = 5;
    uint256 public constant DIGEST_ITEM_PRERUNTIME = 6;
    uint256 public constant DIGEST_ITEM_RUNTIME_ENVIRONMENT_UPDATED = 8;

    /// @dev Enum variant ID for CustomDigestItem::Snowbridge
    bytes1 public constant DIGEST_ITEM_OTHER_SNOWBRIDGE = 0x00;

    /// @dev Verify the message commitment by applying several proofs
    ///
    /// 1. First check that the commitment is included in the digest items of the parachain header
    /// 2. Generate the root of the parachain heads merkle tree
    /// 3. Construct an MMR leaf containing the parachain heads root.
    /// 4. Verify that the MMR leaf is included in the MMR maintained by the BEEFY light client.
    ///
    /// Background info:
    ///
    /// In the Polkadot relay chain, for every block:
    /// 1. A merkle root of finalized parachain headers is constructed:
    ///    https://github.com/paritytech/polkadot/blob/09b61286da11921a3dda0a8e4015ceb9ef9cffca/runtime/rococo/src/lib.rs#L1312.
    /// 2. An MMR leaf is produced, containing this parachain headers root, and is then inserted into the
    ///    MMR maintained by the `merkle-mountain-range` pallet:
    ///    https://github.com/paritytech/substrate/tree/master/frame/merkle-mountain-range
    ///
    /// @param beefyClient The address of the BEEFY light client
    /// @param encodedParaID The SCALE-encoded parachain ID of BridgeHub
    /// @param commitment The message commitment root expected to be contained within the
    ///                   digest of BridgeHub parachain header.
    /// @param proof The chain of proofs described above
    function verifyCommitment(address beefyClient, bytes4 encodedParaID, bytes32 commitment, Proof calldata proof)
        external
        view
        returns (bool)
    {
        // Verify that parachain header contains the commitment
        if (!isCommitmentInHeaderDigest(commitment, proof.header)) {
            return false;
        }

        // Compute the merkle leaf hash of our parachain
        bytes32 parachainHeadHash = createParachainHeaderMerkleLeaf(encodedParaID, proof.header);

        if (proof.headProof.pos >= proof.headProof.width) {
            return false;
        }

        // Compute the merkle root hash of all parachain heads
        bytes32 parachainHeadsRoot = SubstrateMerkleProof.computeRoot(
            parachainHeadHash, proof.headProof.pos, proof.headProof.width, proof.headProof.proof
        );

        bytes32 leafHash = createMMRLeaf(proof.leafPartial, parachainHeadsRoot);

        // Verify that the MMR leaf is part of the MMR maintained by the BEEFY light client
        return BeefyClient(beefyClient).verifyMMRLeafProof(leafHash, proof.leafProof, proof.leafProofOrder);
    }

    // Verify that a message commitment is in the header digest
    function isCommitmentInHeaderDigest(bytes32 commitment, ParachainHeader calldata header)
        internal
        pure
        returns (bool)
    {
        for (uint256 i = 0; i < header.digestItems.length; i++) {
            if (
                header.digestItems[i].kind == DIGEST_ITEM_OTHER && header.digestItems[i].data.length == 33
                    && header.digestItems[i].data[0] == DIGEST_ITEM_OTHER_SNOWBRIDGE
                    && commitment == bytes32(header.digestItems[i].data[1:])
            ) {
                return true;
            }
        }
        return false;
    }

    // SCALE-Encodes: Vec<DigestItem>
    // Reference: https://github.com/paritytech/substrate/blob/14e0a0b628f9154c5a2c870062c3aac7df8983ed/primitives/runtime/src/generic/digest.rs#L40
    function encodeDigestItems(DigestItem[] calldata digestItems) internal pure returns (bytes memory) {
        // encode all digest items into a buffer
        bytes memory accum = hex"";
        for (uint256 i = 0; i < digestItems.length; i++) {
            accum = bytes.concat(accum, encodeDigestItem(digestItems[i]));
        }
        // Encode number of digest items, followed by encoded digest items
        return bytes.concat(ScaleCodec.checkedEncodeCompactU32(digestItems.length), accum);
    }

    function encodeDigestItem(DigestItem calldata digestItem) internal pure returns (bytes memory) {
        if (
            digestItem.kind == DIGEST_ITEM_PRERUNTIME || digestItem.kind == DIGEST_ITEM_CONSENSUS
                || digestItem.kind == DIGEST_ITEM_SEAL
        ) {
            return bytes.concat(
                bytes1(uint8(digestItem.kind)),
                digestItem.consensusEngineID,
                ScaleCodec.checkedEncodeCompactU32(digestItem.data.length),
                digestItem.data
            );
        } else if (digestItem.kind == DIGEST_ITEM_OTHER) {
            return bytes.concat(
                bytes1(uint8(DIGEST_ITEM_OTHER)),
                ScaleCodec.checkedEncodeCompactU32(digestItem.data.length),
                digestItem.data
            );
        } else if (digestItem.kind == DIGEST_ITEM_RUNTIME_ENVIRONMENT_UPDATED) {
            return bytes.concat(bytes1(uint8(DIGEST_ITEM_RUNTIME_ENVIRONMENT_UPDATED)));
        } else {
            revert InvalidParachainHeader();
        }
    }

    // Creates a keccak hash of a SCALE-encoded parachain header
    function createParachainHeaderMerkleLeaf(bytes4 encodedParaID, ParachainHeader calldata header)
        internal
        pure
        returns (bytes32)
    {
        // Hash of encoded parachain header merkle leaf
        return keccak256(createParachainHeader(encodedParaID, header));
    }

    function createParachainHeader(bytes4 encodedParaID, ParachainHeader calldata header)
        internal
        pure
        returns (bytes memory)
    {
        bytes memory encodedHeader = bytes.concat(
            // H256
            header.parentHash,
            // Compact unsigned int
            ScaleCodec.checkedEncodeCompactU32(header.number),
            // H256
            header.stateRoot,
            // H256
            header.extrinsicsRoot,
            // Vec<DigestItem>
            encodeDigestItems(header.digestItems)
        );

        return bytes.concat(
            // u32
            encodedParaID,
            // length of encoded header
            ScaleCodec.checkedEncodeCompactU32(encodedHeader.length),
            encodedHeader
        );
    }

    // SCALE-encode: MMRLeaf
    // Reference: https://github.com/paritytech/substrate/blob/14e0a0b628f9154c5a2c870062c3aac7df8983ed/primitives/consensus/beefy/src/mmr.rs#L52
    function createMMRLeaf(MMRLeafPartial memory leaf, bytes32 parachainHeadsRoot) internal pure returns (bytes32) {
        bytes memory encodedLeaf = bytes.concat(
            ScaleCodec.encodeU8(leaf.version),
            ScaleCodec.encodeU32(leaf.parentNumber),
            leaf.parentHash,
            ScaleCodec.encodeU64(leaf.nextAuthoritySetID),
            ScaleCodec.encodeU32(leaf.nextAuthoritySetLen),
            leaf.nextAuthoritySetRoot,
            parachainHeadsRoot
        );
        return keccak256(encodedLeaf);
    }
}
