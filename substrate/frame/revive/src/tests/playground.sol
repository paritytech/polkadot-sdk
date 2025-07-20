// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Playground {
    function fib(uint n) public pure returns (uint) {
        if (n <= 1) {
            return n;
        }
        return fib(n - 1) + fib(n - 2);
    }

	function bn() public view returns (uint) {
		return block.number;
	}
}
