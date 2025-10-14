// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Arithmetic {
    // We don't run the optimizer to avoid constant folding
    function testArithmetic() public {
        // ADD tests
        uint256 addResult;
        assembly {
            addResult := add(20, 22)
        }
        require(addResult == 42, "ADD basic");

        // SUB tests
        uint256 subResult;
        assembly {
            subResult := sub(42, 20)
        }
        require(subResult == 22, "SUB basic");

        // MUL tests
        uint256 mulResult;
        assembly {
            mulResult := mul(20, 22)
        }
        require(mulResult == 440, "MUL basic");

        // DIV tests
        uint256 divResult;
        assembly {
            divResult := div(100, 5)
        }
        require(divResult == 20, "DIV basic");

        // SDIV tests
        int256 sdivResult1;
        assembly {
            sdivResult1 := sdiv(sub(0, 100), 5)
        }
        require(sdivResult1 == -20, "SDIV neg/pos");

        int256 sdivResult2;
        assembly {
            sdivResult2 := sdiv(100, sub(0, 5))
        }
        require(sdivResult2 == -20, "SDIV pos/neg");

        int256 sdivResult3;
        assembly {
            sdivResult3 := sdiv(sub(0, 100), sub(0, 5))
        }
        require(sdivResult3 == 20, "SDIV neg/neg");

        // REM/MOD tests
        uint256 modResult;
        assembly {
            modResult := mod(100, 7)
        }
        require(modResult == 2, "REM basic");

        // SMOD tests
        int256 smodResult1;
        assembly {
            smodResult1 := smod(sub(0, 100), 7)
        }
        require(smodResult1 == -2, "SMOD neg dividend");

        int256 smodResult2;
        assembly {
            smodResult2 := smod(100, sub(0, 7))
        }
        require(smodResult2 == 2, "SMOD neg divisor");

        // ADDMOD tests
        uint256 addmodResult;
        assembly {
            addmodResult := addmod(10, 15, 7)
        }
        require(addmodResult == 4, "ADDMOD basic");

        // MULMOD tests
        uint256 mulmodResult;
        assembly {
            mulmodResult := mulmod(10, 15, 7)
        }
        require(mulmodResult == 3, "MULMOD basic");

        // EXP tests
        uint256 expResult1;
        assembly {
            expResult1 := exp(2, 3)
        }
        require(expResult1 == 8, "EXP basic");

        uint256 expResult2;
        assembly {
            expResult2 := exp(10, 0)
        }
        require(expResult2 == 1, "EXP zero exponent");

        uint256 expResult3;
        assembly {
            expResult3 := exp(0, 5)
        }
        require(expResult3 == 0, "EXP zero base");

        // EXP overflow test: 2^256 mod 2^256 = 0
        uint256 expResult;
        assembly {
            expResult := exp(2, 256)
        }
        require(expResult == 0, "EXP overflow");

        // EXP test: 2^255 should not overflow
        assembly {
            expResult := exp(2, 255)
        }
        require(expResult == (1 << 255), "EXP 2^255");

        // SIGNEXTEND tests
        uint256 result1;
        assembly {
            result1 := signextend(0, 0xff)
        }
        require(result1 == type(uint256).max, "SIGNEXTEND negative byte");
        uint256 result2;
        assembly {
            result2 := signextend(0, 0x7f)
        }
        require(result2 == 0x7f, "SIGNEXTEND positive byte");
    }
}
