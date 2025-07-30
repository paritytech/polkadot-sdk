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

    function iszero(uint a) public view returns (uint) {
        return a == 0 ? 1 : 0;
    }

    function clz(uint a) public pure returns (uint) {
        return a.clz();
    }

    function byteOp(uint index, uint value) public pure returns (uint) {
        return value.byte(index);
    }
    function shl(uint256 value, uint256 shift) public pure returns (uint256) {
        return value.shl(shift);
    }

    function shr(uint256 value, uint256 shift) public pure returns (uint256) {
        return value.shr(shift);
    }

    function sar(int256 value, uint256 shift) public pure returns (int256) {
        return value.sar(shift);
    }
}