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

    function callvalue() public payable returns (uint64) {
        return uint64(msg.value);
    }

    function calldataload(uint64 offset) public pure returns (bytes32) {
        bytes32 data;
        assembly {
            data := calldataload(offset)
        }
        return data;
    }

    function calldatasize() public pure returns (uint64) {
        return uint64(msg.data.length);
    }

    function calldatacopy(uint64 destOffset, uint64 offset, uint64 size) public pure returns (bytes memory) {
        bytes memory data = new bytes(size);
        assembly {
            calldatacopy(add(data, 0x20), offset, size)
        }
        return data;
    }

    function codesize() public pure returns (uint64) {
        uint256 size;
        assembly {
            size := codesize()
        }
        return uint64(size);
    }

    function codecopy(uint64, /* destOffset */ uint64, /* offset */ uint64 size) public pure returns (bytes memory) {
        bytes memory code = new bytes(size);
        return code;
    }

    function returndatasize(address _callee, bytes memory _data, uint64 _gas) public returns (uint64) {
        uint256 size;
        _callee.staticcall{gas: _gas}(_data);
        assembly {
            size := returndatasize()
        }
        return uint64(size);
    }

    function returndatacopy(
        address _callee,
        bytes memory _data,
        uint64 _gas,
        uint64 destOffset,
        uint64 offset,
        uint64 size
    ) public returns (bytes memory) {
        bytes memory data = new bytes(size);
        _callee.staticcall{gas: _gas}(_data);
        assembly {
            returndatacopy(add(data, 0x20), offset, size)
        }
        return data;
    }

    function gas() public view returns (uint64) {
        return uint64(gasleft());
    }
}
