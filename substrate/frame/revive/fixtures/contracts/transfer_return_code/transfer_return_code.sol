// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Transfer Return Code Contract
/// @notice Tests transfer operations and return codes.
contract TransferReturnCode {
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main fallback function that tests transfer with return codes
    fallback() external payable {
        // Try to transfer 100 wei to the zero address
        address payable target = payable(address(0));
        uint256 amount = 100;
        
        // Perform the transfer and capture the return code
        bool success = target.send(amount);
        uint32 retCode = success ? 0 : 1;
        
        // Return the transfer return code as little-endian bytes
        assembly {
            mstore(0x00, retCode)
            return(0x00, 0x04)
        }
    }
    
    /// @notice Alternative implementation using low-level call
    function callWithLowLevel() public payable returns (uint32) {
        // Try to transfer 100 wei to the zero address
        address payable target = payable(address(0));
        uint256 amount = 100;
        
        // Perform the transfer using low-level call
        (bool success, ) = target.call{value: amount}("");
        uint32 retCode = success ? 0 : 1;
        
        // Return the transfer return code
        return retCode;
    }
    
    /// @notice Allow the contract to receive Ether
    receive() external payable {}
}