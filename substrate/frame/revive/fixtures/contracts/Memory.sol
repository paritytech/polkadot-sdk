// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract Memory {
    /// @notice Expands memory to the specified size by writing a byte at that offset
    /// @param memorySize The memory size in bytes to expand to
    function expandMemory(uint64 memorySize) public pure returns (bool success) {
        // Allocate memory by accessing a byte at the specified offset
        // This will trigger memory expansion up to at least memorySize + 1
        assembly {
            // Store a single byte (0xFF) at the memory offset
            // This forces the EVM to expand memory to accommodate this write
            mstore8(memorySize, 0xFF)
        }

        return false;
    }

    function testMemory() public {
        uint256 value = 0xfe;
        assembly {
            mstore(0, value)
        }
        uint256 result = 123;
        assembly {
            result := mload(0)
        }
        require(result == value, "Memory test failed");

        for (uint256 i = 0; i < 32; i++) {
            assembly {
                mstore8(i, value)
            }
        }
        assembly {
            result := mload(0)
        }
        require(result == 0xfefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefe, "Memory test failed");

        assembly {
            result := msize()
        }
        require(result == 96, "Memory size test failed");
    }

    function testMsize(uint64 offset) public returns (uint64) {
        assembly {
            mstore(offset, 123)
        }
        uint256 value;
        assembly {
            value := msize()
        }
        return uint64(value);
    }

    function testMcopy(uint64 dstOffset, uint64 offset, uint64 size, uint64 value) public returns (uint64) {
        assembly {
            mstore(dstOffset, 0)
        }
        for (uint256 i = 0; i < size; i += 32) {
            assembly {
                mstore(add(offset, i), value)
            }
        }
        assembly {
            mcopy(dstOffset, offset, size)
        }
        uint256 result = 123;
        assembly {
            result := mload(dstOffset)
        }
        return uint64(result);
    }

    // The functions below expand memory to a fixed size before the op-code under test. Calls that
    // differ only in the argument run the same op-codes, so they differ only in the charge that
    // the argument changes.

    /// Copies `len` bytes within the first KiB of memory, which is already expanded.
    function mcopyWithoutGrowth(uint256 len) public pure {
        assembly {
            mstore(0x3e0, 0)
            mcopy(0x100, 0x200, len)
        }
    }

    /// Stores a word at `offset` after expanding memory to 1 KiB.
    function mstoreAt(uint256 offset) public pure {
        assembly {
            mstore(0x3e0, 0)
            mstore(offset, 1)
        }
    }

    /// Returns the first `len` bytes of 64 KiB of expanded memory.
    function returnPrefix(uint256 len) public pure {
        assembly {
            mstore(0xffe0, 0)
            return(0, len)
        }
    }

    /// Copies `len` bytes of call data into the first KiB of memory, which is already expanded.
    function calldatacopyWithoutGrowth(uint256 len) public pure {
        assembly {
            mstore(0x3e0, 0)
            calldatacopy(0, 0, len)
        }
    }
}
