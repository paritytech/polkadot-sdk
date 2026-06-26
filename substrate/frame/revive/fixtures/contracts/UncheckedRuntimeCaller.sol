// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

// Minimal interface for the unstable runtime precompile.
interface IUncheckedRuntime {
    function dispatch(bytes memory encodedCall) external;
    function getStorage(bytes memory key, uint32 maxLen) external returns (bytes memory);
}

// Test helper contract that forwards calls to the `UncheckedRuntime` precompile at
// a caller-provided address.
contract UncheckedRuntimeCaller {
    // Dispatch a SCALE-encoded `RuntimeCall` through the precompile. The runtime
    // call executes as this contract's account.
    function runDispatch(address precompile, bytes memory encodedCall) external {
        IUncheckedRuntime(precompile).dispatch(encodedCall);
    }

    // Read raw runtime storage through the precompile and return the value bytes.
    function runStorage(address precompile, bytes memory key, uint32 maxLen)
        external
        returns (bytes memory value)
    {
        value = IUncheckedRuntime(precompile).getStorage(key, maxLen);
    }
}
