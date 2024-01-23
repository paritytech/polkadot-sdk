// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.23;

import "forge-std/Test.sol";
import "forge-std/console.sol";

import {ScaleCodec} from "../src/utils/ScaleCodec.sol";

contract ScaleCodecTest is Test {
    function testEncodeU256() public {
        assertEq(
            ScaleCodec.encodeU256(12063978950259949786323707366460749298097791896371638493358994162204017315152),
            hex"504d8a21dd3868465c8c9f2898b7f014036935fa9a1488629b109d3d59f8ab1a"
        );
    }

    function testEncodeU128() public {
        assertEq(ScaleCodec.encodeU128(35452847761173902980759433963665451267), hex"036935fa9a1488629b109d3d59f8ab1a");
    }

    function testEncodeU64() public {
        assertEq(ScaleCodec.encodeU64(1921902728173129883), hex"9b109d3d59f8ab1a");
    }

    function testEncodeU32() public {
        assertEq(ScaleCodec.encodeU32(447477849), hex"59f8ab1a");
    }

    function testEncodeU16() public {
        assertEq(ScaleCodec.encodeU16(6827), hex"ab1a");
    }

    function testEncodeCompactU32() public {
        assertEq(ScaleCodec.encodeCompactU32(0), hex"00");
        assertEq(ScaleCodec.encodeCompactU32(63), hex"fc");
        assertEq(ScaleCodec.encodeCompactU32(64), hex"0101");
        assertEq(ScaleCodec.encodeCompactU32(16383), hex"fdff");
        assertEq(ScaleCodec.encodeCompactU32(16384), hex"02000100");
        assertEq(ScaleCodec.encodeCompactU32(1073741823), hex"feffffff");
        assertEq(ScaleCodec.encodeCompactU32(1073741824), hex"0300000040");
        assertEq(ScaleCodec.encodeCompactU32(type(uint32).max), hex"03ffffffff");
    }

    function testCheckedEncodeCompactU32() public {
        assertEq(ScaleCodec.checkedEncodeCompactU32(type(uint32).max), hex"03ffffffff");

        vm.expectRevert(ScaleCodec.UnsupportedCompactEncoding.selector);
        ScaleCodec.checkedEncodeCompactU32(uint256(type(uint32).max) + 1);
    }
}
