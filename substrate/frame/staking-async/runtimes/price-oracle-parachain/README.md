# Price Oracle Parachain

> Quick and dirty note about what I have done so far in case someone wants to push this forward in December 2025.

#### What

First, what is this folder? This is `staking-async/runtimes`, a clone of westend/asset-hub-westend that we have heavily
altered to test staking in AHM. For simplicity, I have put my prototype here.

I have a helper script that runs everything. All you need to do is to make sure `zombienet` is in your path. Then:

1. go to `cd substrate/frame/staking-async/runtimes/papi-tests`
2. `just setup`
3. `bun run src/index.ts price-oracle`

Make sure you have the latest version zombienet, as it contains some fixes that are needed to work here.

> Note: The fix to not use `//stash` as the default is not published. Talk to Javier, he is aware. Without this, even
> though on the relay chain


#### Organization

Final zombienet config is: `zn-oracle.toml`. It runs:

1. `pallet-staking-async-rc-runtime`
2. `pallet-staking-async-price-oracle-parachain-runtime`
3. (and the WAH clone, called `pallet-staking-async-parachain-runtime`)

The new pallets are in the `price-oracle-parachain-runtime`. They are:

1. `price_oracle`: Where we run the offchain worker

#### How it works

We use pretty much only existing traits and mechanisms to forward new validator sets to the price-oracle. It is
explained in `price-oracle/src/lib.rs` docs.

#### Limitation and Next Steps

ZN atm is using the same session key as the stash keys. It should be altered to actually generate new session keys that
are not the same as `derive("Alice")` etc and put them in the keystore and register them. Alternatively, we can write
some scripts that at startup. Without this, our setup is not realistic

lots of TODOs are left in the code. Notably:

1. We need a tx extension that either does `CheckNonce` or mimics it. Currently it doesn't work.
2. Make our `bump` tx `Operational`, making sure even if some manic moves DOTs to this chain and remark-spams it, it is pointless.
3. Work out what the longevity of the `bumps` should be. `CheckNonce` will help as the new ones will invalidate old
   ones. Currently we stack too many txs even with two nodes.
4. No mechanism yet exists to send price update to AH. It will be simple though.
5. Start writing unit tests, ala `ahm-tests` crate which simulates two an RC and a parachain.
