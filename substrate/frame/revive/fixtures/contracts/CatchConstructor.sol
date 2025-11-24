// SPDX-License-Identifier: MIT
pragma solidity >=0.7.0;

// External contract used for try / catch examples
contract CatchConstructorFoo {
    address public owner;

    constructor(address _owner) {
        require(_owner != address(0), "invalid address");
        assert(_owner != 0x0000000000000000000000000000000000000001);
        owner = _owner;
    }

    function myFunc(uint x) public pure returns (string memory) {
        require(x != 0, "require failed");
        return "my func was called";
    }
}

contract CatchConstructorTest {
    event Log(string message);
    event LogBytes(bytes data);

    CatchConstructorFoo public foo;

    constructor() {
        // This CatchConstructorFoo contract is used for example of try catch with external call
        foo = new CatchConstructorFoo(msg.sender);
    }

    // Example of try / catch with external call
    // tryCatchExternalCall(0) => Log("external call failed")
    // tryCatchExternalCall(1) => Log("my func was called")
    function tryCatchExternalCall(uint _i) public {
        try foo.myFunc(_i) returns (string memory result) {
            emit Log(result);
        } catch {
            emit Log("external call failed");
        }
    }

    // Example of try / catch with contract creation
    // tryCatchNewContract(0x0000000000000000000000000000000000000000) => Log("invalid address")
    // tryCatchNewContract(0x0000000000000000000000000000000000000001) => LogBytes("")
    // tryCatchNewContract(0x0000000000000000000000000000000000000002) => Log("CatchConstructorFoo created")
    function tryCatchNewContract(address _owner) public {
        try new CatchConstructorFoo(_owner) returns (CatchConstructorFoo foo) {
            // you can use variable foo here
            emit Log("CatchConstructorFoo created");
        } catch Error(string memory reason) {
            // catch failing revert() and require()
            emit Log(reason);
        } catch (bytes memory reason) {
            // catch failing assert()
            emit LogBytes(reason);
        }
    }
}