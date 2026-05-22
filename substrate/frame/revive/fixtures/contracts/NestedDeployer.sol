// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

contract NestedDeployer {
    function deployChild() external returns (address) {
        return address(new NestedChild());
    }

    function deployAndDestroyChild(address payable beneficiary) external returns (address) {
        NestedChild child = new NestedChild();
        address childAddr = address(child);
        child.destroy(beneficiary);
        return childAddr;
    }
}

contract NestedChild {
    uint256 public state;

    constructor() {
        state = 42;
    }

    function destroy(address payable beneficiary) external {
        selfdestruct(beneficiary);
    }
}
