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

    /// Emits logs as `emitLogs` does, then reverts.
    function emitLogsThenRevert(uint32 count, uint32 size, uint32 lastSize) public {
        emitLogs(count, size, lastSize);
        revert();
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

    /// Writes `stored`, then emits logs as `emitLogs` does.
    function storeAndEmitLogs(uint32 count, uint32 size, uint32 lastSize) public {
        stored = 1;
        emitLogs(count, size, lastSize);
    }

    /// Emits logs as `emitLogs` does, calls `target` with `data`, then stores 1 in `stored` if
    /// the call succeeded and 2 if it failed.
    function emitLogsThenCall(
        uint32 count,
        uint32 size,
        uint32 lastSize,
        address target,
        bytes calldata data
    ) public returns (bool success) {
        emitLogs(count, size, lastSize);
        (success, ) = target.call(data);
        stored = success ? 1 : 2;
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
