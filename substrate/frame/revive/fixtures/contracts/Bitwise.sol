// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Bitwise {
    function test_lt() public pure {
        assert(5 < 10 == true);
        assert(10 < 5 == false);
        assert(5 < 5 == false);
    }

    function test_gt() public pure {
        assert(10 > 5 == true);
        assert(5 > 10 == false);
        assert(5 > 5 == false);
    }

    function test_slt() public pure {
        assert(int256(-5) < int256(5) == true);
        assert(int256(5) < int256(-5) == false);
        assert(int256(5) < int256(5) == false);
    }

    function test_sgt() public pure {
        assert(int256(5) > int256(-5) == true);
        assert(int256(-5) > int256(5) == false);
        assert(int256(5) > int256(5) == false);
    }

    function test_eq() public pure {
        assert((5 == 5) == true);
        assert((5 == 10) == false);
        assert((0 == 0) == true);
    }

    function test_iszero() public pure {
        assert((0 == 0) == true);
        assert((5 == 0) == false);
        assert((type(uint256).max == 0) == false);
    }

    function test_bitand() public pure {
        assert((0xF0 & 0x0F) == 0x00);
        assert((0xFF & 0xFF) == 0xFF);
        assert((0xAA & 0x55) == 0x00);
    }

    function test_bitor() public pure {
        assert((0xF0 | 0x0F) == 0xFF);
        assert((0x00 | 0xFF) == 0xFF);
        assert((0xAA | 0x55) == 0xFF);
    }

    function test_bitxor() public pure {
        assert((0xF0 ^ 0x0F) == 0xFF);
        assert((0xFF ^ 0xFF) == 0x00);
        assert((0xAA ^ 0x55) == 0xFF);
    }

    function test_not() public pure {
        assert(~uint256(0) == type(uint256).max);
        assert(~type(uint256).max == 0);
        assert(~uint256(0xF0) == type(uint256).max - 0xF0);
    }

    function test_byte() public pure {
        uint256 result;
        assembly {
            result := byte(0, 0x1234567890abcdef)
        }
        assert(result == 0x12);
        
        assembly {
            result := byte(1, 0x1234567890abcdef)
        }
        assert(result == 0x34);
    }

    function test_shl() public pure {
        assert((1 << 1) == 2);
        assert((1 << 8) == 256);
        assert((0xFF << 8) == 0xFF00);
    }

    function test_shr() public pure {
        assert((256 >> 1) == 128);
        assert((256 >> 8) == 1);
        assert((0xFF00 >> 8) == 0xFF);
    }

    function test_sar() public pure {
        assert((int256(256) >> 1) == int256(128));
        assert((int256(-256) >> 1) == int256(-128));
        assert((int256(-1) >> 8) == int256(-1));
    }

    function test_clz() public pure {
        uint256 result;
        assembly {
            result := clz(1)
        }
        assert(result == 255);
        
        assembly {
            result := clz(0x8000000000000000000000000000000000000000000000000000000000000000)
        }
        assert(result == 0);
    }
}