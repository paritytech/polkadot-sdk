// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

contract Callee {
    function echo(uint256 value) external pure returns (uint256) {
        assembly {
            mstore(0x00, "oops")   // this will pack into 32 bytes
            revert(0x00, 0x04)
        }
        //assert(0 == 1);
        return value;
    }
}
