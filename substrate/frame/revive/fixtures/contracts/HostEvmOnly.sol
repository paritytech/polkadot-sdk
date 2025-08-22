// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract HostEvmOnly {
    function selfdestructOp(address payable recipient) public {
        assembly{
            selfdestruct(recipient)
        }
    }
    function extcodecopyOp(address account, uint256 offset, uint256 size) public view returns (bytes memory code) {
        code = new bytes(size);
        assembly {
            extcodecopy(account, add(code, 32), offset, size)
        }
    }
}