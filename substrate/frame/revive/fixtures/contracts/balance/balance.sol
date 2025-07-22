// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract Balance {
    constructor() payable {}
    
    fallback() external payable {
        // The balance() API call should return this contract's balance, which should be 0
        assembly {
            let bal := selfbalance()
            if iszero(iszero(bal)) {
                invalid()
            }
        }
    }
}