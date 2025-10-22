// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract HostEvmOnly {
    function selfdestructOp(address payable recipient) public {
        assembly {
            selfdestruct(recipient)
        }
    }

    function extcodecopyOp(address account, uint64 offset, uint64 size) public view returns (bytes memory code) {
        code = new bytes(size);
        assembly {
            extcodecopy(account, add(code, 32), offset, size)
        }
    }
}

contract HostEvmOnlyFactory {
    function createAndSelfdestruct(address payable recipient) public returns (address newContract) {
        // Deploy a new instance of HostEvmOnly
        HostEvmOnly newInstance = new HostEvmOnly();
        newContract = address(newInstance);

        // Call selfdestruct on the newly created contract
        newInstance.selfdestructOp(recipient);

        return newContract;
    }
}
