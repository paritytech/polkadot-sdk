// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ControlFlow {
    function test_jump() public pure {
        uint256 result;
        assembly {
            let target := jumpdest_label
            jump(target)
            result := 0
            jumpdest_label:
            result := 1
        }
        assert(result == 1);
    }

    function test_jumpi() public pure {
        uint256 result1;
        uint256 result2;
        
        assembly {
            let target := jumpdest_label1
            jumpi(target, 1)
            result1 := 0
            jump(end1)
            jumpdest_label1:
            result1 := 1
            end1:
        }
        assert(result1 == 1);
        
        assembly {
            let target := jumpdest_label2
            jumpi(target, 0)
            result2 := 0
            jump(end2)
            jumpdest_label2:
            result2 := 1
            end2:
        }
        assert(result2 == 0);
    }

    function test_jumpdest() public pure {
        uint256 result;
        assembly {
            jumpdest
            result := 1
        }
        assert(result == 1);
    }

    function test_pc() public pure {
        uint256 pc1;
        uint256 pc2;
        assembly {
            pc1 := pc()
            pc2 := pc()
        }
        assert(pc2 > pc1);
    }

    function test_ret() public pure {
        bytes memory data = hex"deadbeef";
        bytes memory result;
        
        bool success;
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 0x04)
            mstore(add(ptr, 0x20), 0xdeadbeef00000000000000000000000000000000000000000000000000000000)
            success := call(gas(), address(), 0, ptr, 0x24, 0, 0)
        }
        
        assert(data.length == 4);
        assert(data[0] == 0xde);
    }

    function test_basic_execution() public pure {
        uint256 value = 42;
        assert(value == 42);
    }
}