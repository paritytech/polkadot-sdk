// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract Memory {

	function testMemory(uint256 offset, uint256 value) public returns (uint256) {
        assembly {
            mstore(offset, value)
        }
        uint256 result = 123;
        assembly {
            result := mload(offset)
        }
        return result - value;
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

	function msizeOp() public view returns (uint256) {
        uint256 value;
        //assembly {
        //    value := msize()
        //}
		return value;
	}

	function mcopyOp(uint256 dstOffset, uint256 offset, uint256 size) public {
        assembly {
            mcopy(dstOffset, offset, size)
        }
	}



}