// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Arithmetic {
    function testArithmetic() public {
        // ADD tests
        require(20 + 22 == 42, "ADD basic");

        // SUB tests
        require(42 - 20 == 22, "SUB basic");

        // MUL tests
        require(20 * 22 == 440, "MUL basic");

        // DIV tests
        require(100 / 5 == 20, "DIV basic");

        // SDIV tests
        require(int(-100) / 5 == -20, "SDIV neg/pos");
        require(int(100) / -5 == -20, "SDIV pos/neg");
        require(int(-100) / -5 == 20, "SDIV neg/neg");

        // REM/MOD tests
        require(100 % 7 == 2, "REM basic");

        // SMOD tests
        require(int(-100) % 7 == -2, "SMOD neg dividend");
        require(int(100) % -7 == 2, "SMOD neg divisor");

        // ADDMOD tests
        require((10 + 15) % 7 == 4, "ADDMOD basic");

        // MULMOD tests
        require((10 * 15) % 7 == 3, "MULMOD basic");

        // EXP tests
        require(2 ** 3 == 8, "EXP basic");
        require(10 ** 0 == 1, "EXP zero exponent");
        require(0 ** 5 == 0, "EXP zero base");

        // SIGNEXTEND tests
        uint result1;
        assembly {
            result1 := signextend(0, 0xff)
        }
        require(result1 == type(uint256).max, "SIGNEXTEND negative byte");
        uint result2;
        assembly {
            result2 := signextend(0, 0x7f)
        }
        require(result2 == 0x7f, "SIGNEXTEND positive byte");
    }
}
