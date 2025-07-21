// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Transfer Return Code Contract
/// @notice Tests transfer operations and return codes to match Rust implementation behavior.
contract TransferReturnCode {
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main fallback function that tests transfer with return codes
    /// @dev Matches the Rust implementation: attempts to transfer 100 wei to zero address
    fallback() external payable {
        // Transfer 100 wei to the zero address (20 zero bytes)
        // This matches the Rust implementation's call parameters
        uint256 amount = 100;
        
        // Use low-level call to match the Rust api::call behavior
        // The Rust code uses api::call with zero address and 100 wei
        (bool success, ) = address(0).call{value: amount}("");
        
        // Map the result to return code: 0 for success, non-zero for failure
        // This matches the Rust implementation's error handling
        uint32 retCode;
        if (success) {
            retCode = 0;
        } else {
            // For failed transfers, use error code 1
            retCode = 1;
        }
        
        // Return the transfer return code as little-endian bytes
        // This exactly matches the Rust implementation's return behavior
        assembly {
            mstore(0x00, retCode)
            return(0x00, 0x04)
        }
    }
    
    /// @notice Deploy function to match Rust contract structure
    function deploy() external {
        // Empty deploy function to match Rust implementation
    }
    
    /// @notice Call function to match Rust contract structure
    function call() external payable {
        // Delegate to fallback for main functionality
        // This ensures consistent behavior regardless of call method
        uint256 amount = 100;
        
        (bool success, ) = address(0).call{value: amount}("");
        
        uint32 retCode;
        if (success) {
            retCode = 0;
        } else {
            retCode = 1;
        }
        
        assembly {
            mstore(0x00, retCode)
            return(0x00, 0x04)
        }
    }
    
    /// @notice Helper function for testing with validation
    /// @dev Uses assembly invalid() for trap behavior on invalid conditions
    function testTransferWithValidation(address target, uint256 amount) external payable returns (uint32) {
        // Validate input parameters using assembly invalid() for trap behavior
        if (amount == 0) {
            assembly { invalid() }
        }
        
        // Perform the transfer
        (bool success, ) = target.call{value: amount}("");
        
        // Return appropriate code
        if (success) {
            return 0;
        } else {
            return 1;
        }
    }
    
    /// @notice Allow the contract to receive Ether
    receive() external payable {}
}