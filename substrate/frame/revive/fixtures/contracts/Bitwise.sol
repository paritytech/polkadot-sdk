// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

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


    function clzOp(uint256 a) public returns (uint256 r) {
        assembly {
            // TODO: CLZ instruction is not yet supported by solidity.
            // r := clz(a)
            r := 0
        }
    }

    function byteOp(uint256 index, uint256 value) public returns (uint256 r) {
        assembly {
            r := byte(index, value)
        }
    }


    function shl(uint256 shift, uint256 value) public returns (uint256 r) {
        assembly {
            r := shl(shift, value)
        }
    }


    function shr(uint256 shift, uint256 value) public returns (uint256 r) {
        assembly {
            r := shr(shift, value)
        }
    }


    function sar(uint256 shift, int256 value) public returns (int256 r) {
        assembly {
           r := sar(shift, value)
        }
    }
}