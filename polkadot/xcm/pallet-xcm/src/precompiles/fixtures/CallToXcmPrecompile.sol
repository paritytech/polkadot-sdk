// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @dev The on-chain address of the XCM (Cross-Consensus Messaging) precompile.
address constant XCM_PRECOMPILE_ADDRESS = address(0xA0000);

contract CallToXcmPrecompile {
    event ExecuteSucceed();
    error ExecuteFailed(bytes reason);

    struct Weight {
        uint64 refTime;
        uint64 proofSize;
    }

    function callExecute(bytes calldata message, Weight calldata weight) public {
        (bool success, bytes memory returnData) = XCM_PRECOMPILE_ADDRESS.call(
            abi.encodeWithSignature("execute(bytes,uint64,uint64)", message, weight.refTime, weight.proofSize)
        );
        
        if (success) {
            emit ExecuteSucceed();
        } else {
            revert ExecuteFailed(returnData);
        }
    }

    function callExecute(bytes calldata message) public {
        (bool success, bytes memory returnData) = XCM_PRECOMPILE_ADDRESS.call(
            abi.encodeWithSignature("execute(bytes)", message)
        );
        
        if (success) {
            emit ExecuteSucceed();
        } else {
            revert ExecuteFailed(returnData);
        }
    }

    function callExecuteAsAccount(bytes calldata message, Weight calldata weight) public {
        (bool success, bytes memory returnData) = XCM_PRECOMPILE_ADDRESS.call(
            abi.encodeWithSignature("executeAsAccount(bytes,uint64,uint64)", message, weight.refTime, weight.proofSize)
        );
        
        if (success) {
            emit ExecuteSucceed();
        } else {
            revert ExecuteFailed(returnData);
        }
    }

    function callExecuteAsAccount(bytes calldata message) public {
        (bool success, bytes memory returnData) = XCM_PRECOMPILE_ADDRESS.call(
            abi.encodeWithSignature("executeAsAccount(bytes)", message)
        );
        
        if (success) {
            emit ExecuteSucceed();
        } else {
            revert ExecuteFailed(returnData);
        }
    }
}