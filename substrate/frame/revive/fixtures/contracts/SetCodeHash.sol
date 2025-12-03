// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

contract SetCodeHash {
    function setCodeHash(bytes32 codeHash) external returns (uint) {
        bytes memory data = abi.encodeWithSelector(ISystem.setCodeHash.selector, codeHash);
        (bool success, bytes memory returnData) = SYSTEM_ADDR.call(data);
        if (!success) {
            assembly {
                revert(add(returnData, 0x20), mload(returnData))
            }
        }
        return 1;
    }
}
