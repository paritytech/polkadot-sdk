// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract BlockInfo {
    function fib(uint n) public pure returns (uint) {
        if (n <= 1) {
            return n;
        }
        return fib(n - 1) + fib(n - 2);
    }

	function blockNumber() public view returns (uint) {
		return block.number;
	}
}
