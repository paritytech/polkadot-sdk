// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.23;

import "forge-std/Test.sol";
import "forge-std/console.sol";

import {Math} from "../src/utils/Math.sol";

contract MathTest is Test {
    struct Log2Test {
        uint256 result;
        uint256 input;
    }

    function setUp() public {}

    function testLog2WithWellKnownValues() public {
        // Test log will well known values generated from python.
        Log2Test[47] memory tests = [
            Log2Test(0, 0),
            Log2Test(0, 1),
            Log2Test(1, 2),
            Log2Test(2, 3),
            Log2Test(2, 4),
            Log2Test(3, 5),
            Log2Test(3, 6),
            Log2Test(3, 8),
            Log2Test(4, 9),
            Log2Test(4, 12),
            Log2Test(4, 16),
            Log2Test(5, 17),
            Log2Test(5, 24),
            Log2Test(5, 32),
            Log2Test(6, 33),
            Log2Test(6, 48),
            Log2Test(6, 64),
            Log2Test(7, 65),
            Log2Test(7, 96),
            Log2Test(7, 128),
            Log2Test(8, 129),
            Log2Test(8, 192),
            Log2Test(8, 256),
            Log2Test(9, 257),
            Log2Test(9, 384),
            Log2Test(9, 512),
            Log2Test(10, 513),
            Log2Test(10, 768),
            Log2Test(10, 1024),
            Log2Test(11, 1025),
            Log2Test(11, 1536),
            Log2Test(11, 2048),
            Log2Test(12, 2049),
            Log2Test(12, 3072),
            Log2Test(12, 4096),
            Log2Test(13, 4097),
            Log2Test(13, 6144),
            Log2Test(13, 8192),
            Log2Test(14, 8193),
            Log2Test(14, 12288),
            Log2Test(14, 16384),
            Log2Test(15, 16385),
            Log2Test(15, 24576),
            Log2Test(15, 32768),
            Log2Test(16, 32769),
            Log2Test(16, 49152),
            Log2Test(16, 65535)
        ];

        for (uint256 t = 0; t < tests.length; ++t) {
            assertEq(tests[t].result, Math.log2(tests[t].input, Math.Rounding.Ceil));
        }
    }

    function testFuzzMin(uint256 a, uint256 b) public {
        vm.assume(a < b);
        assertEq(a, Math.min(a, b));
    }

    function testFuzzMax(uint256 a, uint256 b) public {
        vm.assume(a > b);
        assertEq(a, Math.max(a, b));
    }

    function testFuzzSaturatingAdd(uint16 a, uint16 b) public {
        uint256 result = uint256(a) + uint256(b);
        if (result > 0xFFFF) {
            result = 0xFFFF;
        }
        assertEq(result, Math.saturatingAdd(a, b));
    }

    function testFuzzSaturatingSub(uint256 a, uint256 b) public {
        uint256 result = 0;
        if (a > b) {
            result = a - b;
        }
        assertEq(result, Math.saturatingSub(a, b));
    }
}
