// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

contract Caller {
    function do_call(address callee, uint256 value) external view returns (uint256) {
        (bool success, bytes memory data) = callee.staticcall(
            abi.encodeWithSignature("echo(uint256)", value)
        );
        require(success, "call to Callee failed");
        return abi.decode(data, (uint256));
    }
}
