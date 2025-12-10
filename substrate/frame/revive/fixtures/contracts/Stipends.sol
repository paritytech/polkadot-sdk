// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

/**
 * @title DoNothingReceiver
 * @dev Receiver contract 1: empty receive(), does nothing
 */
contract DoNothingReceiver {
    receive() external payable {}
}

/**
 * @title SimpleReceiver
 * @dev Receiver contract 2: emits events
 */
contract SimpleReceiver {
    event Received(address from, uint256 amount);

    receive() external payable {
        emit Received(msg.sender, msg.value);
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
}

/**
 * @title StipendTest
 * @dev Test contract that verifies stipend behavior for different receiver types
 */
contract StipendTest {
    DoNothingReceiver doNothingReceiver;
    SimpleReceiver simpleReceiver;
    ComplexReceiver complexReceiver;
    address payable eoa;

    constructor() {
        doNothingReceiver = new DoNothingReceiver();
        simpleReceiver = new SimpleReceiver();
        complexReceiver = new ComplexReceiver();
        eoa = payable(address(0x1234567890123456789012345678901234567890));
    }

    // Helper function to attempt transfer (so we can use try-catch)
    function attemptTransfer(address payable to, uint256 amount) external {
        to.transfer(amount);
    }

    // Test transfer method (2300 gas stipend)
    function testTransfer() public payable {
        uint256 amount = msg.value / 4;

        // EOA should succeed
        uint256 balanceBefore = eoa.balance;
        eoa.transfer(amount);
        require(eoa.balance == balanceBefore + amount, "EOA transfer failed");

        // DoNothingReceiver should succeed (empty receive)
        balanceBefore = address(doNothingReceiver).balance;
        payable(address(doNothingReceiver)).transfer(amount);
        require(address(doNothingReceiver).balance == balanceBefore + amount, "DoNothingReceiver transfer failed");

        // SimpleReceiver should succeed
        balanceBefore = address(simpleReceiver).balance;
        payable(address(simpleReceiver)).transfer(amount);
        require(address(simpleReceiver).balance == balanceBefore + amount, "SimpleReceiver transfer failed");

        // ComplexReceiver should fail (not enough gas for SSTORE)
        balanceBefore = address(complexReceiver).balance;
        bool failed = false;
        try this.attemptTransfer(payable(address(complexReceiver)), amount) {
            // Should not succeed
            failed = false;
        } catch {
            failed = true;
        }
        require(failed, "ComplexReceiver transfer should have failed");
        require(address(complexReceiver).balance == balanceBefore, "ComplexReceiver balance changed on failed transfer");
    }

    // Test send method (2300 gas stipend, returns bool)
    function testSend() public payable {
        uint256 amount = msg.value / 4;

        // EOA should succeed
        uint256 balanceBefore = eoa.balance;
        bool success = eoa.send(amount);
        require(success, "EOA send failed");
        require(eoa.balance == balanceBefore + amount, "EOA balance not updated");

        // DoNothingReceiver should succeed (empty receive)
        balanceBefore = address(doNothingReceiver).balance;
        success = payable(address(doNothingReceiver)).send(amount);
        require(success, "DoNothingReceiver send failed");
        require(address(doNothingReceiver).balance == balanceBefore + amount, "DoNothingReceiver balance not updated");

        // SimpleReceiver should succeed
        balanceBefore = address(simpleReceiver).balance;
        success = payable(address(simpleReceiver)).send(amount);
        require(success, "SimpleReceiver send failed");
        require(address(simpleReceiver).balance == balanceBefore + amount, "SimpleReceiver balance not updated");

        // ComplexReceiver should fail (not enough gas for SSTORE)
        balanceBefore = address(complexReceiver).balance;
        success = payable(address(complexReceiver)).send(amount);
        require(!success, "ComplexReceiver send should have failed");
        require(address(complexReceiver).balance == balanceBefore, "ComplexReceiver balance changed on failed send");
    }

    // Test call method (forwards all gas)
    function testCall() public payable {
        uint256 amount = msg.value / 4;

        // EOA should succeed
        uint256 balanceBefore = eoa.balance;
        (bool success, ) = eoa.call{value: amount}("");
        require(success, "EOA call failed");
        require(eoa.balance == balanceBefore + amount, "EOA balance not updated");

        // DoNothingReceiver should succeed (empty receive)
        balanceBefore = address(doNothingReceiver).balance;
        (success, ) = payable(address(doNothingReceiver)).call{value: amount}("");
        require(success, "DoNothingReceiver call failed");
        require(address(doNothingReceiver).balance == balanceBefore + amount, "DoNothingReceiver balance not updated");

        // SimpleReceiver should succeed
        balanceBefore = address(simpleReceiver).balance;
        (success, ) = payable(address(simpleReceiver)).call{value: amount}("");
        require(success, "SimpleReceiver call failed");
        require(address(simpleReceiver).balance == balanceBefore + amount, "SimpleReceiver balance not updated");

        // ComplexReceiver should succeed (enough gas for SSTORE with call)
        balanceBefore = address(complexReceiver).balance;
        uint256 counterBefore = complexReceiver.counter();
        (success, ) = payable(address(complexReceiver)).call{value: amount}("");
        require(success, "ComplexReceiver call failed");
        require(address(complexReceiver).balance == balanceBefore + amount, "ComplexReceiver balance not updated");
        require(complexReceiver.counter() == counterBefore + 1, "ComplexReceiver counter not incremented");
    }

    receive() external payable {}
}