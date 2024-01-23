# Polkadot

We use Polkadot’s[ BEEFY](https://github.com/paritytech/grandpa-bridge-gadget/blob/master/docs/walkthrough.md) gadget to implement an efficient light client that only needs to verify a very small subset of relay chain validator signatures. BEEFY is live on Rococo, and is awaiting deployment on Kusama and Polkadot.

Fundamentally, the BEEFY light client allows the bridge to prove that a specified parachain header was finalized by the relay chain.

We want a bridge design that is light enough to deploy on Ethereum. It will be too expensive to verify signatures from say 1000 validators of the Polkadot relay chain on Ethereum, so we basically have two choices: verify all signatures in succinct proofs or only verify a few signatures. We settled for a design that tries to make the latter cryptoeconomically secure.

The ideal security to aim for is for an attack to be as expensive as the smaller market cap of DOT and ETH. Unfortunately, we can only slash the bond of the few validators whose signatures are verified, so any attack attempt is necessarily much cheaper than the whole market cap. However, we can aim to make an attack very expensive in expectation by making sure that an attack succeeds with low probability and that failed attacks still cost the attackers.

## Update Protocol

The light client needs to be frequently updated with new BEEFY commitments by an untrusted permissionless set of relayers.

BEEFY commitments are signed by relay chain validators. The light client needs to verify these signatures before accepting commitments.

In collaboration with W3F, we have designed a protocol where the light client needs to only verify $$N$$ signatures samples from randomly chosen validators​. The choice of $$N$$ is done dynamically based on a few variables and is described [here](./#signature-sampling).

In the EVM there is no cryptographically secure source of randomness. Instead, we make our update protocol crypto-economically secure through an interactive update protocol. In this protocol, a candidate commitment is verified over 3 transactions. At a high level it works like this:

1. `submitInitial` - In the first transaction, the relayer submits the commitment, a randomly selected validator signature, and an initial bitfield claiming which validators have signed the commitment.
2. The relayer must then wait [MAX\_SEED\_LOOKAHEAD](https://eth2book.info/bellatrix/part3/config/preset/#max\_seed\_lookahead) blocks.
3. `commitPrevRandao` - The relayer submits a second transaction to reveal and commit to a random seed, derived from Ethereum's [RANDAO](https://eips.ethereum.org/EIPS/eip-4399).
4. The relayer requests from the light client a bitfield with $$N$$randomly chosen validators sampled from the initial bitfield.​
5. `submitFinal` - The relayer sends a third and final transaction with signatures for all the validators specified in the final bitfield
6. The light client verifies all validator signatures in the third transaction to ensure:
   1. The provided validators are in the current validator set
   2. The provided validators are in the final bitfield
   3. The provided validators have signed the beefy commitment
7. If the third transaction succeeds then the payload inside the BEEFY commitment is applied

## Signature Sampling

The choice $$N$$ is described by the [formal analysis of signature sampling from W3F](https://hackmd.io/c6STzrvfQGyN2P2rVmTmoA). It consists of the following variables.

$$
N = \lceil log_2(R * V * \frac{1}{S} *(75+E)*172.8)\rceil + 1 + 2 \lceil log_2(C) \rceil
$$

1. $$V$$ - Validator set length.
2. $$C$$ - The number of times a validator's signature was previously used for `submitInitial` calls within a session. There is no limit to how many times `submitInitial` can be called except for its gas cost. This allows an adversary to spam this transaction in order to gain influence over the RANDAO provided they can pay for gas. The light client will track how many times a validator signature is used when calling `submitInitial` in a session and increase the number of validator signatures required to be verified when finalizing the commitment. This will make finalizing the commitment cost more gas and would require more validators to back dishonest claims and be slashed by the BEEFY protocol.
3. $$E$$ - RANDAO commit expiry. The number of blocks a relayer has to commit to a random seed based on RANDAO.
4. The ratio of the total supply of DOT to the minimum amount slashable. These are done using two heuristic variables.
   1. $$R$$ - The ratio of total stake per validator.
   2. $$S$$ - A slash rate which is the percentage of a validator's stake that can be slashed.
5. Constant $$75$$ is the number of slots that an adversary can use to influence RANDAO. See formal analysis for more details.
6. Constant $$172.8$$ is the expected number of choices an adversary has to influence the RANDAO based on Markov chain analysis by W3F. See formal analysis for more details.

From the list above 1 and 2 are known in the light client and can be calculated on-chain. Variables 3, 4.1, and 4.2 are not known by the light client and are instead calculated off-chain and set as a minimum number of required signatures during the initialization of the light client. This minimum is immutable for the life time of the light client.

* [Minimum required signatures](../../../../contracts/src/BeefyClient.sol#L185-L190)
* [Dynamic signature calculation](../../../../contracts/src/BeefyClient.sol#L444)
* [Python implementation of required signatures](../../../../scripts/beefy\_signature\_sampling.py#L9)

## Message Verification

On our parachain, outbound channels periodically emit message commitment hashes which are inserted into the parachain header as a digest item. These commitment hashes are produced by hashing a set of messages submitted by end users.

To verify these commitment hashes, the light client side needs the following information

1. The full message bundle
2. Partial parachain header
3. A merkle leaf proof for the parachain header containing the commitment hash for (1)
4. An MMR leaf proof for the MMR leaf containing the merkle root for the merkle tree in (2)

Working backwards, if the BEEFY light client successfully verifies a parachain header, then the commitment hash within that header is also valid, and the messages mapping to that commitment hash can be safely dispatched.

## Implementation

Solidity Contracts:

* [BeefyClient.sol](../../../../contracts/src/BeefyClient.sol)
* [Verification.sol](../../../../contracts/src/Verification.sol)
