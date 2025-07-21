// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract ImmutableData {
    bytes8 private immutableData;
    
    constructor(bytes8 data) {
        // Set immutable data during deployment
        immutableData = data;
    }
    
    function call(bytes8 expectedData) external view {
        // Get the immutable data and compare with expected
        bytes8 actualData = immutableData;
        if (actualData != expectedData) {
            assembly {
                invalid()
            }
        }
    }
    
    function getImmutableData() external view returns (bytes8) {
        return immutableData;
    }
}