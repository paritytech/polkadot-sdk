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

	function mloadOp(uint256 offset) public view returns (uint256) {
        uint256 value;
        assembly {
            value := mload(offset)
        }
		return value;
	}

	function mstoreOp(uint256 offset, uint256 value) public {
        assembly {
            mstore(offset, value)
        }
	}

	function mstore8Op(uint256 offset, uint256 value) public {
        assembly {
            mstore8(offset, value)
        }
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