# Console Precompile

This directory contains the console precompile implementation for the Polkadot SDK Revive framework. The console precompile provides full compatibility with Foundry's `console.sol` library for debugging and development purposes.

## Address Allocation

**Address**: `0x000000000000000000000000000000000000000B` (decimal 11)

### Why Address 11 (0x0B)?

The console precompile is positioned at decimal 11 following a **sequential allocation strategy**:

```
Standard Ethereum Precompiles:
0x01 - EcRecover (ECDSA signature recovery)
0x02 - Sha256 (SHA-256 hash function)
0x03 - Ripemd160 (RIPEMD-160 hash function)
0x04 - Identity (memory copying)
0x05 - ModExp (modular exponentiation)
0x06 - Bn128Add (elliptic curve addition)
0x07 - Bn128Mul (elliptic curve multiplication)
0x08 - Bn128Pairing (elliptic curve pairing)
0x09 - Blake2F (Blake2b compression)
0x0A - PointEval (KZG point evaluation - unimplemented)
0x0B - Console (this precompile) ← Next available address
```

### Address Range Compliance

- **EIP-1352 Compliant**: Falls within reserved precompile range (0x0000-0xFFFF)
- **Builtin Range**: Uses address under `u16::MAX` as required by the framework
- **No Conflicts**: Passes collision detection with existing precompiles
- **Future-Proof**: Leaves room for additional builtin precompiles (0x0C-0xFFFF)

## Functionality

The console precompile implements **387+ function signatures** from Foundry's `console.sol`, providing comprehensive logging capabilities for smart contract development and debugging.

### Supported Operations
- `console.log()` with various parameter combinations
- `console.logInt()`, `console.logUint()`, `console.logString()`
- `console.logBytes()`, `console.logAddress()`, `console.logBool()`
- Mixed parameter logging (e.g., `console.log(string, uint256, address)`)

### Gas Costs
- Minimal gas cost using `RuntimeCosts::HostFn`
- Designed for development/debugging, not production use

## Usage

### From Solidity (with Foundry)
```solidity
import "forge-std/console.sol";

contract MyContract {
    function debug() public {
        console.log("Debug value:", 42);
        console.log("Address:", msg.sender);
        console.log("Multiple values:", 123, true, "test");
    }
}
```

### Direct Precompile Call
```solidity
// Direct call to precompile at address 0x0B
address CONSOLE_PRECOMPILE = 0x000000000000000000000000000000000000000B;
bytes memory data = abi.encodeWithSignature("log(string)", "Hello World");
(bool success,) = CONSOLE_PRECOMPILE.call(data);
```

## Compatibility

### Foundry Integration
- **100% Compatible**: All function signatures match `forge-std/console.sol`
- **No Migration Required**: Existing Foundry tests work without changes
- **Development Focused**: Optimized for debugging workflows

### Ethereum Compatibility
- **Standard Compliance**: Follows Ethereum precompile conventions
- **Address Safety**: Uses reserved precompile address range
- **EVM Compatibility**: Works with standard Ethereum tooling

## Architecture Context

### Precompile Categories in Revive Framework

1. **Standard Ethereum (0x01-0x09)**: Full Ethereum compatibility
2. **Extended Ethereum (0x0A)**: Placeholder for future Ethereum precompiles
3. **Development Tools (0x0B)**: Console precompile (this implementation)
4. **System Functions (0x900)**: Substrate-specific runtime functions
5. **Testing/Benchmarking (0xEFFF-0xFFFF)**: Internal development tools

### Address Allocation Strategy
```
0x01-0x0A: Standard/Extended Ethereum precompiles
0x0B-0x1F: Basic custom precompiles (Console here)
0x20-0xFF: Extended custom precompiles (available)
0x100-0x8FF: Advanced precompiles (available)
0x900-0x9FF: System precompiles
0xF000-0xFFFF: Testing/benchmarking
```

## Development
 
### Building
The console precompile is built as part of the revive runtime:
```bash
cargo build --package pallet-revive
```

### Testing
Tests are located in the module and cover:
- Function selector parsing
- Parameter encoding/decoding
- Gas cost calculations
- Foundry compatibility validation

### Contributing
When adding new console functions:
1. Add function signature to the dispatch table
2. Implement parameter parsing logic
3. Add corresponding tests
4. Update documentation

## Security Considerations

- **Development Only**: Not intended for production smart contracts
- **Gas Costs**: Minimal costs appropriate for debugging
- **Input Validation**: Proper bounds checking on all parameters
- **Error Handling**: Graceful failures for malformed input

## References

- [EIP-1352: Specify restricted address range for precompiles/system contracts](https://eips.ethereum.org/EIPS/eip-1352)
- [Foundry Console Documentation](https://book.getfoundry.sh/reference/forge-std/console-log)
- [Ethereum Precompile Standards](https://ethereum.org/en/developers/docs/smart-contracts/precompiled/)
