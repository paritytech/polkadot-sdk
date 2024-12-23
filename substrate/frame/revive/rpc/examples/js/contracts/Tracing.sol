// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TracingCaller {
    address public callee;

    constructor(address _callee) {
        require(_callee != address(0), "Callee address cannot be zero");
        callee = _callee;
    }

    function start(uint256 counter) external {
        if (counter == 0) {
            return;
        }

        TracingCallee(callee).consumeGas();

        try TracingCallee(callee).failingFunction() {
        } catch {
        }

        try TracingCallee(callee).consumeGas{gas: 100}() {
        } catch {
        }

		this.start(counter - 1);
    }
}

contract TracingCallee {
    event GasConsumed(address indexed caller);

    function consumeGas() external {
		// burn some gas
        for (uint256 i = 0; i < 10; i++) {
			uint256(keccak256(abi.encodePacked(i)));
        }

        emit GasConsumed(msg.sender);
    }

    function failingFunction() external pure {
        require(false, "This function always fails");
    }
}

