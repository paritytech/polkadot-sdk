// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

// The `aggregate3` function of Multicall3: makes each call with all the gas left and reverts when
// a call that does not allow failure fails.
contract Multicall3 {
    struct Call3 {
        address target;
        bool allowFailure;
        bytes callData;
    }

    struct Result {
        bool success;
        bytes returnData;
    }

    function aggregate3(Call3[] calldata calls) external returns (Result[] memory returnData) {
        returnData = new Result[](calls.length);
        for (uint256 index = 0; index < calls.length; index++) {
            Call3 calldata call3 = calls[index];
            (bool success, bytes memory ret) = call3.target.call(call3.callData);
            require(success || call3.allowFailure, "Multicall3: call failed");
            returnData[index] = Result(success, ret);
        }
    }
}
