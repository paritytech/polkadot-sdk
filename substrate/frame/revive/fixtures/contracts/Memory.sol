// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract Memory {

	function testMemory() public {
        uint256 value = 0xfe;
        assembly {
            mstore(0, value)
        }
        uint256 result = 123;
        assembly {
            result := mload(0)
        }
        require(result == value, "Memory test failed");

        for (uint256 i = 0; i < 32; i++) {
            assembly {
                mstore8(i, value)
            }
        }
        assembly {
            result := mload(0)
        }
        require(result == 0xfefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefe, "Memory test failed");

        assembly {
            result := msize()
        }
        require(result == 61, "Memory size test failed");
	}

	function testMstore8(uint256 offset, uint256 value) public returns (uint256) {
        for (uint256 i = 0; i < 32; i++) {
            assembly {
                mstore8(add(offset, i), value)
            }
        }
        uint256 result = 123;
        assembly {
            result := mload(offset)
        }
        return result;
	}

	function testMsize(uint256 offset) public returns (uint256) {
        assembly {
            mstore(offset, 123)
        }
        uint256 value;
        assembly {
            value := msize()
        }
		return value;
	}

	function testMcopy(uint256 dstOffset, uint256 offset, uint256 size, uint256 value) public returns (uint256) {
        assembly {
            mstore(dstOffset, 0)
        }
        for (uint256 i = 0; i < size; i += 32) {
            assembly {
                mstore(add(offset, i), value)
            }
        }
        assembly {
            mcopy(dstOffset, offset, size)
        }
        uint256 result = 123;
        assembly {
            result := mload(dstOffset)
        }
        return result;
	}



}