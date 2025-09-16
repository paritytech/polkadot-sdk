// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

contract System {
    function keccak256Func(bytes memory data) public pure returns (bytes32) {
        return keccak256(data);
    }

    function addressFunc() public view returns (address) {
        return address(this);
    }

    function caller() public view returns (address) {
        return msg.sender;
    }

    function callvalue() public payable returns (uint256) {
        return msg.value;
    }

    function calldataload(uint256 offset) public pure returns (bytes32) {
        bytes32 data;
        assembly {
            data := calldataload(offset)
        }
        return data;
    }

    function calldatasize() public pure returns (uint256) {
        return msg.data.length;
    }

    function calldatacopy(
        uint256 destOffset,
        uint256 offset,
        uint256 size
    ) public pure returns (bytes memory) {
        bytes memory data = new bytes(size);
        assembly {
            calldatacopy(add(data, 0x20), offset, size)
        }
        return data;
    }

    function codesize() public pure returns (uint256) {
        uint256 size;
        assembly {
            size := codesize()
        }
        return size;
    }

    function codecopy(
        uint256 /* destOffset */,
        uint256 /* offset */,
        uint256 size
    ) public pure returns (bytes memory) {
        bytes memory code = new bytes(size);
        return code;
    }

    function returndatasize(
        address _callee,
        bytes memory _data,
        uint _gas
    ) public returns (uint256) {
        uint256 size;
        _callee.staticcall{gas: _gas}(_data);
        assembly {
            size := returndatasize()
        }
        return size;
    }

    function returndatacopy(
        address _callee,
        bytes memory _data,
        uint _gas,
        uint256 destOffset,
        uint256 offset,
        uint256 size
    ) public returns (bytes memory) {
        bytes memory data = new bytes(size);
        _callee.staticcall{gas: _gas}(_data);
        assembly {
            returndatacopy(add(data, 0x20), offset, size)
        }
        return data;
    }

    function gas() public view returns (uint256) {
        return gasleft();
    }
}
