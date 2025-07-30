// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Bitwise {

    function lt(uint a, uint b) public view returns (uint) {
        return a < b ? 1 : 0;
    }

    function gt(uint a, uint b) public view returns (uint) {
        return a > b ? 1 : 0;
    }

    function eq(uint a, uint b) public view returns (uint) {
        return a == b ? 1 : 0;
    }

    function slt(int a, int b) public view returns (uint) {
        return a < b ? 1 : 0;
    }

    function sgt(int a, int b) public view returns (uint) {
        return a > b ? 1 : 0;
    }

    function and(uint a, uint b) public view returns (uint) {
        return a & b;
    }

    function or(uint a, uint b) public view returns (uint) {
        return a | b;
    }

    function xor(uint a, uint b) public view returns (uint) {
        return a ^ b;
    }

    function not(uint a) public view returns (uint) {
        return ~a;
    }
}