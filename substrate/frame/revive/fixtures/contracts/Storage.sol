// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

// Minimal two-slot storage contract for testing storage overrides.
// `first` occupies slot 0, `second` occupies slot 1.
contract Storage {
    uint256 public first;
    uint256 public second;

    function setBoth(uint256 _first, uint256 _second) external {
        first = _first;
        second = _second;
    }
}
