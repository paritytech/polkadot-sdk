<div align="center">

![SDK Logo](../docs/images/Polkadot_Logo_Horizontal_Pink_White.png#gh-dark-mode-only)
![SDK Logo](../docs/images/Polkadot_Logo_Horizontal_Pink_Black.png#gh-light-mode-only)

# `polkadot-sdk`

<!-- markdownlint-disable-next-line MD013 -->
[![StackExchange](https://img.shields.io/badge/StackExchange-Community%20&%20Support-222222?logo=stackexchange)](https://substrate.stackexchange.com/)

</div>

`polkadot-sdk` is an umbrella crate for the [Polkadot SDK](https://github.com/paritytech/polkadot-sdk), in the sense that it is an "umbrella" that encompasses other components. More specifically, it re-exports all other published crates in the SDK, except for the example and fuzzing, plus some of the runtime crates. `polkadot-sdk` aims to be an entry to the Polkadot and Substrate ecosystem and make the SDK more approachable—the entire development environment made available with one dependency. More importantly, it guarantees the right combination of crate versions so that third-party tools for selecting compatible crate versions are no longer necessary.

## 💻 Usage

The re-exported crates are grouped under the following feature sets.

- `node`
- `runtime`
- `runtime-full`
- `experimental`
- `runtime-benchmarks`
- `serde`
- `tuples-96`
- `try-runtime`
- `with-tracing`

When using `polkadot-sdk` to build a node, it is a good start to enable the `node` feature.

```toml
[dependencies]
polkadot-sdk = { version = "0.12.0", features = ["node"] }
```

For a runtime implementation, you need the `runtime` feature instead. Besides, you may want to opt out of `std` with `default-features = false` to allow the runtime to be executed in environments where `std` isn't available.

```toml
[dependencies]
polkadot-sdk = { version = "0.12.0", features = ["runtime"], default-features = false }
```

The other features above are meant to be used as accessories to `node`, `runtime`, or `runtime-full`. For example, if the runtime needs benchmarking, you can enable `runtime-benchmarks` optionally in your own feature.

```toml
[dependencies]
polkadot-sdk = { version = "0.12.0", features = ["runtime"], default-features = false }

[features]
runtime-benchmarks = ["polkadot-sdk/frame-benchmarks"]
```

```shell
$ cargo build --features runtime-benchmarks
```

In addition to the features above, each re-exported crate is feature-gated individually to give more informed users fine-grained control over the dependencies. If you know exactly which crates are needed, you may consider specifying the crate names in the `features` list to reduce build time.

In the contrived example below, only the `frame-support` and `frame-system` crates are needed from `polkadot-sdk`:

```toml
[dependencies]
polkadot-sdk = { version = "0.12.0", features = ["frame-support", "frame-system"] }
```

```rust
// `frame_support` and `frame_system` are declared here
use polkadot_sdk::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    // `frame_support` and `frame_system` are imported in the outer module,
    // but are not automatically inherited here. Need to "re-import" to make
    // them available in the inner module.
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

To learn more about building with the Polkadot SDK, you may start with these [guides](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/index.html) and our [official docs](https://docs.polkadot.com/).

## 💡 Pro Tips

In Substrate, a runtime can be seen as a tuple of various pallets. Since the number of pallets can  vary and there is no way to anticipate it, we have to generate impl-trait for tuples of different sizes upfront, from 0-tuple to 64-tuple to be specific (64 is chosen to balance between usability and compile time). Seldomly, when the runtime grows to have more than 64 pallets, the trait implementations will cease to apply, then the feature `tuples-96` (or even `tuples-128`) must be enabled (at the cost of increased compile time).

```toml
[dependencies]
polkadot-sdk = { version = "0.12.0", features = ["runtime", "tuples-96"], default-features = false }
```

## 🚀 Versioning

We do a stable release for the SDK every three months with a version schema reflecting the release cadence, which is tracked in the [release registry](https://github.com/paritytech/release-registry/). At the time of writing, the latest version is `stable2412` (released in 2024 December). To avoid confusion, we will align the versioning of `polkadot-sdk` with the established schema. For instance, the next stable version will be `2503.0.0`.
