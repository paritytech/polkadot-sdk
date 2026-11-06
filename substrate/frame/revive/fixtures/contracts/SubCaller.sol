// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.0;

/// Accepts every call.
contract Sink {
    fallback() external payable {}
}

/// Reverts every call.
contract Reverter {
    fallback() external payable {
        revert();
    }
}

/// Calls, delegate calls or creates contracts a given number of times.
contract SubCaller {
    function callN(address target, uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            (bool success, ) = target.call("");
            require(success, "call failed");
        }
    }

    function callIgnoringN(address target, uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            (bool success, ) = target.call("");
            success;
        }
    }

    /// Like `callIgnoringN`, but does not copy the return data.
    function callDroppingOutputN(address target, uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            assembly {
                pop(call(gas(), target, 0, 0, 0, 0, 0))
            }
        }
    }

    function delegateCallN(address target, uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            (bool success, ) = target.delegatecall("");
            require(success, "delegate call failed");
        }
    }

    function createN(uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            new Sink();
        }
    }
}

/// `SubCaller` with more code that its functions never run.
contract SubCallerLarge is SubCaller {
    function mix(uint256 x) external pure returns (uint256) {
        x = (x * 31 + 7) ^ (x >> 3);
        x = (x * 37 + 11) ^ (x >> 5);
        x = (x * 41 + 13) ^ (x >> 7);
        x = (x * 43 + 17) ^ (x >> 11);
        x = (x * 47 + 19) ^ (x >> 13);
        x = (x * 53 + 23) ^ (x >> 17);
        x = (x * 59 + 29) ^ (x >> 19);
        x = (x * 61 + 31) ^ (x >> 23);
        x = (x * 67 + 37) ^ (x >> 29);
        x = (x * 71 + 41) ^ (x >> 31);
        x = (x * 73 + 43) ^ (x >> 37);
        x = (x * 79 + 47) ^ (x >> 41);
        x = (x * 83 + 53) ^ (x >> 43);
        x = (x * 89 + 59) ^ (x >> 47);
        x = (x * 97 + 61) ^ (x >> 53);
        x = (x * 101 + 67) ^ (x >> 59);
        return x;
    }

    function mix2(uint256 x, uint256 y) external pure returns (uint256) {
        x = (x + y * 3) / (y % 7 + 1);
        y = (y + x * 5) / (x % 11 + 1);
        x = (x + y * 7) / (y % 13 + 1);
        y = (y + x * 11) / (x % 17 + 1);
        x = (x + y * 13) / (y % 19 + 1);
        y = (y + x * 17) / (x % 23 + 1);
        x = (x + y * 19) / (y % 29 + 1);
        y = (y + x * 23) / (x % 31 + 1);
        x = (x + y * 29) / (y % 37 + 1);
        y = (y + x * 31) / (x % 41 + 1);
        x = (x + y * 37) / (y % 43 + 1);
        y = (y + x * 41) / (x % 47 + 1);
        return x ^ y;
    }

    function mix3(int256 x, int256 y) external pure returns (int256) {
        x = (x - y * 3) / (y % 7 + 8);
        y = (y - x * 5) / (x % 11 + 12);
        x = (x - y * 7) / (y % 13 + 14);
        y = (y - x * 11) / (x % 17 + 18);
        x = (x - y * 13) / (y % 19 + 20);
        y = (y - x * 17) / (x % 23 + 24);
        x = (x - y * 19) / (y % 29 + 30);
        y = (y - x * 23) / (x % 31 + 32);
        x = (x - y * 29) / (y % 37 + 38);
        y = (y - x * 31) / (x % 41 + 42);
        x = (x - y * 37) / (y % 43 + 44);
        y = (y - x * 41) / (x % 47 + 48);
        return x - y;
    }

    function hashes(bytes calldata data) external pure returns (bytes32 a, bytes32 b, bytes32 c) {
        a = keccak256(data);
        b = keccak256(abi.encodePacked(a, data.length));
        c = keccak256(abi.encode(a, b, data));
        a = keccak256(abi.encodePacked(c, b, a));
        b = keccak256(abi.encode(a, c, data.length, data));
        c = keccak256(abi.encodePacked(b, a, c, data));
    }

    function strings(string calldata s, uint256 n) external pure returns (string memory out) {
        out = s;
        for (uint256 i = 0; i < n; i++) {
            out = string(abi.encodePacked(out, s, bytes1(uint8(48 + (i % 10)))));
        }
    }

    function fold(uint256[] calldata values) external pure returns (uint256 acc) {
        for (uint256 i = 0; i < values.length; i++) {
            acc = (acc * 131 + values[i]) % 1000000007;
            acc ^= acc << 13;
            acc ^= acc >> 7;
            acc ^= acc << 17;
        }
    }

    function sortFirst(uint256[8] memory values) external pure returns (uint256[8] memory) {
        for (uint256 i = 0; i < values.length; i++) {
            for (uint256 j = i + 1; j < values.length; j++) {
                if (values[j] < values[i]) {
                    (values[i], values[j]) = (values[j], values[i]);
                }
            }
        }
        return values;
    }
}
