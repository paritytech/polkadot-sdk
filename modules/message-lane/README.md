# Message Lane Module

The Message Lane Module is used to deliver messages from source to target chain. Message is (almost) opaque to the module and the final goal is to hand message to the message dispatch mechanism.

## Overview

*In progress*

## Weights of module extrinsics

The main assumptions behind weight formulas is:
- all possible costs are paid in advance by the message submitter;
- whenever possible, relayer tries to minimize cost of its transactions. So e.g. even though sender always pays for delivering outbound lane state proof, relayer may not include it in the delivery transaction (unless message lane module on target chain requires that);
- weight formula should incentivize relayer to not to submit any redundand data in the extrinsics arguments;
- the extrinsic shall never be executing slower (i.e. has larger actual weight) than defined by the formula.

### Weight of `send_message` call

#### Related benchmarks

| Benchmark                         | Description                                            |
|-----------------------------------|--------------------------------------------------------|
| `send_minimal_message_worst_case` | Sends 0-size message with worst possible conditions    |
| `send_1_kb_message_worst_case`    | Sends 1KB-size message with worst possible conditions  |
| `send_16_kb_message_worst_case`   | Sends 16KB-size message with worst possible conditions |

#### Weight formula

The weight formula is:
```
Weight = BaseWeight + MessageSizeInKilobytes * MessageKiloByteSendWeight
```

Where:

| Component                   | How it is computed?                                                          | Description                                                                                                                                  |
|-----------------------------|------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------|
| `SendMessageOverhead`       | `send_minimal_message_worst_case`                                            | Weight of sending minimal (0 bytes) message                    |
| `MessageKiloByteSendWeight` | `(send_16_kb_message_worst_case - send_1_kb_message_worst_case)/15` | Weight of sending every additional kilobyte of the message |

### Weight of `receive_messages_proof` call

#### Related benchmarks

| Benchmark                                               | Description*                                                                                                            |
|---------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------|
| `receive_single_message_proof`                          | Receives proof of single `EXPECTED_DEFAULT_MESSAGE_LENGTH` message                                                      |
| `receive_two_messages_proof`                            | Receives proof of two identical `EXPECTED_DEFAULT_MESSAGE_LENGTH` messages                                              |
| `receive_single_message_proof_with_outbound_lane_state` | Receives proof of single `EXPECTED_DEFAULT_MESSAGE_LENGTH` message and proof of outbound lane state at the source chain |
| `receive_single_message_proof_1_kb`                     | Receives proof of single message. The proof has size of approximately 1KB**                                             |
| `receive_single_message_proof_16_kb`                    | Receives proof of single message. The proof has size of approximately 16KB**                                            |

*\* - In all benchmarks all received messages are dispatched and their dispatch cost is near to zero*

*\*\* - Trie leafs are assumed to have minimal values. The proof is derived from the minimal proof by including more trie nodes. That's because according to `receive_message_proofs_with_large_leaf` and `receive_message_proofs_with_extra_nodes` benchmarks, increasing proof by including more nodes has slightly larger impact on performance than increasing values stored in leafs*.

#### Weight formula

The weight formula is:
```
Weight = BaseWeight + OutboundStateDeliveryWeight + MessagesCount * MessageDeliveryWeight + MessagesDispatchWeight + Max(0, ActualProofSize - ExpectedProofSize) * ProofByteDeliveryWeight
```

Where:

| Component                     | How it is computed?                                                                      | Description                                                                                                                                                                                                                                                                                                                                                                                         |
|-------------------------------|------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `BaseWeight`                  | `2*receive_single_message_proof - receive_two_messages_proof`                            | Weight of receiving and parsing minimal proof                                                                                                                                                                                                                                                                                                                                                       |
| `OutboundStateDeliveryWeight` | `receive_single_message_proof_with_outbound_lane_state - receive_single_message_proof`   | Additional weight when proof includes outbound lane state                                                                                                                                                                                                                                                                                                                                           |
| `MessageDeliveryWeight`       | `receive_two_messages_proof - receive_single_message_proof`                              | Weight of of parsing and dispatching (without actual dispatch cost) of every message                                                                                                                                                                                                                                                                                                                |
| `MessagesCount`               |                                                                                          | Provided by relayer                                                                                                                                                                                                                                                                                                                                                                                 |
| `MessagesDispatchWeight`      |                                                                                          | Provided by relayer                                                                                                                                                                                                                                                                                                                                                                                 |
| `ActualProofSize`             |                                                                                          | Provided by relayer                                                                                                                                                                                                                                                                                                                                                                                 |
| `ExpectedProofSize`           | `EXPECTED_DEFAULT_MESSAGE_LENGTH * MessagesCount + EXTRA_STORAGE_PROOF_SIZE`             | Size of proof that we are expecting. This only includes `EXTRA_STORAGE_PROOF_SIZE` once, because we assume that intermediate nodes likely to be included in the proof only once. This may be wrong, but since weight of processing proof with many nodes is almost equal to processing proof with large leafs, additional cost will be covered because we're charging for extra proof bytes anyway  |
| `ProofByteDeliveryWeight`     | `(receive_single_message_proof_16_kb - receive_single_message_proof_1_kb) / (15 * 1024)` | Weight of processing every additional proof byte over `ExpectedProofSize` limit                                                                                                                                                                                                                                                                                                                     |

