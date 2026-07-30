# Scarcity Claims

`pallet-scarcity-claims` converts one-time claim credits committed by a trusted source chain into
Scarcity NFTs. The source chain admits a voucher public key and credit hash into a compact binary
Merkle trie and sends only the root, root identifier, and leaf count to the destination chain.

A claim supplies the exact leaf, its Merkle proof, a destination purse key, a Scarcity collection,
and a voucher signature. The signature binds the claim to the destination chain's genesis hash,
root identifier, credit, collection, and destination, so a relayer may safely submit it without
being able to redirect the NFT.

After verification, the pallet reads the collection's current owner and asks the configured
collection selector to choose an item definition using the credit hash as entropy. The standard
selector adapter requires the owner to be a deployed `pallet-revive` contract and calls:

```solidity
function select(uint32 collection, bytes32 entropy) external returns (uint32 item);
```

The Revive call originates from the collection owner's own contract account and does not bump that
account's system nonce. This lets the contract distinguish the runtime self-call from ordinary
external callers. A stateful selector's storage deposit is paid by that owner account and
is bounded by the runtime's configured limit. Dispatch weight reserves the maximum proof work, the
generated Revive call base weight, and the configured execution limit. A successful claim refunds
the proof and contract components down to the actual proof depth and execution consumed.

The selected item is minted through `pallet_scarcity::MintWithoutDeposit`. Scarcity still enforces
the collection, item, supply, and one-NFT-per-purse invariants. Claims add no instance or
instance-metadata deposit; the trusted source protocol is the scarce resource bounding growth.

## Authority boundaries

- `RootOrigin` is the trust boundary for root ingestion. A production Asset Hub should configure
  it as the authenticated XCM origin of the Personhood chain root producer. `Root` is suitable
  only for development.
- Personhood decides which voucher keys receive credits. This pallet does not interpret an
  identity or `AccountOrPerson`.
- A Scarcity collection owner has full CRUD authority at the storage layer. When that owner is a
  contract, the contract is the programming and ACL layer for selection and any other collection
  operation. Normal account owners can continue managing collections manually, but the standard
  claims selector will not claim into them.
- Each credit hash is globally single-use, even across different roots and collections.

## Commitment and authorization formats

The Merkle trie is:

```text
BasicProvingTrie<BlakeTwo256, (sr25519::Public, H256), u32>
```

The key is `(voucher_public_key, credit_hash)` and the value is the source timestamp. The producer
must ensure that every credit hash is globally unique across every root it publishes. A reused hash
would already be spent if claimed from an earlier root and could prevent a later root from reaching
its claimed-complete count. `BasicProvingTrie` sorts keys by
their SCALE encoding through a `BTreeMap`; producers must use the exact SDK implementation or
reproduce that ordering and SCALE leaf encoding.

The signed message is the SCALE encoding of:

```text
(
  b"pallet-scarcity-claims/v1",
  destination_chain_genesis_hash,
  root_id,
  credit_hash,
  collection_id,
  destination_account,
)
```

The proof's embedded root, leaf count, key, and value must all match the submitted claim and stored
root record.

## Atomicity and retries

The credit is marked as in-progress before contract execution, preventing same-credit reentrancy.
Proof verification, contract selection, minting, and final claim accounting execute in one storage
transaction. Any failure rolls everything back, so a selector revert, unknown item, or occupied
destination does not consume the credit. Final root accounting re-reads live storage after the
selector returns, so a selector that submits a different valid claim reentrantly cannot have its
counter update overwritten by the outer claim.

Roots and successful claim records are retained. Root delivery is monotonic and idempotent:
re-delivering identical data succeeds, while conflicting or older new identifiers are rejected.

## Production status

This pallet has not been formally audited. It is intended for development and test networks and
should not be used in production until its source-chain admission, XCM origin, contract adapter,
weights, and adversarial behavior have received an independent security review.
