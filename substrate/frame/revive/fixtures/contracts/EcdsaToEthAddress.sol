// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

contract EcdsaToEthAddress {
    function convert(uint8[33] calldata publicKey) external returns (bytes20) {
        bytes memory data = abi.encodeWithSelector(ISystem.EcdsaToEthAddress.selector, publicKey);
        (bool success, bytes memory returnData) = SYSTEM_ADDR.call(data);
        if (!success) {
            assembly {
                revert(add(returnData, 0x20), mload(returnData))
            }
        }
        bytes20 ok = abi.decode(returnData, (bytes20));
        return ok;
    }
}
