// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract Origin {
    constructor() {
        // Empty constructor
    }
    
    function call() external returns (address) {
        address caller = msg.sender;
        address origin = tx.origin;
        
        // If caller is not the origin, return the origin
        if (caller != origin) {
            return origin;
        }
        
        // Otherwise, call itself recursively
        address thisContract = address(this);
        (bool success, bytes memory returnData) = thisContract.call{gas: gasleft() - 10000}(
            abi.encodeWithSignature("call()")
        );
        require(success, "Recursive call failed");
        
        address returnedOrigin = abi.decode(returnData, (address));
        require(returnedOrigin == origin, "Origin mismatch");
        
        return origin;
    }
}