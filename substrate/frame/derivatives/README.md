# derivatives

The purpose of the `pallet-derivatives` is to cover the following derivative asset support
scenarios:

  1. The `pallet-derivatives` can serve as an API for creating and destroying derivatives.
  2. It can store a mapping between the foreign original ID (e.g., XCM `AssetId` or `(AssetId, AssetInstance)`)
  and the local derivative ID.

The scenarios can be combined.

## Motivation

The motivation differs depending on the scenario in question.

### The first scenario

The `pallet-derivatives` can be helpful when another pallet, which hosts the derivative assets,
doesn't provide a good enough way to create new assets in the context of them being derivatives.

For instance, the asset hosting pallet might have an asset class (NFT collection or fungible currency) creation extrinsic,
but among its parameters, there could be things like some admin account, currency decimals, various permissions, etc.

When creating a regular (i.e., non-derivative) asset class via such an extrinsic,
these parameters allow one to conveniently set all the needed data for the asset class.
However, when creating a derivative asset class, we usually can't allow an arbitrary user to
influence such parameters since they should be set per the original asset class owner's desires.

Thus, we can either require a privileged origin for derivative asset classes (such as Root or some collective)
or we could provide an alternative API where the sensitive parameters are omitted (and set by the chain runtime automatically).

The first approach dominates in the ecosystem at the moment since:
  1. It is simple
  2. There was no pallet to make such an alternative API without rewriting individual
     asset-hosting pallets
  3. Only fungible derivatives were ever made (with rare exceptions like an NFT derivative
     collection on Karura).

The fungible derivatives are one of the reasons because they almost always have at least
decimals and symbol information that should be correct, so only a privileged origin is
acceptable to do the registration, since there is no way (at the time of writing) to communicate
asset data between chains directly (this will be fixed when Fellowship RFC 125 will be implemented).

Derivative NFT collections and their tokens, on the other hand, just need to point to the
originals. An NFT derivative is meant to participate in mechanisms unique to the given hosting
chain, such as NFT fractionalization, nesting, etc., where only its ID is needed to do said interactions.

In the future, there could be interactions where NFT data is needed. These interactions will be
able to leverage XCM Asset Metadata instructions from Fellowship RFC 125.
However, even with the IDs only, there are use cases (as mentioned above), and more could be discovered.
Requiring a privileged origin where no sensitive parameters are needed for registering derivative NFT collections
is raising an unreasonable barrier for NFT interoperability between chains.
So, providing an API for unprivileged derivative registration is a preferable choice in this case.

Moreover, the future data communication via XCM can benefit both fungible and non-fungible
derivative collections registration.
  1. The `create_derivative` extrinsic of this pallet can be configured to initiate the
     registration process
  by sending the `ReportMetadata` instruction to the reserve chain. It can be configured such that
  this can be done by anyone.
  2. The reserve chain will decide whether to send the data or an error depending on its state.
  3. Our chain will handle the reserve chain's response and decide whether it is okay to register
     the given asset.

### The second scenario

Saving the mapping between the original ID and the derivative ID is needed when their types
differ and the derivative ID value can't be deterministically deduced from the original ID.
This situation can arise in the following cases:
  * The original ID type is incompatible with a derivative ID type.
  For example, let `pallet-nfts` instance host derivative NFT collections. We can't set the
  `CollectionId` (the derivative ID type) to XCM `AssetId` (the original ID type)
  because `pallet-nfts` requires `CollectionId` to be incrementable.
  * It is desired to have a continuous ID space for all objects, both derivative and local.
  For instance, one might want to reuse the existing pallet combinations (like `pallet-nfts`
  instance + `pallet-nfts-fractionalization` instance) without adding new pallet instances between
  the one hosting NFTs and many special logic pallets. In this case, the original ID type would be
  `(AssetId, AssetInstance)`, and the derivative ID type can be anything.

## Usage examples

The `src/mock/mod.rs` contains a mock runtime declaration that contains several instances of the pallet
to test the scenarios mentioned above.
This test configuration can be viewed as a usage example alongside the tests in the `src/tests.rs`.
