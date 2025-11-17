// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "./Terminate.sol";

contract TerminateCaller {
    receive() external payable {}
    
    constructor() payable {}

    function sendFundsAfterTerminate(address payable terminate_addr, uint value, address beneficiary) external {
        terminate_addr.call(abi.encodeWithSelector(Terminate.terminate.selector, beneficiary));
        (bool success, ) = terminate_addr.call{value: value}("");
        require(success, "terminate reverted");
    }

    function revertAfterTerminate(address terminate_addr, address beneficiary) external {
        terminate_addr.call(abi.encodeWithSelector(Terminate.terminate.selector, beneficiary));
        _revert();
    }

    function _revert() private {
        revert("Deliberate revert");
    }
}