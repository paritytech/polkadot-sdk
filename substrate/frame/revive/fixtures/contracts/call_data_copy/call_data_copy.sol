// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract CallDataCopy {
    constructor() {
        // Empty constructor
    }
    
    function call() external pure {
        // Expect call data of [0xFF; 32]
        require(msg.data.length >= 36, "Insufficient call data"); // 4 bytes selector + 32 bytes data
        
        bytes memory buf = new bytes(32);
        
        // Test 1: Copy full 32 bytes from offset 4 (after selector)
        assembly {
            calldatacopy(add(buf, 0x20), 4, 32)
        }
        
        bytes32 expected = bytes32(hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
        bytes32 actual;
        assembly {
            actual := mload(add(buf, 0x20))
        }
        require(actual == expected, "Test 1 failed");
        
        // Test 2: Copy 8 bytes from offset 31+4
        buf = new bytes(32);
        // Fill with test data first
        bytes32 testData = bytes32(hex"ff000000000000000000ffffffffffffffffffffffffffffffffffffffffff");
        assembly {
            mstore(add(buf, 0x20), testData)
        }
        
        // Copy 8 bytes from offset 35 (31+4)
        assembly {
            calldatacopy(add(buf, 0x20), 35, 8)
        }
        
        // Test 3: Copy from offset 32+4 (beyond data) - should be zeros
        buf = new bytes(32);
        assembly {
            calldatacopy(add(buf, 0x20), 36, 32)
        }
        
        assembly {
            actual := mload(add(buf, 0x20))
        }
        require(actual == bytes32(0), "Test 3 failed");
        
        // Test 4: Copy from very large offset - should be zeros
        buf = new bytes(32);
        // Fill with 0xFF first
        assembly {
            mstore(add(buf, 0x20), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
        }
        
        assembly {
            calldatacopy(add(buf, 0x20), 0xffffffff, 32)
        }
        
        assembly {
            actual := mload(add(buf, 0x20))
        }
        require(actual == bytes32(0), "Test 4 failed");
    }
}