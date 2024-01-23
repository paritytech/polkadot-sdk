// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2023 Axelar Network
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

pragma solidity 0.8.23;

import {IERC20} from "../interfaces/IERC20.sol";

error TokenTransferFailed();
error NativeTransferFailed();

library SafeTokenCall {
    function safeCall(IERC20 token, bytes memory callData) internal {
        (bool success, bytes memory returnData) = address(token).call(callData);
        bool transferred = success && (returnData.length == uint256(0) || abi.decode(returnData, (bool)));
        if (!transferred || address(token).code.length == 0) {
            revert TokenTransferFailed();
        }
    }
}

library SafeTokenTransfer {
    function safeTransfer(IERC20 token, address receiver, uint256 amount) internal {
        SafeTokenCall.safeCall(token, abi.encodeCall(IERC20.transfer, (receiver, amount)));
    }
}

library SafeTokenTransferFrom {
    function safeTransferFrom(IERC20 token, address from, address to, uint256 amount) internal {
        SafeTokenCall.safeCall(token, abi.encodeCall(IERC20.transferFrom, (from, to, amount)));
    }
}

library SafeNativeTransfer {
    function safeNativeTransfer(address payable receiver, uint256 amount) internal {
        bool success;
        assembly {
            success := call(gas(), receiver, amount, 0, 0, 0, 0)
        }
        if (!success) {
            revert NativeTransferFailed();
        }
    }
}