#### Why for every message sent using `send_message` we will be able to craft `receive_messages_proof` transaction?

We have following checks in `send_message` transaction on the source chain:
- message size should be less than or equal to `2/3` of maximal extrinsic size on the target chain;
- message dispatch weight should be less than or equal to the `1/2` of maximal extrinsic dispatch weight on the target chain.

Delivery transaction is an encoded delivery call and signed extensions. So we have `1/3` of maximal extrinsic size reserved for:
- storage proof, excluding the message itself. Currently, on our test chains, the overhead is always within `EXTRA_STORAGE_PROOF_SIZE` limits (1024 bytes);
- signed extras and other call arguments (`relayer_id: SourceChain::AccountId`, `messages_count: u32`, `dispatch_weight: u64`).

On Millau chain, maximal extrinsic size is `0.75 * 2MB`, so `1/3` is `512KB` (`524_288` bytes). This should be enough to cover these extra arguments and signed extensions.

Let's exclude message dispatch cost from single message delivery transaction weight formula:
```
Weight = BaseWeight + OutboundStateDeliveryWeight + MessageDeliveryWeight + Max(0, ActualProofSize - ExpectedProofSize) * ProofByteDeliveryWeight
```

So we have `1/2` of maximal extrinsic weight to cover these components. `BaseWeight`, `OutboundStateDeliveryWeight` and `MessageDeliveryWeight` are determined using benchmarks and are hardcoded into runtime. Adequate relayer would only include required trie nodes into the proof. So if message size would be maximal (`2/3` of `MaximalExtrinsicSize`), then the extra proof size would be `MaximalExtrinsicSize / 3 * 2 - EXPECTED_DEFAULT_MESSAGE_LENGTH`.

Both conditions are verified by `pallet_message_lane::ensure_weights_are_correct` and `pallet_message_lane::ensure_able_to_receive_messages` functions, which must be called from every runtime' tests.

### Weight of `receive_messages_delivery_proof` call

#### Related benchmarks

| Benchmark                                                   | Description                                                                              |
|-------------------------------------------------------------|------------------------------------------------------------------------------------------|
| `receive_delivery_proof_for_single_message`                 | Receives proof of single message delivery                                                |
| `receive_delivery_proof_for_two_messages_by_single_relayer` | Receives proof of two messages delivery. Both messages are delivered by the same relayer |
| `receive_delivery_proof_for_two_messages_by_two_relayers`   | Receives proof of two messages delivery. Messages are delivered by different relayers    |

#### Weight formula

The weight formula is:
```
Weight = BaseWeight + MessagesCount * MessageConfirmationWeight + RelayersCount * RelayerRewardWeight + Max(0, ActualProofSize - ExpectedProofSize) * ProofByteDeliveryWeight
```

Where:

| Component                 | How it is computed?                                                                                                   | Description                                                                                                                                                                                             |
|---------------------------|-----------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `BaseWeight`              | `2*receive_delivery_proof_for_single_message - receive_delivery_proof_for_two_messages_by_single_relayer`             | Weight of receiving and parsing minimal delivery proof                                                                                                                                                  |
| `MessageDeliveryWeight`   | `receive_delivery_proof_for_two_messages_by_single_relayer - receive_delivery_proof_for_single_message`               | Weight of confirming every additional message                                                                                                                                                           |
| `MessagesCount`           |                                                                                                                       | Provided by relayer                                                                                                                                                                                     |
| `RelayerRewardWeight`     | `receive_delivery_proof_for_two_messages_by_two_relayers - receive_delivery_proof_for_two_messages_by_single_relayer` | Weight of rewarding every additional relayer                                                                                                                                                            |
| `RelayersCount`           |                                                                                                                       | Provided by relayer                                                                                                                                                                                     |
| `ActualProofSize`         |                                                                                                                       | Provided by relayer                                                                                                                                                                                     |
| `ExpectedProofSize`       | `EXTRA_STORAGE_PROOF_SIZE`                                                                                            | Size of proof that we are expecting                                                                                                                                                                     |
| `ProofByteDeliveryWeight` | `(receive_single_message_proof_16_kb - receive_single_message_proof_1_kb) / (15 * 1024)`                              | Weight of processing every additional proof byte over `ExpectedProofSize` limit. We're using the same formula, as for message delivery, because proof mechanism is assumed to be the same in both cases |

#### Why we're always able to craft `receive_messages_delivery_proof` transaction?

There can be at most `<PeerRuntime as pallet_message_lane::Config>::MaxUnconfirmedMessagesAtInboundLane` messages and at most `<PeerRuntime as pallet_message_lane::Config>::MaxUnrewardedRelayerEntriesAtInboundLane` unrewarded relayers in the single delivery confirmation transaction.

We're checking that this transaction may be crafted in the `pallet_message_lane::ensure_able_to_receive_confirmation` function, which must be called from every runtime' tests.
