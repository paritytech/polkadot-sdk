// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Errors {
	bool public state;

	// Payable function that can be used to test insufficient funds errors
    function valueMatch(uint256 value) public payable {
		require(msg.value == value , "msg.value does not match value");
    }

    function setState(bool newState) public {
        state = newState;
    }

    // Trigger a require statement failure with a custom error message
    function triggerRequireError() public pure {
        require(false, "This is a require error");
    }

    // Trigger an assert statement failure
    function triggerAssertError() public pure {
        assert(false);
    }

    // Trigger a revert statement with a custom error message
    function triggerRevertError() public pure {
        revert("This is a revert error");
    }

    // Trigger a division by zero error
    function triggerDivisionByZero() public pure returns (uint256) {
        uint256 a = 1;
        uint256 b = 0;
        return a / b;
    }

    // Trigger an out-of-bounds array access
    function triggerOutOfBoundsError() public pure returns (uint256) {
        uint256[] memory arr = new uint256[](1);
        return arr[2];
    }

    // Trigger a custom error
    error CustomError(string message);

    function triggerCustomError() public pure {
        revert CustomError("This is a custom error");
    }
}

