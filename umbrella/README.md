<div align="center">

![SDK Logo](../docs/images/Polkadot_Logo_Horizontal_Pink_White.png#gh-dark-mode-only)
![SDK Logo](../docs/images/Polkadot_Logo_Horizontal_Pink_Black.png#gh-light-mode-only)

# `polkadot-sdk`

<!-- markdownlint-disable-next-line MD013 -->
[![StackExchange](https://img.shields.io/badge/StackExchange-Community%20&%20Support-222222?logo=stackexchange)](https://substrate.stackexchange.com/)

</div>

`polkadot-sdk` is an umbrella crate for the [Polkadot SDK](https://github.com/paritytech/polkadot-sdk), in the sense that it is an "umbrella" that encompasses other components. More specifically, it re-exports all other published crates in the SDK, except for the example and fuzzing, plus some of the runtime crates.

`polkadot-sdk` aims to be an entry to the Polkadot and Substrate ecosystem and make the SDK more approachable—the entire development environment made available with one dependency. More importantly, it guarantees the right combination of crate versions so that third-party tools for selecting compatible crate versions are no longer necessary.

## 💻 Usage

The re-exported crates are grouped under the following feature sets.

- `node`
- `runtime`
- `std`
- `experimental`
- `runtime-benchmarks`
- `runtime-full`
- `serde`
- `tuples-96`
- `try-runtime`
- `with-tracing`

Choose which ones to enable based on the needs. For example, when building a runtime, the `Cargo.toml` may contain the following. To build a runtime that can run on a wide variety of environments, you may want to opt out of `std` with `default-features = false`.

```toml
[dependencies]
polkadot-sdk = { version = "0.12.0", features = ["runtime"], default-features = false }
```

In addition to the features above, each re-exported crate is individually feature-gated. For situations where only a small subset of crates is needed, you may consider specifying the exact set of crates you need to reduce build time.

In the contrived example below, only the `frame-support` and `frame-system` crates are needed from `polkadot-sdk`.

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

To learn more about building with the Polkadot SDK, you may start with these [guides](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/index.html).

## 💡 Pro Tips

(Any suggestions?)

In Substrate, a runtime can be seen as a tuple of various pallets. Since the number of pallets varies and there is no way to anticipate it, we have to generate impl-trait for tuples of different sizes upfront, from 0-tuple to 64-tuple to be specific (64 is chosen to balance usability and compile time).

Seldomly, if the runtime grows larger than 64 pallets, the trait implementations will no longer apply, then the feature `tuples-96` (or even `tuples-128`) is required (at the cost of increased compile time).

## 🚀 Versioning

We do a stable release for the SDK every three months with a version schema reflecting the release cadence. At the time of writing, the latest version is `stable2412` (released in 2024 December). To avoid confusion, we will align the versioning of `polkadot-sdk` with the established schema. For instance, the next stable version will be `2503.0.0`.
