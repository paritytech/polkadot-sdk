// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Bitwise {
    function testBitwise() public pure {
        require(5 < 10, "LT basic");
        require(type(uint256).max - 1 < type(uint256).max, "LT max");

        require(10 > 5, "GT basic");
        require(type(uint256).max > type(uint256).max - 1, "GT max");

        require(5 != 10, "NEQ basic");
        require(10 == 10, "EQ basic");
        require(type(uint256).max == type(uint256).max, "EQ max");

        require(int(-5) < int(10), "SLT basic");
        require(type(int256).min < 0, "SLT min");

        require(int(5) > int(-10), "SGT basic");
        require(0 > type(int256).min, "SGT min");

        require((5 & 3) == 1, "AND basic");
        require((5 | 3) == 7, "OR basic");
        require((5 ^ 3) == 6, "XOR basic");
        require(~uint(0) == type(uint256).max, "NOT basic");

        require((1 << 3) == 8, "SHL basic");
        require((8 >> 3) == 1, "SHR basic");
    }
}
