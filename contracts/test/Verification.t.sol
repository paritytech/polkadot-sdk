// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.23;

import "openzeppelin/utils/Strings.sol";
import "forge-std/Test.sol";
import "forge-std/console.sol";

import {ScaleCodec} from "../src/utils/ScaleCodec.sol";
import {BeefyClientMock} from "./mocks/BeefyClientMock.sol";
import {Verification, VerificationWrapper} from "./mocks/VerificationWrapper.sol";

contract VerificationTest is Test {
    BeefyClientMock public beefyClient;
    VerificationWrapper public v;

    uint32 public constant BRIDGE_HUB_PARA_ID = 1013;
    bytes4 public encodedParachainID;

    function setUp() public {
        beefyClient = new BeefyClientMock(3, 8, 16);
        encodedParachainID = ScaleCodec.encodeU32(BRIDGE_HUB_PARA_ID);
        v = new VerificationWrapper();
    }

    function testCreateParachainHeaderMerkleLeaf() public {
        Verification.DigestItem[] memory digestItems = new Verification.DigestItem[](3);
        digestItems[0] = Verification.DigestItem({kind: 6, consensusEngineID: 0x61757261, data: hex"c1f05e0800000000"});
        digestItems[1] = Verification.DigestItem({
            kind: 4,
            consensusEngineID: 0x52505352,
            data: hex"73a902d5a4fa8fea942d01ad3c1dc32b51192c3a98c39fcc59299006ed391a5e2e005501"
        });
        digestItems[2] = Verification.DigestItem({
            kind: 5,
            consensusEngineID: 0x61757261,
            data: hex"fcfbfaf1ad15d24cb4980436c18aec6211e2255f648df0e05e73a7858fba8c31726925f1a825383d0d3cb590502b18978101a6391fbeef5ab53e14c05124188c"
        });

        Verification.ParachainHeader memory header = Verification.ParachainHeader({
            parentHash: 0x1df01d40273b074708115135fd7f76801ad4e4f1266a771a037962ee3a03259d,
            number: 866538,
            stateRoot: 0x7b2d59d4de7c629b55a9bc9b76d932616f2011a26f09b52da36e070d6a7eee0d,
            extrinsicsRoot: 0x9d1c5d256003f68dda03dc33810a88a61f73791dc7ff92b04232a6b1b4f4b3c0,
            digestItems: digestItems
        });

        bytes memory headerExpected =
            hex"1df01d40273b074708115135fd7f76801ad4e4f1266a771a037962ee3a03259daae334007b2d59d4de7c629b55a9bc9b76d932616f2011a26f09b52da36e070d6a7eee0d9d1c5d256003f68dda03dc33810a88a61f73791dc7ff92b04232a6b1b4f4b3c00c066175726120c1f05e080000000004525053529073a902d5a4fa8fea942d01ad3c1dc32b51192c3a98c39fcc59299006ed391a5e2e00550105617572610101fcfbfaf1ad15d24cb4980436c18aec6211e2255f648df0e05e73a7858fba8c31726925f1a825383d0d3cb590502b18978101a6391fbeef5ab53e14c05124188c";

        assertEq(
            keccak256(
                bytes.concat(
                    ScaleCodec.encodeU32(BRIDGE_HUB_PARA_ID),
                    ScaleCodec.encodeCompactU32(uint32(headerExpected.length)),
                    headerExpected
                )
            ),
            v.createParachainHeaderMerkleLeaf(encodedParachainID, header)
        );
    }

    function testCreateParachainHeaderMerkleFailInvalidHeader() public {
        Verification.DigestItem[] memory digestItems = new Verification.DigestItem[](1);
        // Create an invalid digest item
        digestItems[0] =
            Verification.DigestItem({kind: 666, consensusEngineID: 0x61757261, data: hex"c1f05e0800000000"});

        Verification.ParachainHeader memory header = Verification.ParachainHeader({
            parentHash: 0x1df01d40273b074708115135fd7f76801ad4e4f1266a771a037962ee3a03259d,
            number: 866538,
            stateRoot: 0x7b2d59d4de7c629b55a9bc9b76d932616f2011a26f09b52da36e070d6a7eee0d,
            extrinsicsRoot: 0x9d1c5d256003f68dda03dc33810a88a61f73791dc7ff92b04232a6b1b4f4b3c0,
            digestItems: digestItems
        });

        vm.expectRevert(Verification.InvalidParachainHeader.selector);
        v.createParachainHeaderMerkleLeaf(encodedParachainID, header);
    }

    function testIsCommitmentInHeaderDigest() public view {
        Verification.DigestItem[] memory digestItems = new Verification.DigestItem[](4);
        digestItems[0] = Verification.DigestItem({kind: 6, consensusEngineID: 0x61757261, data: hex"c1f05e0800000000"});
        digestItems[1] = Verification.DigestItem({
            kind: 4,
            consensusEngineID: 0x52505352,
            data: hex"73a902d5a4fa8fea942d01ad3c1dc32b51192c3a98c39fcc59299006ed391a5e2e005501"
        });
        digestItems[2] = Verification.DigestItem({
            kind: 0,
            consensusEngineID: 0x00000000,
            data: hex"00b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c"
        });
        digestItems[3] = Verification.DigestItem({
            kind: 5,
            consensusEngineID: 0x61757261,
            data: hex"fcfbfaf1ad15d24cb4980436c18aec6211e2255f648df0e05e73a7858fba8c31726925f1a825383d0d3cb590502b18978101a6391fbeef5ab53e14c05124188c"
        });

        Verification.ParachainHeader memory header = Verification.ParachainHeader({
            parentHash: 0x1df01d40273b074708115135fd7f76801ad4e4f1266a771a037962ee3a03259d,
            number: 866538,
            stateRoot: 0x7b2d59d4de7c629b55a9bc9b76d932616f2011a26f09b52da36e070d6a7eee0d,
            extrinsicsRoot: 0x9d1c5d256003f68dda03dc33810a88a61f73791dc7ff92b04232a6b1b4f4b3c0,
            digestItems: digestItems
        });

        // Digest item at index 2 contains the commitment
        assert(v.isCommitmentInHeaderDigest(0xb5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c, header));

        // Now remove the commitment from the parachain header
        header.digestItems[2] = header.digestItems[3];
        assert(
            !v.isCommitmentInHeaderDigest(0xb5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c, header)
        );
    }
}
