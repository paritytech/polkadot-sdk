// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.23;

import {BeefyClient} from "../../src/BeefyClient.sol";
import {Uint16Array, createUint16Array} from "../../src/utils/Uint16Array.sol";
import "forge-std/console.sol";

contract BeefyClientMock is BeefyClient {
    constructor(uint256 randaoCommitDelay, uint256 randaoCommitExpiration, uint256 minNumRequiredSignatures)
        BeefyClient(
            randaoCommitDelay,
            randaoCommitExpiration,
            minNumRequiredSignatures,
            0,
            BeefyClient.ValidatorSet(0, 0, 0x0),
            BeefyClient.ValidatorSet(1, 0, 0x0)
        )
    {}

    function encodeCommitment_public(Commitment calldata commitment) external pure returns (bytes memory) {
        return encodeCommitment(commitment);
    }

    function setTicketValidatorSetLen(bytes32 commitmentHash, uint32 validatorSetLen) external {
        tickets[createTicketID(msg.sender, commitmentHash)].validatorSetLen = validatorSetLen;
    }

    function setLatestBeefyBlock(uint32 _latestBeefyBlock) external {
        latestBeefyBlock = _latestBeefyBlock;
    }

    function initialize_public(
        uint64 _initialBeefyBlock,
        ValidatorSet calldata _initialValidatorSet,
        ValidatorSet calldata _nextValidatorSet
    ) external {
        latestBeefyBlock = _initialBeefyBlock;
        currentValidatorSet.id = _initialValidatorSet.id;
        currentValidatorSet.length = _initialValidatorSet.length;
        currentValidatorSet.root = _initialValidatorSet.root;
        currentValidatorSet.usageCounters = createUint16Array(currentValidatorSet.length);
        nextValidatorSet.id = _nextValidatorSet.id;
        nextValidatorSet.length = _nextValidatorSet.length;
        nextValidatorSet.root = _nextValidatorSet.root;
        nextValidatorSet.usageCounters = createUint16Array(nextValidatorSet.length);
        console.log(currentValidatorSet.usageCounters.data.length);
    }

    // Used to verify integrity of storage to storage copies
    function copyCounters() external {
        currentValidatorSet.usageCounters = createUint16Array(1000);
        for (uint256 i = 0; i < 1000; i++) {
            currentValidatorSet.usageCounters.set(i, 5);
        }
        nextValidatorSet.usageCounters = createUint16Array(800);
        for (uint256 i = 0; i < 800; i++) {
            nextValidatorSet.usageCounters.set(i, 7);
        }

        // Perform the copy
        currentValidatorSet = nextValidatorSet;

        assert(currentValidatorSet.usageCounters.data.length == nextValidatorSet.usageCounters.data.length);
        assert(currentValidatorSet.usageCounters.get(799) == 7);
    }

    function getValidatorCounter(bool next, uint256 index) public view returns (uint16) {
        if (next) {
            return nextValidatorSet.usageCounters.get(index);
        }
        return currentValidatorSet.usageCounters.get(index);
    }

    function computeNumRequiredSignatures_public(
        uint256 validatorSetLen,
        uint256 signatureUsageCount,
        uint256 minSignatures
    ) public pure returns (uint256) {
        return computeNumRequiredSignatures(validatorSetLen, signatureUsageCount, minSignatures);
    }

    function computeQuorum_public(uint256 numValidators) public pure returns (uint256) {
        return computeQuorum(numValidators);
    }
}
