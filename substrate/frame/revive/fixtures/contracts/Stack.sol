// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Stack {
    function test_pop() public pure {
        uint256 result;
        assembly {
            let a := 42
            let b := 99
            pop(b)
            result := a
        }
        assert(result == 42);
    }

    function test_push0() public pure {
        uint256 result;
        assembly {
            result := 0
        }
        assert(result == 0);
    }

    function test_push() public pure {
        uint256 value = 123;
        assert(value == 123);
    }

    function test_dup1() public pure {
        uint256 val1;
        uint256 val2;
        assembly {
            let a := 42
            dup1
            val2 := pop()
            val1 := pop()
        }
        assert(val1 == 42);
        assert(val2 == 42);
    }

    function test_dup2() public pure {
        uint256 val1;
        uint256 val2;
        uint256 val3;
        assembly {
            let a := 42
            let b := 99
            dup2
            val3 := pop()
            val2 := pop()
            val1 := pop()
        }
        assert(val1 == 42);
        assert(val2 == 99);
        assert(val3 == 42);
    }

    function test_dup3() public pure {
        uint256 val1;
        uint256 val2;
        uint256 val3;
        uint256 val4;
        assembly {
            let a := 42
            let b := 99
            let c := 123
            dup3
            val4 := pop()
            val3 := pop()
            val2 := pop()
            val1 := pop()
        }
        assert(val1 == 42);
        assert(val2 == 99);
        assert(val3 == 123);
        assert(val4 == 42);
    }

    function test_swap1() public pure {
        uint256 val1;
        uint256 val2;
        assembly {
            let a := 42
            let b := 99
            swap1
            val2 := pop()
            val1 := pop()
        }
        assert(val1 == 99);
        assert(val2 == 42);
    }

    function test_swap2() public pure {
        uint256 val1;
        uint256 val2;
        uint256 val3;
        assembly {
            let a := 42
            let b := 99
            let c := 123
            swap2
            val3 := pop()
            val2 := pop() 
            val1 := pop()
        }
        assert(val1 == 123);
        assert(val2 == 99);
        assert(val3 == 42);
    }

    function test_swap3() public pure {
        uint256 val1;
        uint256 val2;
        uint256 val3;
        uint256 val4;
        assembly {
            let a := 42
            let b := 99
            let c := 123
            let d := 456
            swap3
            val4 := pop()
            val3 := pop()
            val2 := pop()
            val1 := pop()
        }
        assert(val1 == 456);
        assert(val2 == 99);
        assert(val3 == 123);
        assert(val4 == 42);
    }
}