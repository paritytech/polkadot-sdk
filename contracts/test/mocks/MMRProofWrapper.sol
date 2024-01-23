// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.23;

import {MMRProof} from "../../src/utils/MMRProof.sol";

contract MMRProofWrapper {
    function verifyLeafProof(bytes32 root, bytes32 leafHash, bytes32[] calldata proof, uint256 proofOrder)
        external
        pure
        returns (bool)
    {
        return MMRProof.verifyLeafProof(root, leafHash, proof, proofOrder);
    }
}
