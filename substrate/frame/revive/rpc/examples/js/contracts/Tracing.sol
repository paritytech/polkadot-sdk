// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TracingCaller {
	event TraceEvent(uint256 value, string message);
    address payable public callee;

	constructor(address payable _callee) public payable {
        require(_callee != address(0), "Callee address cannot be zero");
        callee = _callee;
    }

    function start(uint256 counter) external {
        if (counter == 0) {
			return;
        }

        uint256 paymentAmount = 0.01 ether;
        callee.transfer(paymentAmount);

        emit TraceEvent(counter, "before");
		TracingCallee(callee).consumeGas(counter);
		emit TraceEvent(counter, "after");

        try TracingCallee(callee).failingFunction{value: paymentAmount}() {
        } catch {
        }

		this.start(counter - 1);
    }
}

contract TracingCallee {
    event CalleeCalled(uint256 counter);

    function consumeGas(uint256 counter) external {
		// burn some gas
        for (uint256 i = 0; i < 10; i++) {
			uint256(keccak256(abi.encodePacked(i)));
        }

        emit CalleeCalled(counter);
    }

	function failingFunction() external payable {
        require(false, "This function always fails");
    }

	// Enable contract to receive Ether
    receive() external payable {}
}

