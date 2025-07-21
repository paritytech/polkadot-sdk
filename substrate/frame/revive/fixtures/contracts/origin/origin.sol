// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract Origin {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        address caller = msg.sender;
        address txOrigin = tx.origin;
        
        // If caller is not the origin, return the origin
        if (caller != txOrigin) {
            assembly {
                mstore(0x00, txOrigin)
                return(0x00, 0x20)
            }
        }
        
        // Otherwise, call itself recursively
        address thisContract = address(this);
        (bool success, bytes memory returnData) = thisContract.call{gas: gasleft() - 10000}(
            ""
        );
        require(success, "Recursive call failed");
        
        address returnedOrigin = abi.decode(returnData, (address));
        require(returnedOrigin == txOrigin, "Origin mismatch");
        
        assembly {
            mstore(0x00, txOrigin)
            return(0x00, 0x20)
        }
    }
}