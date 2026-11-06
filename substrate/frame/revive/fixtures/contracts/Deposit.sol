// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.20;

import "@revive/IStorage.sol";

contract DepositPrecompile {
  uint256 a;
  uint256 b;

  function clearStorageSlot(uint256 slot) internal {
    bytes memory key = abi.encodePacked(bytes32(slot));
    (bool _success, ) = STORAGE_ADDR.delegatecall(
        abi.encodeWithSelector(IStorage.clearStorage.selector, 0, true, key)
    );
  }

  function clearAll() external {
    uint slot;
    assembly {
        slot := a.slot
    }
    clearStorageSlot(slot);
    assembly {
        slot := b.slot
    }
    clearStorageSlot(slot);
  }

  function setAndClear() external {
    a = 2;
    b = 3;

    bytes32 key = bytes32(bytes1(0x01)) >> 248;
    bytes memory keyBytes = abi.encodePacked(key);

    (bool success, ) = STORAGE_ADDR.delegatecall(
        abi.encodeWithSelector(IStorage.clearStorage.selector, 0, true, keyBytes)
    );
  }

  function callSetAndClear() external {
    this.setVars();

    bytes32 key = bytes32(bytes1(0x01)) >> 248;
    bytes memory keyBytes = abi.encodePacked(key);

    (bool success, ) = STORAGE_ADDR.delegatecall(
        abi.encodeWithSelector(IStorage.clearStorage.selector, 0, true, keyBytes)
    );
  }

  function setAndCallClear() external {
    a = 2;
    b = 3;

    this.clear();
  }


  function setVars() external {
    a = 2;
    b = 3;
  }

  function clear() external {
    bytes32 key = bytes32(bytes1(0x01)) >> 248;
    bytes memory keyBytes = abi.encodePacked(key);

    (bool success, ) = STORAGE_ADDR.delegatecall(
        abi.encodeWithSelector(IStorage.clearStorage.selector, 0, true, keyBytes)
    );
  }
}

contract DepositDirect {
  uint256 a;
  uint256 b;

  function clearAll() external {
    a = 0;
    b = 0;
  }

  function setAndClear() external {
    a = 2;
    b = 3;
    b = 0;
  }

  function callSetAndClear() external {
    this.setVars();

    b = 0;
  }

  function setAndCallClear() external {
    a = 2;
    b = 3;

    this.clear();
  }


  function setVars() external {
    a = 2;
    b = 3;
  }

  function clear() external {
    b = 0;
  }
}