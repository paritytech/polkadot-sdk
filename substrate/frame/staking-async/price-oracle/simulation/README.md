## Simulation of Price Oracle

We want to have a price oracle system where stake-baked validators vote onchian to oraclize the price of an asset-pair onchain. We assume the validators are 2/3 honest. They use substrate's Offchain-worker functionality which automatically runs some code on every block. The code has access to HTTP endpoints.

For each price pair, we assume there are a number of endpoints available to query them from, a validators will query one randomly, such that if one is down, or wrong, we are resilient.

The price oracle system is intended to be a damped one, such that:
1. reacts somewhat slowly to abrupt price changes.
2. Moreover, since it is oracalized by the validators, we want to limit the powers of each validator to forcefully adjust the price.

We want to simulate a few scenarios and determine how fast the onchain price would react to it with different implementations. Moreover, see how each of the faults named below would affect them.

Suppose there is a known continuous canonical offchain price at each `time`.

```typescript
function canonicalPrice(time: number): number {}
```

And the blockchain has a fixed block-time interval `B` (time between block `N` and `N+1`) at which point it can update the price.
In time `t` (block `N`) until `t+B` (block `N+1`), validators have a chance to submit transactions to the chain, and the chain can update its notion of price at `t+B` or block `N+1` based on these votes.


## Questions

### Vote Definition?

Based on my understanding, the notion of the vote doesn't matter, and it should be the most expressive. For example, we have discussed that the validators can vote:

- `Down`: the price should go down, not specifying the amount
- `Down(amount)`: the price should go down by `amount`
- `Absolute(amount)`: the price _is_ `amount`.

Ultimately, the latter two are simply equal to one another, and can be interpreted by the chain as the former, if the chain wishes to do so (the chain can cast `Absolute(amouht)` to `Up` or `Down` based on the onchain price). The only difference is that `Down` is simply less expressive, while `Absolute` is most expressive.

### Vote Age

In this simulation, we are not taking into account long network delays for transaction propagation.
For simplicity, we are assuming that for the price update of block N+1, all relevant votes are from
the time period between block N to N+1. Votes pertaining to block N-1 are assumed to be invalidated.

Future simulations can improve this, and mimic old votes making it into the chain.

### How Many Votes?

In each block, how many validator votes should be processed? 1, up to all validator votes. Similar to the above, I argue that we should take as many votes as we can, as having more data can never be harmless, and can always be downcasted to a smaller equivalent. For example:
- We store the votes of all validators who managed to submit within the block such that we know which one pertains to which validator, so we have full freedom to tally them at the end of the block as we wish
  - We can tally all
  - We can tally the first N
  - We can tally a random subset of N

### How to Tally?

Then, the main open questions that remains is: How should we tally the votes
- The tools that we have in hand before tallying is
  - As named above in [[#How Many Votes?]], we can look at a subset of votes instead of all.
  - Throw away votes that are more than `MAX_VOTE_DIFF` away from the onchain price. It is likely that an API format has changed
  - Min votes: If we have less than `MIN_VOTE` votes, we don't tally and move on to the next block.
- Tally all votes
  - average them, and use at face value
  - average them, and move towards the new price by a `MAX_PRICE_MOVE` of that specific asset.
- Tally `C` random votes
  - average them, and use at face value
  - average them, and move towards the new price by a `MAX_PRICE_MOVE` of that specific asset.
- Take the high confidence ones based on
- [ ] More ideas... TODO

## Confidence

We can introduce the idea of "confidence" in feed and price. The confidence is a percentage number. Both follow a similar pattern where by default the confidence increases and caps at 1.

### Feed Confidence

- Validators vote (`downvote_feed`) to decrease the confidence in some feed by `X`
- At every block where no `downvote_feed` is present, the feed confidence is incremented.
- Once confidence reaches 0, it is not used by any of the validators anymore.
- We should model how fast it takes for validators to mark a bad with confidence `0` to figure out the right feed count.

### Price Confidence

- The final price of each asset also has an attached confidence metric.
- Each successful update in which more than 2/3 (or some other `X`) of validators contributed increases the confidence, capping it at `1`
  - Variance of votes can also be used to determine what degree of consensus existed on this price, influencing the confidence.
- Each block where the price is not updated, the price confidence drops by `X`
- The price and its confidence is what is reported to the rest of the system periodically (possibly at each block), allowing them to pause operations if the confidence falls below a threshold.

### Validator Confidence

Validators also have a confidence score, all set to 1 at first. Other validators may monitor the
votes of one another, and report a decrease in confidence. Once confidence hits `0` or `0 < X < 1`,
they are disabled and slashed.

## Resource Considerations

> Weight, Gas, Storage ops

Many of the other implementation in the space, given them being EVM contracts in chains with other activity, deploy various techniques to limit the resources. I believe a dedicated parachain is well enough for us to, in the worst case, ask all validators to submit votes in each block, and tally them each block, even if the price change has been small.

- [ ] To be validated, but my experience says doing a tally of only up to 600 votes per block is well within the boundaries of a single core parachain. We also have the option to go multi-core.

## Known Parameters

The system will work with:

- 600 validators
- 6s block times

## Further Ideas

Running the OCW in a TEE is only even beneficial, but takes a lot of engineering to do.

## What Could Go Wrong

### Non Malicious

> Particularly relevant as the OCW code parsing the API data is rather fixed; if the API format/limits change, updating it will be hard.
> A remedy is to provide a backup script that the validator can run next to their node, and it would use the session key to sign a tx using TS, but this is very encouraging to validators to faff with the script. TEE would really help here.

- No data: The api could be down -- validator has no votes
- No data: The api's return format has changed, and the validator's script cannot read it.
- Bad data: The api's return format has changed, and the validator's script misinterprets it into a wildly wrong number.
- Bad data: The api is malicious and returns a wildly wrong number

### Malicious
- Drifting malice: Validator always submits `price + delta` where `delta` is in her favor.
- Flash crash: validator reports `price * 100`
- Lazy: validator submits the same price as what is onchain to save resources.

