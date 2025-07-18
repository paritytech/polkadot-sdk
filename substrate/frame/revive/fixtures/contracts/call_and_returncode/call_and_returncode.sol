// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract CallAndReturncode {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Parse input: callee_addr (20 bytes), value (8 bytes), callee_input (rest)
        require(msg.data.length >= 28, "Invalid input length");
        
        address callee_addr = address(bytes20(msg.data[0:20]));
        uint64 value = uint64(bytes8(msg.data[20:28]));
        bytes memory callee_input = msg.data[28:];
        
        // Call the callee
        (bool success, bytes memory output) = callee_addr.call{value: value}(callee_input);
        
        // Prepare return data: first 4 bytes are return code, rest is output
        uint32 code = success ? 0 : 1;
        bytes memory result = new bytes(4 + output.length);
        
        // Copy return code as little-endian
        result[0] = bytes1(uint8(code));
        result[1] = bytes1(uint8(code >> 8));
        result[2] = bytes1(uint8(code >> 16));
        result[3] = bytes1(uint8(code >> 24));
        
        // Copy output
        for (uint i = 0; i < output.length; i++) {
            result[4 + i] = output[i];
        }
        
        // Return the combined data
        assembly {
            return(add(result, 0x20), mload(result))
        }
    }
}