// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Terminate } from "./Terminate.sol";

contract TerminateDelegator {
    receive() external payable {}
    
    constructor() payable {}

    function delegateCallTerminate(address terminate_addr, uint8 method, address beneficiary) external {
        bytes memory data = abi.encodeWithSelector(Terminate.terminate.selector, method, beneficiary);
        (bool success, ) = terminate_addr.delegatecall(data);
        require(success, "delegatecall terminate reverted");
    }
}
