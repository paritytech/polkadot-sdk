// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.23;

import {Test} from "forge-std/Test.sol";
import {console} from "forge-std/console.sol";
import {stdJson} from "forge-std/StdJson.sol";

import {MMRProof} from "../src/utils/MMRProof.sol";
import {MMRProofWrapper} from "./mocks/MMRProofWrapper.sol";

contract MMRProofTest is Test {
    using stdJson for string;

    struct Fixture {
        bytes32[] leaves;
        Proof[] proofs;
        bytes32 rootHash;
    }

    struct Proof {
        bytes32[] items;
        uint256 order;
    }

    bytes public fixtureData;

    MMRProofWrapper public wrapper;

    function setUp() public {
        wrapper = new MMRProofWrapper();

        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/test/data/mmr-fixture-data-15-leaves.json");
        //string memory json = vm.readFile(path);
        fixtureData = vm.readFile(path).parseRaw("");
    }

    function fixture() public view returns (Fixture memory) {
        return abi.decode(fixtureData, (Fixture));
    }

    function testVerifyLeafProof() public {
        Fixture memory fix = fixture();

        for (uint256 i = 0; i < fix.leaves.length; i++) {
            assertTrue(wrapper.verifyLeafProof(fix.rootHash, fix.leaves[i], fix.proofs[i].items, fix.proofs[i].order));
        }
    }

    function testVerifyLeafProofFailsExceededProofSize() public {
        Fixture memory fix = fixture();

        vm.expectRevert(MMRProof.ProofSizeExceeded.selector);
        wrapper.verifyLeafProof(fix.rootHash, fix.leaves[0], new bytes32[](257), fix.proofs[0].order);
    }
}
