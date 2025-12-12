// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

contract Sr25519Verify {
    function verify(uint8[64] calldata signature, bytes calldata message, bytes32 publicKey) external returns (bool) {
        bytes memory data = abi.encodeWithSelector(ISystem.sr25519Verify.selector, signature, message, publicKey);
        (bool success, bytes memory returnData) = SYSTEM_ADDR.call(data);
        if (!success) {
            assembly {
                revert(add(returnData, 0x20), mload(returnData))
            }
        }
        bool ok = abi.decode(returnData, (bool));
        return ok;
    }
}