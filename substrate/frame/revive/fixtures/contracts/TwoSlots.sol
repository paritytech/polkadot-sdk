// SPDX-License-Identifier: MIT
pragma solidity ^0.8.4;

/// Minimal two-slot storage contract for testing storage overrides.
/// `first` occupies slot 0, `second` occupies slot 1.
contract TwoSlots {
    uint256 public first;
    uint256 public second;

    constructor(uint256 _first, uint256 _second) {
        first = _first;
        second = _second;
    }
}
