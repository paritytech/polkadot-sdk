// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

/**
 * @title Sender
 * @dev Sender contract: provides three transfer methods
 */
contract StipendSender {
    event TransferSuccess(string method, address to, uint256 amount, uint256 gasUsed);
    event TransferFailed(string method, address to, uint256 amount, string reason);

    // Method 1: transfer (2300 gas stipend)
    function sendViaTransfer(address payable to) external payable {
        uint256 gasBefore = gasleft();
        to.transfer(msg.value);
        uint256 gasUsed = gasBefore - gasleft();
        emit TransferSuccess("transfer", to, msg.value, gasUsed);
    }

    // Method 2: send (2300 gas stipend, returns bool)
    function sendViaSend(address payable to) external payable returns (bool) {
        uint256 gasBefore = gasleft();
        bool success = to.send(msg.value);
        uint256 gasUsed = gasBefore - gasleft();

        if (success) {
            emit TransferSuccess("send", to, msg.value, gasUsed);
        } else {
            emit TransferFailed("send", to, msg.value, "send returned false");
        }
        return success;
    }

    // Method 3: call (forwards all gas)
    function sendViaCall(address payable to) external payable returns (bool) {
        uint256 gasBefore = gasleft();
        (bool success, ) = to.call{value: msg.value}("");
        uint256 gasUsed = gasBefore - gasleft();

        if (success) {
            emit TransferSuccess("call", to, msg.value, gasUsed);
        } else {
            emit TransferFailed("call", to, msg.value, "call returned false");
        }
        return success;
    }

    receive() external payable {}
}

/**
 * @title DoNothingReceiver
 * @dev Receiver contract 1: empty receive(), does nothing
 */
contract DoNothingReceiver {
    receive() external payable {}

    function getBalance() external view returns (uint256) {
        return address(this).balance;
    }
}

/**
 * @title SimpleReceiver
 * @dev Receiver contract 2: only emits events
 */
contract SimpleReceiver {
    event Received(address from, uint256 amount);

    receive() external payable {
        emit Received(msg.sender, msg.value);
    }

    function getBalance() external view returns (uint256) {
        return address(this).balance;
    }
}

/**
 * @title ComplexReceiver
 * @dev Receiver contract 3: performs complex operations (SSTORE)
 */
contract ComplexReceiver {
    uint256 public counter;
    event Received(address from, uint256 amount, uint256 newCounter);

    receive() external payable {
        counter += 1;
        emit Received(msg.sender, msg.value, counter);
    }

    function getBalance() external view returns (uint256) {
        return address(this).balance;
    }
}