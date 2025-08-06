// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract HostSelfDestructEvm {
    function selfdestructOp(address payable recipient) public {
        assembly{
            selfdestruct(recipient)
        }
    }
}