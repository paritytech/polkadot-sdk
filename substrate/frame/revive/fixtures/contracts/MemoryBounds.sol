// SPDX-License-Identifier: MIT

pragma solidity ^0.8.24;

contract MemoryBounds {
	fallback() external {
		assembly {

			// Accessing OOB offsets should always work when the length is 0.
			return(100000, 0)
		}
	}
}

