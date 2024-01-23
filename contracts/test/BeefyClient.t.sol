// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.23;

import {Strings} from "openzeppelin/utils/Strings.sol";
import {Test} from "forge-std/Test.sol";
import {console} from "forge-std/console.sol";
import {stdJson} from "forge-std/StdJson.sol";

import {BeefyClient} from "../src/BeefyClient.sol";
import {BeefyClientMock} from "./mocks/BeefyClientMock.sol";
import {ScaleCodec} from "../src/utils/ScaleCodec.sol";
import {Bitfield} from "../src/utils/Bitfield.sol";

contract BeefyClientTest is Test {
    using stdJson for string;

    BeefyClientMock beefyClient;
    uint8 randaoCommitDelay;
    uint8 randaoCommitExpiration;
    uint256 minNumRequiredSignatures;
    uint32 blockNumber;
    uint32 prevRandao;
    uint32 setSize;
    uint32 setId;
    uint128 currentSetId;
    uint128 nextSetId;
    bytes32 commitHash;
    bytes32 root;
    uint256[] bitSetArray;
    uint256[] absentBitSetArray;
    uint256[] bitfield;
    uint256[] absentBitfield;
    bytes32 mmrRoot;
    uint256[] finalBitfield;
    BeefyClient.ValidatorProof[] finalValidatorProofs;
    BeefyClient.ValidatorProof[] finalValidatorProofs3SignatureCount;
    bytes32[] mmrLeafProofs;
    BeefyClient.MMRLeaf mmrLeaf;
    uint256 leafProofOrder;
    BeefyClient.MMRLeaf emptyLeaf;
    bytes32[] emptyLeafProofs;
    uint256 emptyLeafProofOrder;
    bytes2 mmrRootID = bytes2("mh");
    string bitFieldFile0SignatureCount;
    string bitFieldFile3SignatureCount;

    function setUp() public {
        randaoCommitDelay = uint8(vm.envOr("RANDAO_COMMIT_DELAY", uint256(3)));
        randaoCommitExpiration = uint8(vm.envOr("RANDAO_COMMIT_EXP", uint256(8)));
        minNumRequiredSignatures = uint8(vm.envOr("MINIMUM_REQUIRED_SIGNATURES", uint256(16)));
        prevRandao = uint32(vm.envOr("PREV_RANDAO", uint256(377)));

        string memory beefyCommitmentFile = string.concat(vm.projectRoot(), "/test/data/beefy-commitment.json");

        string memory beefyCommitmentRaw = vm.readFile(beefyCommitmentFile);

        bitFieldFile0SignatureCount = string.concat(vm.projectRoot(), "/test/data/beefy-final-bitfield-0.json");
        bitFieldFile3SignatureCount = string.concat(vm.projectRoot(), "/test/data/beefy-final-bitfield-3.json");

        blockNumber = uint32(beefyCommitmentRaw.readUint(".params.commitment.blockNumber"));
        setId = uint32(beefyCommitmentRaw.readUint(".params.commitment.validatorSetID"));
        commitHash = beefyCommitmentRaw.readBytes32(".commitmentHash");
        mmrRoot = beefyCommitmentRaw.readBytes32(".params.commitment.payload[0].data");
        mmrLeafProofs = beefyCommitmentRaw.readBytes32Array(".params.leafProof");
        leafProofOrder = beefyCommitmentRaw.readUint(".params.leafProofOrder");
        decodeMMRLeaf(beefyCommitmentRaw);

        string memory beefyValidatorSetFile = string.concat(vm.projectRoot(), "/test/data/beefy-validator-set.json");
        string memory beefyValidatorSetRaw = vm.readFile(beefyValidatorSetFile);
        setSize = uint32(beefyValidatorSetRaw.readUint(".validatorSetSize"));
        root = beefyValidatorSetRaw.readBytes32(".validatorRoot");
        bitSetArray = beefyValidatorSetRaw.readUintArray(".participants");
        absentBitSetArray = beefyValidatorSetRaw.readUintArray(".absentees");

        console.log("current validator's merkle root is: %s", Strings.toHexString(uint256(root), 32));

        beefyClient = new BeefyClientMock(randaoCommitDelay, randaoCommitExpiration, minNumRequiredSignatures);

        bitfield = beefyClient.createInitialBitfield(bitSetArray, setSize);
        absentBitfield = beefyClient.createInitialBitfield(absentBitSetArray, setSize);

        string memory finalProofFile0SignatureCount =
            string.concat(vm.projectRoot(), "/test/data/beefy-final-proof-0.json");
        string memory finalProofRaw0SignatureCount = vm.readFile(finalProofFile0SignatureCount);
        loadFinalProofs(finalProofRaw0SignatureCount, finalValidatorProofs);

        string memory finalProofFile3SignatureCount =
            string.concat(vm.projectRoot(), "/test/data/beefy-final-proof-3.json");
        string memory finalProofRaw3SignatureCount = vm.readFile(finalProofFile3SignatureCount);
        loadFinalProofs(finalProofRaw3SignatureCount, finalValidatorProofs3SignatureCount);
    }

    function initialize(uint32 _setId) public returns (BeefyClient.Commitment memory) {
        currentSetId = _setId;
        nextSetId = _setId + 1;
        BeefyClient.ValidatorSet memory vset = BeefyClient.ValidatorSet(currentSetId, setSize, root);
        BeefyClient.ValidatorSet memory nextvset = BeefyClient.ValidatorSet(nextSetId, setSize, root);
        beefyClient.initialize_public(0, vset, nextvset);
        BeefyClient.PayloadItem[] memory payload = new BeefyClient.PayloadItem[](1);
        payload[0] = BeefyClient.PayloadItem(mmrRootID, abi.encodePacked(mmrRoot));
        return BeefyClient.Commitment(blockNumber, setId, payload);
    }

    function printBitArray(uint256[] memory bits) private view {
        for (uint256 i = 0; i < bits.length; i++) {
            console.log("bits index at %d is %d", i, bits[i]);
        }
    }

    function loadFinalProofs(string memory finalProofRaw, BeefyClient.ValidatorProof[] storage finalProofs) internal {
        bytes memory proofRaw = finalProofRaw.readBytes(".finalValidatorsProofRaw");
        BeefyClient.ValidatorProof[] memory proofs = abi.decode(proofRaw, (BeefyClient.ValidatorProof[]));
        for (uint256 i = 0; i < proofs.length; i++) {
            finalProofs.push(proofs[i]);
        }
    }

    // Ideally should also update `finalValidatorProofs` with another round of ffi based on the `finalBitfield` here
    // For simplicity we just use the proof previously cached
    // still update `finalBitfield` here is to simulate more close to the real workflow and make gas estimation more accurate
    function createFinalProofs() internal {
        finalBitfield = beefyClient.createFinalBitfield(commitHash, bitfield);
    }

    function commitPrevRandao() internal {
        vm.prevrandao(bytes32(uint256(prevRandao)));
        beefyClient.commitPrevRandao(commitHash);
    }

    // Regenerate bitField file
    function regenerateBitField(string memory bitfieldFile, uint256 numRequiredSignatures) internal {
        console.log("print initialBitField");
        printBitArray(bitfield);
        prevRandao = uint32(vm.envOr("PREV_RANDAO", prevRandao));
        finalBitfield = Bitfield.subsample(prevRandao, bitfield, numRequiredSignatures, setSize);
        console.log("print finalBitField");
        printBitArray(finalBitfield);

        string memory finalBitFieldRaw = "";
        finalBitFieldRaw = finalBitFieldRaw.serialize("finalBitFieldRaw", abi.encode(finalBitfield));

        string memory finaliBitFieldStr = "";
        finaliBitFieldStr = finaliBitFieldStr.serialize("finalBitField", finalBitfield);

        string memory output = finalBitFieldRaw.serialize("final", finaliBitFieldStr);

        vm.writeJson(output, bitfieldFile);
    }

    function decodeMMRLeaf(string memory beefyCommitmentRaw) internal {
        uint8 version = uint8(beefyCommitmentRaw.readUint(".params.leaf.version"));
        uint32 parentNumber = uint32(beefyCommitmentRaw.readUint(".params.leaf.parentNumber"));
        bytes32 parentHash = beefyCommitmentRaw.readBytes32(".params.leaf.parentHash");
        uint64 nextAuthoritySetID = uint64(beefyCommitmentRaw.readUint(".params.leaf.nextAuthoritySetID"));
        uint32 nextAuthoritySetLen = uint32(beefyCommitmentRaw.readUint(".params.leaf.nextAuthoritySetLen"));
        bytes32 nextAuthoritySetRoot = beefyCommitmentRaw.readBytes32(".params.leaf.nextAuthoritySetRoot");
        bytes32 parachainHeadsRoot = beefyCommitmentRaw.readBytes32(".params.leaf.parachainHeadsRoot");
        mmrLeaf = BeefyClient.MMRLeaf(
            version,
            parentNumber,
            parentHash,
            nextAuthoritySetID,
            nextAuthoritySetLen,
            nextAuthoritySetRoot,
            parachainHeadsRoot
        );
    }

    function testSubmit() public returns (BeefyClient.Commitment memory) {
        BeefyClient.Commitment memory commitment = initialize(setId);

        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 0);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 1);

        // mine random delay blocks
        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        beefyClient.submitFinal(
            commitment, bitfield, finalValidatorProofs, emptyLeaf, emptyLeafProofs, emptyLeafProofOrder
        );

        assertEq(beefyClient.latestBeefyBlock(), blockNumber);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 1);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 0);
        return commitment;
    }

    function testSubmitWithOldBlockFailsWithStaleCommitment() public {
        BeefyClient.Commitment memory commitment = initialize(setId);
        beefyClient.setLatestBeefyBlock(commitment.blockNumber + 1);
        vm.expectRevert(BeefyClient.StaleCommitment.selector);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);
    }

    function testSubmitWithHandoverAndOldBlockFailsWithStaleCommitment() public {
        BeefyClient.Commitment memory commitment = initialize(setId - 1);
        beefyClient.setLatestBeefyBlock(commitment.blockNumber + 1);
        vm.expectRevert(BeefyClient.StaleCommitment.selector);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);
    }

    function testSubmitWith3SignatureCount() public returns (BeefyClient.Commitment memory) {
        BeefyClient.Commitment memory commitment = initialize(setId);

        // Signature count is 0 for the first submitInitial.
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 0);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // Signature count is now 1 after a second submitInitial.
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 1);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // Signature count is still 1 because we use another validator.
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[1].index), 0);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[1]);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[1].index), 1);

        // Signature count is now 2 after a third submitInitial.
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 2);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // Signature count is now 3 after a forth submitInitial.
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 3);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // mine random delay blocks
        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        beefyClient.submitFinal(
            commitment, bitfield, finalValidatorProofs3SignatureCount, emptyLeaf, emptyLeafProofs, emptyLeafProofOrder
        );

        assertEq(beefyClient.latestBeefyBlock(), blockNumber);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 4);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 0);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[1].index), 1);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[1].index), 0);
        return commitment;
    }

    function testSubmitFailWithInvalidValidatorProofWhenNotProvidingSignatureCount() public {
        BeefyClient.Commitment memory commitment = initialize(setId);

        // Signature count is 0 for the first submitInitial.
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // Signature count is now 1 after a second submitInitial.
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // Signature count is now 2 after a third submitInitial.
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // mine random delay blocks
        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        // make an invalid signature
        vm.expectRevert(BeefyClient.InvalidValidatorProofLength.selector);
        beefyClient.submitFinal(commitment, bitfield, finalValidatorProofs, mmrLeaf, mmrLeafProofs, leafProofOrder);
    }

    function testSubmitFailInvalidSignature() public {
        BeefyClient.Commitment memory commitment = initialize(setId);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // mine random delay blocks
        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        // make an invalid signature
        finalValidatorProofs[0].r = 0xb5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c;
        vm.expectRevert(BeefyClient.InvalidSignature.selector);
        beefyClient.submitFinal(
            commitment, bitfield, finalValidatorProofs, emptyLeaf, emptyLeafProofs, emptyLeafProofOrder
        );
    }

    function testSubmitFailValidatorNotInBitfield() public {
        BeefyClient.Commitment memory commitment = initialize(setId);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // mine random delay blocks
        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        // make an invalid validator index
        finalValidatorProofs[0].index = 0;
        vm.expectRevert(BeefyClient.InvalidValidatorProof.selector);
        beefyClient.submitFinal(
            commitment, bitfield, finalValidatorProofs, emptyLeaf, emptyLeafProofs, emptyLeafProofOrder
        );
    }

    function testSubmitFailWithStaleCommitment() public {
        BeefyClient.Commitment memory commitment = initialize(setId);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // mine random delay blocks
        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        // Simulates another submitFinal incrementing the latestBeefyBlock
        beefyClient.setLatestBeefyBlock(commitment.blockNumber + 1);

        vm.expectRevert(BeefyClient.StaleCommitment.selector);
        beefyClient.submitFinal(
            commitment, bitfield, finalValidatorProofs, emptyLeaf, emptyLeafProofs, emptyLeafProofOrder
        );
    }

    function testSubmitFailWithInvalidBitfield() public {
        BeefyClient.Commitment memory commitment = initialize(setId);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        // invalid bitfield here
        bitfield[0] = 0;
        vm.expectRevert(BeefyClient.InvalidBitfield.selector);
        beefyClient.submitFinal(
            commitment, bitfield, finalValidatorProofs, emptyLeaf, emptyLeafProofs, emptyLeafProofOrder
        );
    }

    function testSubmitFailWithoutPrevRandao() public {
        BeefyClient.Commitment memory commitment = initialize(setId);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // reverted without commit PrevRandao
        vm.expectRevert(BeefyClient.PrevRandaoNotCaptured.selector);
        beefyClient.submitFinal(
            commitment, bitfield, finalValidatorProofs, emptyLeaf, emptyLeafProofs, emptyLeafProofOrder
        );
    }

    function testSubmitFailForPrevRandaoTooEarlyOrTooLate() public {
        BeefyClient.Commitment memory commitment = initialize(setId);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);
        // reverted for commit PrevRandao too early
        vm.expectRevert(BeefyClient.WaitPeriodNotOver.selector);
        commitPrevRandao();

        // reverted for commit PrevRandao too late
        vm.roll(block.number + randaoCommitDelay + randaoCommitExpiration + 1);
        vm.expectRevert(BeefyClient.TicketExpired.selector);
        commitPrevRandao();
    }

    function testSubmitFailForPrevRandaoCapturedMoreThanOnce() public {
        BeefyClient.Commitment memory commitment = initialize(setId);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);
        vm.roll(block.number + randaoCommitDelay);
        commitPrevRandao();

        vm.expectRevert(BeefyClient.PrevRandaoAlreadyCaptured.selector);
        commitPrevRandao();
    }

    function testSubmitWithHandover() public {
        //initialize with previous set
        BeefyClient.Commitment memory commitment = initialize(setId - 1);

        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 0);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 0);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 0);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 1);

        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        beefyClient.submitFinal(commitment, bitfield, finalValidatorProofs, mmrLeaf, mmrLeafProofs, leafProofOrder);
        assertEq(beefyClient.latestBeefyBlock(), blockNumber);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 1);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 0);
    }

    function testSubmitWithHandoverCountersAreCopiedCorrectly() public {
        //initialize with previous set
        BeefyClient.Commitment memory commitment = initialize(setId - 1);

        // submit with the first validator
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[1]);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[1].index), 0);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[1].index), 1);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 0);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 1);

        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        beefyClient.submitFinal(commitment, bitfield, finalValidatorProofs, mmrLeaf, mmrLeafProofs, leafProofOrder);
        assertEq(beefyClient.latestBeefyBlock(), blockNumber);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 1);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 0);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[1].index), 1);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[1].index), 0);
    }

    function testCommitPrevRandaoCalledInSequence() public {
        vm.expectRevert(BeefyClient.InvalidTicket.selector);
        commitPrevRandao();
    }

    function testSubmitWithHandoverAnd3SignatureCount() public {
        //initialize with previous set
        BeefyClient.Commitment memory commitment = initialize(setId - 1);

        // Signature count is 0 for the first submitInitial.
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 0);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // Signature count is now 1 after a second submitInitial.
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 1);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // Signature count is still 1 because we use another validator.
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[1].index), 0);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[1]);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[1].index), 1);

        // Signature count is now 2 after a third submitInitial.
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 2);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // Signature count is now 3 after a forth submitInitial.
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 3);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        beefyClient.submitFinal(
            commitment, bitfield, finalValidatorProofs3SignatureCount, mmrLeaf, mmrLeafProofs, leafProofOrder
        );
        assertEq(beefyClient.latestBeefyBlock(), blockNumber);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[0].index), 4);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[0].index), 0);
        assertEq(beefyClient.getValidatorCounter(false, finalValidatorProofs[1].index), 1);
        assertEq(beefyClient.getValidatorCounter(true, finalValidatorProofs[1].index), 0);
    }

    function testSubmitWithHandoverFailWithInvalidValidatorProofWhenNotProvidingSignatureCount() public {
        //initialize with previous set
        BeefyClient.Commitment memory commitment = initialize(setId - 1);

        // Signature count is 0 for the first submitInitial.
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // Signature count is now 1 after a second submitInitial.
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // Signature count is now 2 after a third submitInitial.
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        vm.expectRevert(BeefyClient.InvalidValidatorProofLength.selector);
        beefyClient.submitFinal(commitment, bitfield, finalValidatorProofs, mmrLeaf, mmrLeafProofs, leafProofOrder);
    }

    function testSubmitWithHandoverFailWithoutPrevRandao() public {
        //initialize with previous set
        BeefyClient.Commitment memory commitment = initialize(setId - 1);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        vm.expectRevert(BeefyClient.PrevRandaoNotCaptured.selector);
        beefyClient.submitFinal(commitment, bitfield, finalValidatorProofs, mmrLeaf, mmrLeafProofs, leafProofOrder);
    }

    function testSubmitWithHandoverFailStaleCommitment() public {
        BeefyClient.Commitment memory commitment = initialize(setId - 1);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        // mine random delay blocks
        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        // Simulates another submitFinal incrementing the latestBeefyBlock
        beefyClient.setLatestBeefyBlock(commitment.blockNumber + 1);

        vm.expectRevert(BeefyClient.StaleCommitment.selector);
        beefyClient.submitFinal(
            commitment, bitfield, finalValidatorProofs, emptyLeaf, emptyLeafProofs, emptyLeafProofOrder
        );
    }

    function testScaleEncodeCommit() public {
        BeefyClient.PayloadItem[] memory _payload = new BeefyClient.PayloadItem[](2);
        _payload[0] = BeefyClient.PayloadItem(bytes2("ab"), hex"000102");
        _payload[1] =
            BeefyClient.PayloadItem(mmrRootID, hex"3ac49cd24778522203e8bf40a4712ea3f07c3803bbd638cb53ebb3564ec13e8c");

        BeefyClient.Commitment memory _commitment = BeefyClient.Commitment(5, 7, _payload);

        bytes memory encoded = beefyClient.encodeCommitment_public(_commitment);

        assertEq(
            encoded,
            hex"0861620c0001026d68803ac49cd24778522203e8bf40a4712ea3f07c3803bbd638cb53ebb3564ec13e8c050000000700000000000000"
        );
    }

    function testCreateInitialBitfield() public {
        initialize(setId);
        uint256[] memory initialBitfield = beefyClient.createInitialBitfield(bitSetArray, setSize);
        assertTrue(initialBitfield.length == (setSize + 255) / 256);
        printBitArray(initialBitfield);
    }

    function testCreateInitialBitfieldInvalid() public {
        initialize(setId);
        vm.expectRevert(BeefyClient.InvalidBitfieldLength.selector);
        beefyClient.createInitialBitfield(bitSetArray, bitSetArray.length - 1);
    }

    function testCreateFinalBitfield() public {
        BeefyClient.Commitment memory commitment = initialize(setId);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);
        vm.roll(block.number + randaoCommitDelay);
        commitPrevRandao();

        uint256[] memory finalBits = beefyClient.createFinalBitfield(commitHash, bitfield);
        assertTrue(Bitfield.countSetBits(finalBits) < Bitfield.countSetBits(bitfield));
    }

    function testCreateFinalBitfieldInvalid() public {
        BeefyClient.Commitment memory commitment = initialize(setId);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);
        vm.roll(block.number + randaoCommitDelay);
        commitPrevRandao();

        // make invalid bitfield not same as initialized
        bitfield[0] = 0;
        vm.expectRevert(BeefyClient.InvalidBitfield.selector);
        beefyClient.createFinalBitfield(commitHash, bitfield);
    }

    function testSubmitFailWithInvalidValidatorSet() public {
        BeefyClient.Commitment memory commitment = initialize(setId);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        vm.roll(block.number + randaoCommitDelay);
        commitPrevRandao();

        createFinalProofs();

        //reinitialize with next validator set
        initialize(setId + 1);
        //submit will be reverted with InvalidCommitment
        vm.expectRevert(BeefyClient.InvalidCommitment.selector);
        beefyClient.submitFinal(commitment, bitfield, finalValidatorProofs, mmrLeaf, mmrLeafProofs, leafProofOrder);
    }

    function testSubmitWithHandoverFailWithInvalidValidatorSet() public {
        //initialize with previous set
        BeefyClient.Commitment memory commitment = initialize(setId - 1);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        vm.roll(block.number + randaoCommitDelay);
        commitPrevRandao();

        createFinalProofs();

        //reinitialize with next validator set
        initialize(setId + 1);
        //submit will be reverted with InvalidCommitment
        vm.expectRevert(BeefyClient.InvalidCommitment.selector);
        beefyClient.submitFinal(commitment, bitfield, finalValidatorProofs, mmrLeaf, mmrLeafProofs, leafProofOrder);
    }

    function testSubmitFailWithInvalidTicket() public {
        BeefyClient.Commitment memory commitment = initialize(setId);
        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        vm.roll(block.number + randaoCommitDelay);
        commitPrevRandao();

        createFinalProofs();

        // Changing the commitment changes its hash, so the ticket can't be found.
        // A zero value ticket is returned in this case, because submitInitial hasn't run for this commitment.
        BeefyClient.Commitment memory _commitment = BeefyClient.Commitment(blockNumber, setId + 1, commitment.payload);
        //submit will be reverted with InvalidTicket
        vm.expectRevert(BeefyClient.InvalidTicket.selector);
        beefyClient.submitFinal(_commitment, bitfield, finalValidatorProofs, mmrLeaf, mmrLeafProofs, leafProofOrder);
    }

    function testSubmitFailWithInvalidMMRLeaf() public {
        //initialize with previous set
        BeefyClient.Commitment memory commitment = initialize(setId - 1);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        vm.roll(block.number + randaoCommitDelay);

        vm.prevrandao(bytes32(uint256(prevRandao)));

        beefyClient.commitPrevRandao(commitHash);

        createFinalProofs();

        //construct nextAuthoritySetID with a wrong value
        mmrLeaf.nextAuthoritySetID = setId;
        //submit will be reverted with InvalidMMRLeaf
        vm.expectRevert(BeefyClient.InvalidMMRLeaf.selector);
        beefyClient.submitFinal(commitment, bitfield, finalValidatorProofs, mmrLeaf, mmrLeafProofs, leafProofOrder);
    }

    function testSubmitFailWithInvalidMMRLeafProof() public {
        //initialize with previous set
        BeefyClient.Commitment memory commitment = initialize(setId - 1);

        beefyClient.submitInitial(commitment, bitfield, finalValidatorProofs[0]);

        vm.roll(block.number + randaoCommitDelay);

        commitPrevRandao();

        createFinalProofs();

        //construct parentNumber with a wrong value
        mmrLeaf.parentNumber = 1;
        //submit will be reverted with InvalidMMRLeafProof
        vm.expectRevert(BeefyClient.InvalidMMRLeafProof.selector);
        beefyClient.submitFinal(commitment, bitfield, finalValidatorProofs, mmrLeaf, mmrLeafProofs, leafProofOrder);
    }

    function testSubmitFailWithNotEnoughClaims() public {
        BeefyClient.Commitment memory commitment = initialize(setId);
        uint256[] memory initialBits = absentBitfield;
        Bitfield.set(initialBits, finalValidatorProofs[0].index);
        printBitArray(initialBits);
        vm.expectRevert(BeefyClient.NotEnoughClaims.selector);
        beefyClient.submitInitial(commitment, initialBits, finalValidatorProofs[0]);
    }

    function testRegenerateBitField() public {
        // Generate a bitfield for signature count 0.
        uint256 numRequiredSignatures =
            beefyClient.computeNumRequiredSignatures_public(setSize, 0, minNumRequiredSignatures);
        regenerateBitField(bitFieldFile0SignatureCount, numRequiredSignatures);
        // Generate a bitfield for signature count 3.
        numRequiredSignatures = beefyClient.computeNumRequiredSignatures_public(setSize, 3, minNumRequiredSignatures);
        regenerateBitField(bitFieldFile3SignatureCount, numRequiredSignatures);
    }

    function testFuzzComputeValidatorSetQuorum(uint128 validatorSetLen) public {
        // There must be atleast 1 validator.
        vm.assume(validatorSetLen > 0);
        // Calculator 2/3 with flooring due to integer division.
        uint256 twoThirdsMajority = uint256(validatorSetLen) * 2 / 3;
        uint256 result = beefyClient.computeQuorum_public(validatorSetLen);
        assertGt(result, twoThirdsMajority, "result is greater than 2/3rds");
        assertLe(result, validatorSetLen, "result is less than validator set length.");
        assertGt(result, 0, "result is greater than zero.");
    }

    function testFuzzSignatureSamplingRanges(uint128 validatorSetLen, uint16 signatureUsageCount, uint16 minSignatures)
        public
    {
        // There must be atleast 1 validator.
        vm.assume(validatorSetLen > 0);
        // Min signatures must be less than the amount of validators.
        vm.assume(beefyClient.computeQuorum_public(validatorSetLen) > minSignatures);

        uint256 result =
            beefyClient.computeNumRequiredSignatures_public(validatorSetLen, signatureUsageCount, minSignatures);

        // Calculator 2/3 with flooring due to integer division plus 1.
        uint256 twoThirdsMajority = uint256(validatorSetLen) * 2 / 3 + 1;
        assertLe(result, twoThirdsMajority, "result is less than or equal to quorum.");
        assertGe(result, minSignatures, "result is greater than or equal to minimum signatures.");
        assertLe(result, validatorSetLen, "result is less than validator set length.");
        assertGt(result, 0, "result is greater than zero.");
    }

    function testSignatureSamplingCases() public {
        uint256 result = beefyClient.computeQuorum_public(1);
        assertEq(1, result, "B");
        result = beefyClient.computeNumRequiredSignatures_public(1, 0, 0);
        assertEq(1, result, "C");
    }

    function testStorageToStorageCopies() public {
        beefyClient.copyCounters();
    }

    function testFuzzInitializationValidation(uint128 currentId, uint128 nextId) public {
        vm.assume(currentId < type(uint128).max);
        vm.assume(currentId + 1 != nextId);
        vm.expectRevert("invalid-constructor-params");
        new BeefyClient(
            randaoCommitDelay,
            randaoCommitExpiration,
            minNumRequiredSignatures,
            0,
            BeefyClient.ValidatorSet(currentId, 0, 0x0),
            BeefyClient.ValidatorSet(nextId, 0, 0x0)
        );
    }
}
