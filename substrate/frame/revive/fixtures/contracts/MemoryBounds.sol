// SPDX-License-Identifier: MIT

pragma solidity ^0.8.24;

contract MemoryBounds {
	fallback() external payable {
		assembly {
			// Accessing OOB offsets should always work when the length is 0.
			return(exp(128, 128), 0)
		}
	}
}

