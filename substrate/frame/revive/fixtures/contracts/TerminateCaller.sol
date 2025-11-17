// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "./Terminate.sol";

contract TerminateCaller {
    receive() external payable {}
    
    constructor() payable {}

    function sendFundsAfterTerminate(address payable terminate_addr, uint value, address beneficiary) external {
        // payable(terminate_addr).send(value);
        terminate_addr.call(abi.encodeWithSelector(Terminate.terminate.selector, beneficiary));
        // require(payable(terminate_addr).send(value), "send reverted");
            (bool success, ) = terminate_addr.call{value: value}("");
    require(success, "send reverted");
    }
}