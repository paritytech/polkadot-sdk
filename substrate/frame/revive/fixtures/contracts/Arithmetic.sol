// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Arithmetic {
    function test_add() public pure {
        assert(5 + 3 == 8);
        assert(0 + 0 == 0);
        assert(type(uint256).max - 1 + 1 == type(uint256).max);
    }

    function test_mul() public pure {
        assert(5 * 3 == 15);
        assert(0 * 100 == 0);
        assert(1 * 42 == 42);
    }

    function test_sub() public pure {
        assert(10 - 3 == 7);
        assert(5 - 5 == 0);
        assert(type(uint256).max - 1 == type(uint256).max - 1);
    }

    function test_div() public pure {
        assert(15 / 3 == 5);
        assert(10 / 2 == 5);
        assert(7 / 2 == 3);
    }

    function test_sdiv() public pure {
        assert(int256(15) / int256(3) == int256(5));
        assert(int256(-15) / int256(3) == int256(-5));
        assert(int256(-15) / int256(-3) == int256(5));
    }

    function test_rem() public pure {
        assert(10 % 3 == 1);
        assert(15 % 5 == 0);
        assert(7 % 2 == 1);
    }

    function test_smod() public pure {
        assert(int256(10) % int256(3) == int256(1));
        assert(int256(-10) % int256(3) == int256(-1));
        assert(int256(10) % int256(-3) == int256(1));
    }

    function test_addmod() public pure {
        assert(addmod(5, 3, 7) == 1);
        assert(addmod(10, 15, 20) == 5);
        assert(addmod(0, 0, 5) == 0);
    }

    function test_mulmod() public pure {
        assert(mulmod(5, 3, 7) == 1);
        assert(mulmod(10, 15, 100) == 50);
        assert(mulmod(0, 100, 7) == 0);
    }

    function test_exp() public pure {
        assert(2 ** 3 == 8);
        assert(5 ** 2 == 25);
        assert(10 ** 0 == 1);
    }

    function test_signextend() public pure {
        uint256 result;
        assembly {
            result := signextend(0, 0xff)
        }
        assert(result == type(uint256).max);
        
        assembly {
            result := signextend(0, 0x7f)
        }
        assert(result == 0x7f);
    }
}