// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.23;

import {Verification} from "../../src/Verification.sol";

contract VerificationWrapper {
    function createParachainHeaderMerkleLeaf(bytes4 encodedParachainID, Verification.ParachainHeader calldata header)
        external
        pure
        returns (bytes32)
    {
        return Verification.createParachainHeaderMerkleLeaf(encodedParachainID, header);
    }

    function isCommitmentInHeaderDigest(bytes32 commitment, Verification.ParachainHeader calldata header)
        external
        pure
        returns (bool)
    {
        return Verification.isCommitmentInHeaderDigest(commitment, header);
    }
}
