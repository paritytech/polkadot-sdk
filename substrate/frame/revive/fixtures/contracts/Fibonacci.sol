// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Fibonacci {
    function fib(uint64 n) public pure returns (uint64) {
        if (n <= 1) {
            return n;
        }
        return fib(n - 1) + fib(n - 2);
    }
}
