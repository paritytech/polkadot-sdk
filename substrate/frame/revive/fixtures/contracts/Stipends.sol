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
 * and reverts when denied.
 */
contract ReentrancyProbe {
    receive() external payable {
        (bool reentered, ) = msg.sender.call("");
        require(reentered, "reentry denied");
    }
}

/**
 * @title StipendSender
 * @dev Calls the receiver in each call form that the stipend rule checks.
 */
contract StipendSender {
    address payable immutable receiver;

    constructor(address payable _receiver) {
        receiver = _receiver;
    }

    function transferToReceiver() public payable {
        receiver.transfer(msg.value);
    }

    function isSendDenied() public payable returns (bool) {
        return !receiver.send(msg.value);
    }

    function isCallWithGasDenied(uint64 gasLimit) public payable returns (bool) {
        (bool ok, ) = receiver.call{value: msg.value, gas: gasLimit}("");
        return !ok;
    }

    function isStaticCallWithGasDenied(uint64 gasLimit) public view returns (bool) {
        (bool ok, ) = address(receiver).staticcall{gas: gasLimit}("");
        return !ok;
    }

    function isSelfSendAllowed() public payable returns (bool) {
        return payable(address(this)).send(msg.value);
    }

    receive() external payable {}
}

/**
 * @title WarmWriteSender
 * @dev Makes a slot hot with a normal write, then reaches it again on a limited gas budget.
 */
contract WarmWriteSender {
    uint256 public counter;

    function isWarmWriteDenied(BumpableCounter receiver) public payable returns (bool) {
        receiver.bump();
        return !payable(address(receiver)).send(msg.value);
    }

    function isWarmWriteDeniedWithGas(
        BumpableCounter receiver,
        uint64 gasLimit
    ) public payable returns (bool) {
        receiver.bump();
        (bool ok, ) = payable(address(receiver)).call{value: msg.value, gas: gasLimit}("");
        return !ok;
    }

    function isWarmStaticReadDenied(
        BumpableCounter receiver,
        uint64 gasLimit
    ) public returns (bool) {
        receiver.bump();
        (bool ok, ) =
            address(receiver).staticcall{gas: gasLimit}(abi.encodeCall(receiver.counter, ()));
        return !ok;
    }

    function isWarmDelegateWriteDenied(
        BumpableCounter receiver,
        uint64 gasLimit
    ) public returns (bool) {
        counter += 1;
        (bool ok, ) = address(receiver).delegatecall{gas: gasLimit}(
            abi.encodeCall(BumpableCounter.bump, ())
        );
        return !ok;
    }
}

/**
 * @title BumpableCounter
 * @dev Stores a `counter`, and `bump` adds one to it.
 */
abstract contract BumpableCounter {
    uint256 public counter;

    function bump() external {
        counter += 1;
    }

    function counterSlot() internal pure returns (bytes32 slot) {
        assembly {
            slot := counter.slot
        }
    }
}

/**
 * @title StorageHelpers
 * @dev Storage helpers shared by the receivers.
 */
library StorageHelpers {
    // The `flags` bit of the storage precompile that selects transient storage.
    uint32 internal constant TRANSIENT = 1;

    function clearStorage(uint32 flags, bytes32 key) internal returns (bool success) {
        (success, ) = STORAGE_ADDR.delegatecall(
            abi.encodeCall(IStorage.clearStorage, (flags, true, abi.encodePacked(key)))
        );
    }

    function takeStorage(uint32 flags, bytes32 key) internal returns (bool success) {
        (success, ) = STORAGE_ADDR.delegatecall(
            abi.encodeCall(IStorage.takeStorage, (flags, true, abi.encodePacked(key)))
        );
    }

    // Writes 1 to transient slot 0, failing unless the write is visible.
    function writeTransient() internal {
        uint256 value;
        assembly {
            tstore(0, 1)
            value := tload(0)
        }
        require(value == 1, "transient write denied");
    }

    function transientValue() internal view returns (uint256 value) {
        assembly {
            value := tload(0)
        }
    }
}

/**
 * @title WritingReceiver
 * @dev Increments `counter` on `bump` and on receiving value.
 */
contract WritingReceiver is BumpableCounter {
    receive() external payable {
        counter += 1;
    }
}

/**
 * @title ClearingReceiver
 * @dev Increments `counter` on `bump` and zeroes it on receiving value.
 */
contract ClearingReceiver is BumpableCounter {
    receive() external payable {
        counter = 0;
    }
}

/**
 * @title PrecompileClearingReceiver
 * @dev Increments `counter` on `bump` and clears its slot through the storage precompile on
 * receiving value.
 */
contract PrecompileClearingReceiver is BumpableCounter {
    receive() external payable {
        require(StorageHelpers.clearStorage(0, counterSlot()), "clear denied");
    }
}

/**
 * @title PrecompileTakingReceiver
 * @dev Increments `counter` on `bump` and takes its slot through the storage precompile on
 * receiving value.
 */
contract PrecompileTakingReceiver is BumpableCounter {
    receive() external payable {
        require(StorageHelpers.takeStorage(0, counterSlot()), "take denied");
    }
}

/**
 * @title NestedWritingReceiver
 * @dev On receiving value, writes storage from a nested call, delegate call or create.
 */
contract NestedWritingReceiver {
    enum Nesting { Call, DelegateCall, Create }

    uint256 public counter;
    WritingReceiver immutable target;
    Nesting immutable nesting;

    constructor(WritingReceiver _target, Nesting _nesting) {
        target = _target;
        nesting = _nesting;
    }

    receive() external payable {
        if (nesting == Nesting.Call) {
            target.bump();
        } else if (nesting == Nesting.DelegateCall) {
            (bool success, ) =
                address(target).delegatecall(abi.encodeCall(BumpableCounter.bump, ()));
            require(success, "delegate call failed");
        } else {
            new CounterStartingAtOne();
        }
    }
}

/**
 * @title CounterStartingAtOne
 * @dev Writes storage in its constructor.
 */
contract CounterStartingAtOne {
    uint256 public counter = 1;
}

/**
 * @title TransientWritingReceiver
 * @dev Writes transient storage on receiving value, failing unless the write is visible.
 */
contract TransientWritingReceiver {
    receive() external payable {
        StorageHelpers.writeTransient();
    }
}

/**
 * @title TransientClearingReceiver
 * @dev Writes and then zeroes transient storage on receiving value, failing unless the slot ends
 * empty.
 */
contract TransientClearingReceiver {
    receive() external payable {
        StorageHelpers.writeTransient();
        assembly {
            tstore(0, 0)
        }
        require(StorageHelpers.transientValue() == 0, "transient clear denied");
    }
}

/**
 * @title TransientPrecompileClearingReceiver
 * @dev Writes transient storage and clears it through the storage precompile on receiving value.
 */
contract TransientPrecompileClearingReceiver {
    receive() external payable {
        StorageHelpers.writeTransient();
        require(StorageHelpers.clearStorage(StorageHelpers.TRANSIENT, bytes32(0)), "clear denied");
        require(StorageHelpers.transientValue() == 0, "transient clear denied");
    }
}

/**
 * @title TransientPrecompileTakingReceiver
 * @dev Writes transient storage and takes it through the storage precompile on receiving value.
 */
contract TransientPrecompileTakingReceiver {
    receive() external payable {
        StorageHelpers.writeTransient();
        require(StorageHelpers.takeStorage(StorageHelpers.TRANSIENT, bytes32(0)), "take denied");
        require(StorageHelpers.transientValue() == 0, "transient take denied");
    }
}
