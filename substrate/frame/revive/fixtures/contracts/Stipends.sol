// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@revive/IStorage.sol";

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
 * @title ReentrancyAttacker
 * @dev On receiving ETH, attempts to call back into the sender
 */
contract ReentrancyAttacker {
    receive() external payable {
        // Classic reentrancy: try to drain more ETH from the sender.
        // We intentionally don't revert on failure so the outer transfer
        // succeeds and the test can check the balance invariant.
        msg.sender.call(
            abi.encodeWithSignature("attemptTransfer(address,uint256)", address(this), msg.value)
        );
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
    ReentrancyAttacker reentrancyAttacker;
    address payable eoa;

    constructor() {
        doNothingReceiver = new DoNothingReceiver();
        simpleReceiver = new SimpleReceiver();
        complexReceiver = new ComplexReceiver();
        reentrancyAttacker = new ReentrancyAttacker();
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

    // Test transfer with zero value (solc injects gas=2300 explicitly)
    function testTransferZero() public {
        // EOA should succeed
        eoa.transfer(0);

        // DoNothingReceiver should succeed (empty receive)
        payable(address(doNothingReceiver)).transfer(0);

        // SimpleReceiver should succeed
        payable(address(simpleReceiver)).transfer(0);
    }

    // Test send with zero value (solc injects gas=2300 explicitly)
    function testSendZero() public {
        // EOA should succeed
        bool success = eoa.send(0);
        require(success, "EOA send zero failed");

        // DoNothingReceiver should succeed
        success = payable(address(doNothingReceiver)).send(0);
        require(success, "DoNothingReceiver send zero failed");

        // SimpleReceiver should succeed
        success = payable(address(simpleReceiver)).send(0);
        require(success, "SimpleReceiver send zero failed");
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

    // Test that the transfer stipend prevents reentrancy. The attacker's receive()
    // tries to call back into attemptTransfer() to drain more ETH, but the stipend
    // starves that call, so it drains nothing: the attacker receives only `amount`.
    function testTransferReentrancy() public payable {
        uint256 amount = msg.value / 4;
        uint256 attackerBefore = address(reentrancyAttacker).balance;
        uint256 selfBefore = address(this).balance;

        this.attemptTransfer(payable(address(reentrancyAttacker)), amount);
        require(
            address(reentrancyAttacker).balance == attackerBefore + amount,
            "Attacker should receive exactly the transferred amount"
        );
        require(
            address(this).balance == selfBefore - amount,
            "StipendTest should lose exactly the transferred amount"
        );
    }

    // Test that the send stipend prevents reentrancy.
    function testSendReentrancy() public payable {
        uint256 amount = msg.value / 4;
        uint256 attackerBefore = address(reentrancyAttacker).balance;
        uint256 selfBefore = address(this).balance;

        bool success = payable(address(reentrancyAttacker)).send(amount);
        require(success, "Send to reentrancy attacker should succeed");
        require(
            address(reentrancyAttacker).balance == attackerBefore + amount,
            "Attacker should receive exactly the sent amount"
        );
        require(
            address(this).balance == selfBefore - amount,
            "StipendTest should lose exactly the sent amount"
        );
    }

    receive() external payable {}
}

/**
 * @title ReentrancyProbe
 * @dev Checks whether reentry is admitted. The reentrant call is cheap enough to fit the stipend,
 * and reverts when denied, which makes the outer call fail.
 */
contract ReentrancyProbe {
    receive() external payable {
        (bool reentered, ) = msg.sender.call("");
        require(reentered, "reentry denied");
    }
}

/**
 * @title StipendSender
 * @dev Small enough that its code loads within the stipend when it is reentered.
 */
contract StipendSender {
    address payable immutable probe;

    constructor(address payable _probe) {
        probe = _probe;
    }

    function attemptTransfer(address payable to, uint256 amount) external {
        to.transfer(amount);
    }

    function isTransferDenied() public payable returns (bool) {
        try this.attemptTransfer(probe, msg.value) {
            return false;
        } catch {
            return true;
        }
    }

    function isSendDenied() public payable returns (bool) {
        return !probe.send(msg.value);
    }

    /// @dev Passing a gas limit explicitly keeps the probe on the stipend but lets it reenter,
    /// since only transfer and send are guarded.
    function isCallWithGasDenied(uint64 gasLimit) public payable returns (bool) {
        (bool ok, ) = probe.call{value: msg.value, gas: gasLimit}("");
        return !ok;
    }

    /// @dev The guard must stop the callee reaching back, not the sender reaching its own address.
    function isSelfSendAllowed() public payable returns (bool) {
        return payable(address(this)).send(msg.value);
    }

    receive() external payable {}
}

/**
 * @title WritingReceiver
 * @dev Increments `counter` on `bump` and on receiving value.
 */
contract WritingReceiver {
    uint256 public counter;

    function bump() external {
        counter += 1;
    }

    receive() external payable {
        counter += 1;
    }
}

/**
 * @title WarmWriteSender
 * @dev Makes the receiver's slot hot with a normal call, then sends to it on the stipend alone.
 */
contract WarmWriteSender {
    function isWarmWriteDenied(WritingReceiver receiver) public payable returns (bool) {
        receiver.bump();
        return !payable(address(receiver)).send(msg.value);
    }
}

/**
 * @title ClearingReceiver
 * @dev Increments `counter` on `bump` and zeroes it on receiving value.
 */
contract ClearingReceiver {
    uint256 public counter;

    function bump() external {
        counter += 1;
    }

    receive() external payable {
        counter = 0;
    }
}

/**
 * @title PrecompileClearingReceiver
 * @dev Increments `counter` on `bump` and clears its slot through the storage precompile on receiving value.
 */
contract PrecompileClearingReceiver {
    uint256 public counter;

    function bump() external {
        counter += 1;
    }

    receive() external payable {
        uint256 slot;
        assembly {
            slot := counter.slot
        }
        (bool success, ) = STORAGE_ADDR.delegatecall(
            abi.encodeWithSelector(IStorage.clearStorage.selector, 0, true, abi.encodePacked(bytes32(slot)))
        );
        require(success, "clear denied");
    }
}

/**
 * @title PrecompileTakingReceiver
 * @dev Increments `counter` on `bump` and takes its slot through the storage precompile on receiving value.
 */
contract PrecompileTakingReceiver {
    uint256 public counter;

    function bump() external {
        counter += 1;
    }

    receive() external payable {
        uint256 slot;
        assembly {
            slot := counter.slot
        }
        (bool success, ) = STORAGE_ADDR.delegatecall(
            abi.encodeWithSelector(IStorage.takeStorage.selector, 0, true, abi.encodePacked(bytes32(slot)))
        );
        require(success, "take denied");
    }
}

// The `flags` bit of the storage precompile that selects transient storage.
uint32 constant TRANSIENT = 1;

/**
 * @title TransientWritingReceiver
 * @dev Writes transient storage on receiving value, failing unless the write is visible.
 */
contract TransientWritingReceiver {
    receive() external payable {
        uint256 value;
        assembly {
            tstore(0, 1)
            value := tload(0)
        }
        require(value == 1, "transient write denied");
    }
}

/**
 * @title TransientClearingReceiver
 * @dev Writes and then zeroes transient storage on receiving value, failing unless the slot ends empty.
 */
contract TransientClearingReceiver {
    receive() external payable {
        uint256 value;
        assembly {
            tstore(0, 1)
            tstore(0, 0)
            value := tload(0)
        }
        require(value == 0, "transient clear denied");
    }
}

/**
 * @title TransientPrecompileClearingReceiver
 * @dev Writes transient storage and clears it through the storage precompile on receiving value.
 */
contract TransientPrecompileClearingReceiver {
    receive() external payable {
        uint256 value;
        assembly {
            tstore(0, 1)
        }
        (bool success, ) = STORAGE_ADDR.delegatecall(
            abi.encodeWithSelector(IStorage.clearStorage.selector, TRANSIENT, true, abi.encodePacked(bytes32(0)))
        );
        require(success, "clear denied");
        assembly {
            value := tload(0)
        }
        require(value == 0, "transient clear denied");
    }
}

/**
 * @title TransientPrecompileTakingReceiver
 * @dev Writes transient storage and takes it through the storage precompile on receiving value.
 */
contract TransientPrecompileTakingReceiver {
    receive() external payable {
        uint256 value;
        assembly {
            tstore(0, 1)
        }
        (bool success, ) = STORAGE_ADDR.delegatecall(
            abi.encodeWithSelector(IStorage.takeStorage.selector, TRANSIENT, true, abi.encodePacked(bytes32(0)))
        );
        require(success, "take denied");
        assembly {
            value := tload(0)
        }
        require(value == 0, "transient take denied");
    }
}
