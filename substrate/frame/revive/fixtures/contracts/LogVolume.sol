// SPDX-License-Identifier: MIT
pragma solidity ^0.8.4;

/// Emits logs of a chosen size, so a test can fill a transaction's receipt to an exact number of
/// bytes.
contract LogVolume {
    event Outer(uint256 value);

    uint256 public stored;

    /// Emits `count` logs without topics and with `size` bytes of data each, then one more with
    /// `lastSize` bytes of data.
    function emitLogs(uint32 count, uint32 size, uint32 lastSize) public {
        assembly {
            for { let i := 0 } lt(i, count) { i := add(i, 1) } {
                log0(0, size)
            }
            log0(0, lastSize)
        }
    }

    /// Emits `Outer`, then lets a sub-call run `emitLogs` and reports whether it succeeded.
    function emitLogsGuarded(uint32 count, uint32 size, uint32 lastSize)
        public
        returns (bool ok)
    {
        emit Outer(1);
        try this.emitLogs(count, size, lastSize) {
            ok = true;
        } catch {
            ok = false;
        }
    }

    /// Writes `stored`, then calls `target` with `data` and requires the call to succeed.
    function storeAndCall(address target, bytes calldata data) public {
        stored = 1;
        (bool success, ) = target.call(data);
        require(success, "call failed");
    }

    /// Calls `target` with `data`, requires the call to succeed, then reverts.
    function callThenRevert(address target, bytes calldata data) public {
        (bool success, ) = target.call(data);
        require(success, "call failed");
        revert("reverted after the call");
    }

    /// Emits `Outer`, then lets a sub-call run `callThenRevert` and swallows its revert.
    function callThenRevertGuarded(address target, bytes calldata data) public {
        emit Outer(1);
        try this.callThenRevert(target, data) {} catch {}
    }
}

/// Emits logs of a chosen size from its constructor.
contract LogVolumeConstructor {
    /// Emits `count` logs without topics and with `size` bytes of data each, then one more with
    /// `lastSize` bytes of data.
    constructor(uint32 count, uint32 size, uint32 lastSize) {
        assembly {
            for { let i := 0 } lt(i, count) { i := add(i, 1) } {
                log0(0, size)
            }
            log0(0, lastSize)
        }
    }
}
