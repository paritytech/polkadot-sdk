// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Balance {
    fallback() external payable {
        assembly {
            let bal := selfbalance()
            if iszero(iszero(bal)) {
                invalid()
            }
        }
    }
}