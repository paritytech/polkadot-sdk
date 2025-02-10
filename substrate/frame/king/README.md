# King Pallet for Substrate

This pallet implements subnet management functionality with King and Provider roles for blockchain networks.

## Overview

The King pallet provides functionality for:
- Creating and managing subnets
- King verification of providers
- Performance parameter management
- Custom verification types

## Key Components

### Roles
- **King**: Administrator who creates and manages subnets
- **Provider**: Participant who offers resources within a subnet

### Storage Items
```rust
pub type Subnets<T: Config> = StorageDoubleMap<...>;
pub type VerifiedProviders<T: Config> = StorageNMap<...>;
```

### Types

#### SubnetInfo
```rust
pub struct SubnetInfo<T: Config> {
    pub king: T::AccountId,
    pub title: BoundedVec<u8, T::MaxTitleLength>,
    pub performance_params: PerformanceParams,
    pub verification_type: VerificationType,
}
```

#### PerformanceParams
```rust
pub struct PerformanceParams {
    pub min_cpu_cores: u32,
    pub min_memory: u32,
    pub min_storage: u32,
}
```

#### VerificationType
```rust
pub enum VerificationType {
    Performance,
    Stake,
    Custom(BoundedVec<u8, ConstU32<100>>),
}
```

## Extrinsics

### create_subnet
Creates a new subnet with specified parameters.
```rust
fn create_subnet(
    origin,
    title,
    performance_params,
    verification_type
) -> DispatchResult
```

### verify_provider
Verifies a provider for participation in a subnet.
```rust
fn verify_provider(
    origin,
    subnet_id,
    provider
) -> DispatchResult
```

## Events
- `SubnetCreated`: Emitted when a new subnet is created
- `ProviderVerified`: Emitted when a provider is verified

## Errors
- `SubnetLimitReached`
- `SubnetNotFound`
- `ProviderAlreadyVerified`
- `UnauthorizedKing`

## Testing

The pallet includes comprehensive tests covering:
- Subnet creation
- Provider verification
- Error conditions
- Event emission

Run tests with:
```bash
cargo test
```

## Configuration

To use this pallet, include it in your runtime's `construct_runtime!` macro:
```rust
construct_runtime!(
    pub enum Runtime {
        King: pallet_king,
    }
);
```

### Runtime Configuration
```rust
impl pallet_king::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxTitleLength = ConstU32<100>;
    type MaxSubnetsPerKing = ConstU32<10>;
    type WeightInfo = WeightInfo;
}
```

## License
[Add your license information here]

## Contributing
[Add contributing guidelines here]

---

Note: This pallet is part of the Substrate framework and follows FRAME conventions for pallet development.