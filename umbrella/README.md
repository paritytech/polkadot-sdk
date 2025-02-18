# `polkadot-sdk`

`polkadot-sdk` is an umbrella crate re-exporting all other published crates, except for the example and fuzzing, plus some of the runtime crates like Rococo. Consider it the entry to the whole Polkadot and Substrate ecosystem.

`polkadot-sdk` aims to make the SDK more approachable - the entire development environment made available with one dependency. More importantly, it guarantees the right combination of crate versions, thus third-party tools for selecting compatible crate versions are no longer needed.

## Usage

The re-exported crates are grouped under the following feature sets.

- `std`: for enabling `std` support of the enabled features
- `experimental`: for experimental features
- `node`: for node implementation
- `runtime`: for runtime implementation
- `runtime-benchmarks`: for benchmarking runtimes
- `runtime-full`: for more comprehensive runtime implementation?
- `serde`: for `serde` en/decoding support
- `tuples-96`: required by runtimes with more than 64 pallets
- `try-runtime`: for cli support?
- `with-tracing`: for tracing support

Choose which ones to enable based on the needs. For example, when building a runtime with more than 64 pallets, benchmarking needs and optional tracing, the `Cargo.toml` may contain

```toml
[dependencies]
polkadot-sdk = { version = "0.12.0", features = ["runtime", "tuples-96"] }

[features]
runtime-benchmarks = ["polkadot-sdk/runtime-benchmarks"]
with-tracing = ["polkadot-sdk/with-tracing"]
```

```shell
# build enabling runtime-benchmarks and with-tracing
$ cargo build --features "runtime-benchmarks,with-tracing"
```

## Pro Tips

* fine-grained control over dependencies
* any suggestions?

In addition to the features above, each re-exported crate is feature-gated individually so that one can control exactly which crates to include. In the contrived example below, only the `frame-support` and `frame-system` crates are needed.

```toml
[dependencies]
polkadot-sdk = { version = "0.12.0", features = ["frame-support", "frame-system"] }
```

```rust
// `frame_support` and `frame_system` are declared here
use polkadot_sdk::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    pub type Balance = u128;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    #[pallet::disable_frame_system_supertrait_check]
    pub trait Config: frame_system::Config {}

    #[pallet::storage]
    pub type Balances<T: Config> = StorageMap<_, _, T::AccountId, Balance>;

    impl<T: Config> Pallet<T> {
        pub fn transfer(
            from: T::RuntimeOrigin,
            to: T::AccountId,
            amount: Balance,
        ) -> DispatchResult {
            let sender = ensure_signed(from)?;
            let sender_balance = Balances::<T>::get(&sender).ok_or("NonExistentAccount")?;
            let sender_remainder = sender_balance
                .checked_sub(amount)
                .ok_or("InsufficientBalance")?;

            Balances::<T>::mutate(to, |b| *b = Some(b.unwrap_or(0) + amount));
            Balances::<T>::insert(&sender, sender_remainder);

            Ok(())
        }
    }
}
```

## Versioning

We do a stable release for the SDK quarterly with a version scheme reflecting the release cadence. At the time of writing, the latest version is `stable2412` (released on December 2024). To avoid confusion, we plan to align with the established scheme for the umbrella crate. For example, the next stable version of `polkadot-sdk` will be `2503.0.0`.
