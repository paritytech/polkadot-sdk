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
///
/// Every loop body is one call, delegate call or create in assembly. Calls pass no input and copy
/// no output, so the loops of `SubCaller` and `SubCallerLarge` compile to nearly the same code.
contract SubCaller {
    function callN(address target, uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            assembly {
                if iszero(call(gas(), target, 0, 0, 0, 0, 0)) {
                    revert(0, 0)
                }
            }
        }
    }

    function callIgnoringN(address target, uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            assembly {
                pop(call(gas(), target, 0, 0, 0, 0, 0))
            }
        }
    }

    function delegateCallN(address target, uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            assembly {
                if iszero(delegatecall(gas(), target, 0, 0, 0, 0)) {
                    revert(0, 0)
                }
            }
        }
    }

    function createN(uint256 n) external {
        bytes memory code = type(Sink).creationCode;
        for (uint256 i = 0; i < n; i++) {
            assembly {
                if iszero(create(0, add(code, 32), mload(code))) {
                    revert(0, 0)
                }
            }
        }
    }
}

/// `SubCaller` with more code that its functions never run.
///
/// The extra functions do arithmetic on their arguments. They take and return single words,
/// with no arrays, strings or hashing.
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

    function mix4(uint256 x, uint256 y) external pure returns (uint256) {
        x = (x * 103 + y) ^ (y << 3);
        y = (y * 107 + x) ^ (x << 5);
        x = (x * 109 + y) & (y | 0xff00ff00ff00ff00);
        y = (y * 113 + x) | (x & 0x00ff00ff00ff00ff);
        x = (x * 127 + y) ^ (y << 7);
        y = (y * 131 + x) ^ (x << 11);
        x = (x * 137 + y) & (y | 0xf0f0f0f0f0f0f0f0);
        y = (y * 139 + x) | (x & 0x0f0f0f0f0f0f0f0f);
        x = (x * 149 + y) ^ (y << 13);
        y = (y * 151 + x) ^ (x << 17);
        x = (x * 157 + y) & (y | 0xcccccccccccccccc);
        y = (y * 163 + x) | (x & 0x3333333333333333);
        return x + y;
    }

    function mix5(int256 x, int256 y) external pure returns (int256) {
        x = (x * 223 - y) >> 3;
        y = (y * 227 - x) >> 5;
        x = (x < y ? x - y * 229 : y - x * 233) >> 7;
        y = (y > x ? y + x * 239 : x + y * 241) >> 11;
        x = (x * 251 - y) >> 13;
        y = (y * 257 - x) >> 17;
        x = (x < y ? x - y * 263 : y - x * 269) >> 19;
        y = (y > x ? y + x * 271 : x + y * 277) >> 23;
        x = (x * 281 - y) >> 29;
        y = (y * 283 - x) >> 31;
        x = (x < y ? x - y * 293 : y - x * 307) >> 37;
        y = (y > x ? y + x * 311 : x + y * 313) >> 41;
        return x ^ y;
    }

    function mix6(uint256 x, uint256 y) external pure returns (uint256) {
        x = (x ** 3 + y) % (y + 317);
        y = (y ** 5 + x) % (x + 331);
        x = (x ** 7 + y) % (y + 337);
        y = (y ** 3 + x) % (x + 347);
        x = (x ** 5 + y) % (y + 349);
        y = (y ** 7 + x) % (x + 353);
        x = (x ** 3 + y) % (y + 359);
        y = (y ** 5 + x) % (x + 367);
        x = (x ** 7 + y) % (y + 373);
        y = (y ** 3 + x) % (x + 379);
        return x * y;
    }

    function mix7(uint256 x, uint256 y) external pure returns (uint256) {
        x = (x * 383 + 389) ^ (y >> 3) ^ (y << 61);
        y = (y * 397 + 401) ^ (x >> 5) ^ (x << 59);
        x = (x * 409 + 419) ^ (y >> 7) ^ (y << 57);
        y = (y * 421 + 431) ^ (x >> 11) ^ (x << 53);
        x = (x * 433 + 439) ^ (y >> 13) ^ (y << 51);
        y = (y * 443 + 449) ^ (x >> 17) ^ (x << 47);
        x = (x * 457 + 461) ^ (y >> 19) ^ (y << 45);
        y = (y * 463 + 467) ^ (x >> 23) ^ (x << 41);
        x = (x * 479 + 487) ^ (y >> 29) ^ (y << 35);
        y = (y * 491 + 499) ^ (x >> 31) ^ (x << 33);
        x = (x * 503 + 509) ^ (y >> 37) ^ (y << 27);
        y = (y * 521 + 523) ^ (x >> 41) ^ (x << 23);
        return x - y;
    }
}
