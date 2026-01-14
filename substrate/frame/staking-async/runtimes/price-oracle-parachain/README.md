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


#### Organization

Final zombienet config is: `zn-oracle.toml`. It runs:

1. `pallet-staking-async-rc-runtime`
2. `pallet-staking-async-price-oracle-parachain-runtime`
3. (and the WAH clone, called `pallet-staking-async-parachain-runtime`)

The new pallets are in the `price-oracle-parachain-runtime`. They are:

1. `price_oracle`: Where we run the offchain worker
2. `rc_client`: The one receiving validators from the RC, and acting as the local session manager of the price-oracle
   parachain.

#### How it works

We use pretty much only existing traits and mechanisms to forward new validator sets to the price-oracle and integrate
them in an existing session pallet, with a bit of gymnastics. It is explained in `price-oracle/src/lib.rs` docs.

#### Limitation and Next Steps

ZN atm is using the same session key as the stash keys. It should be altered to actually generate new session keys that
are not the same as `derive("Alice")` etc and put them in the keystore and register them. Alternatively, we can write
some scripts that at startup. Without this, our setup is not realistic

lots of TODOs are left in the code. Notably:

- [x] OCW RELIABILITY: OCWs are not running quite reliably on every block, it seems to be every other block
- [x] PRIORITY INC: Not all transactions are being included, some fail with priority. Priority should only ever go up.
- [x] LONGEVITY: Bump transctions should have low lengivity.
- [ ] OCWs should not overlap, add the lock mechanism from EPMB
- [x] Use some real HTTP endpoints
- [x] TRANSACTION DROPPING: need to know when and why transactions get dropped.
   - run with txpool=trace
   - run the txpool monitor.
   - Conclusion: Happens because we are sending new txs before the previous one is included.
   - [ ] can I trace the transaction status from within the OCW?
- [ ] TEST-VAL-SWAP: Test setup where RC swaps Bob with someone else.
- [ ] OPERATIONAL: Bump should be operational.
- [ ] SEND-TO-AH: Mechanism to send update to AH
- [ ] UNIT-TEST-SETUP: Unit test setup, we can use ahm-test, but it would be very good to mimic the runtime level stuff like signed tx
  generation, so better write it in the runtime.
- [ ] DESIGN: Who else should be able to transact on this chain? should we have a signed ext that will block all other origins
  other than current validators? Probably yes, because there are system remark and so on, and users can transact with
  them for free!
  - Or use system call filter? Might prevent governance interventions
  - Or a tx extension that will just check the call, and allow bump call only.
- [ ] XCM-CONFIG: Block all teleportation and so on to this chain. You should not be able to transfer funds to this chain to begin
  with. Maybe.
- [ ] DESIGN-WHEN-NUDGE: Options
  - All validators YOLO send at all blocks, increasing the priority (as it is now)
  - If feasible: All validators send and wait till it is included, then repeat
  - All validators can send at all blocks, but default OCW settings is such that they converge to one per block.
    - Or we can enforce this.
- [ ] Confidence on endpoints should be dropped and reported.
- [ ] How to represent price and bumps: FixedU128?
