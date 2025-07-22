// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

contract Predicted {
    uint public salt;

    constructor(uint _salt) {
        salt = _salt;
    }
}

contract AddressPredictor {
    constructor(uint _salt, bytes memory _bytecode) payable {
        address deployed = address(new Predicted{salt: bytes32(_salt)}(_salt));
        address predicted = predictAddress(_salt, _bytecode);
        assert(deployed == predicted);
    }

    function predictAddress(
        uint _foo,
        bytes memory _bytecode
    ) public view returns (address predicted) {
        bytes32 addr = keccak256(
            abi.encodePacked(
                bytes1(0xff),
                address(this),
                bytes32(_foo),
                keccak256(abi.encodePacked(_bytecode, abi.encode(_foo)))
            )
        );
        predicted = address(uint160(uint(addr)));
    }
}
