> NOTE: We have recently made significant changes to our repository structure. In order to streamline our development
process and foster better contributions, we have merged three separate repositories Cumulus, Substrate and Polkadot into
this repository. Read more about the changes [
here](https://polkadot-public.notion.site/Polkadot-SDK-FAQ-fbc4cecc2c46443fb37b9eeec2f0d85f).

# Polkadot SDK

![](https://cms.polkadot.network/content/images/2021/06/1-xPcVR_fkITd0ssKBvJ3GMw.png)

[![StackExchange](https://img.shields.io/badge/StackExchange-Community%20&%20Support-222222?logo=stackexchange)](https://substrate.stackexchange.com/)

The Polkadot SDK repository provides all the resources needed to start building on the Polkadot network, a multi-chain
blockchain platform that enables different blockchains to interoperate and share information in a secure and scalable
way. The Polkadot SDK comprises three main pieces of software:

## [Polkadot](./polkadot/)
[![PolkadotForum](https://img.shields.io/badge/Polkadot_Forum-e6007a?logo=polkadot)](https://forum.polkadot.network/)
[![Polkadot-license](https://img.shields.io/badge/License-GPL3-blue)](./polkadot/LICENSE)

Implementation of a node for the https://polkadot.network in Rust, using the Substrate framework. This directory
currently contains runtimes for the Polkadot, Kusama, Westend, and Rococo networks. In the future, these will be
relocated to the [`runtimes`](https://github.com/polkadot-fellows/runtimes/) repository.

## [Substrate](./substrate/)
 [![SubstrateRustDocs](https://img.shields.io/badge/Rust_Docs-Substrate-24CC85?logo=rust)](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/polkadot_sdk/substrate/index.html)
 [![Substrate-license](https://img.shields.io/badge/License-GPL3%2FApache2.0-blue)](./substrate/README.md#LICENSE)

Substrate is the primary blockchain SDK used by developers to create the parachains that make up the Polkadot network.
Additionally, it allows for the development of self-sovereign blockchains that operate completely independently of
Polkadot.

## [Cumulus](./cumulus/)
[![CumulusRustDocs](https://img.shields.io/badge/Rust_Docs-Cumulus-222222?logo=rust)](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/polkadot_sdk/cumulus/index.html)
[![Cumulus-license](https://img.shields.io/badge/License-GPL3-blue)](./cumulus/LICENSE)

Cumulus is a set of tools for writing Substrate-based Polkadot parachains.

## Releases

> [!NOTE]  
> Our release process is still Work-In-Progress and may not yet reflect the aspired outline here.

The Polkadot-SDK has two release channels: `stable` and `nightly`. Production software is advised to only use `stable`.
`nightly` is meant for tinkerers to try out the latest features. The detailed release process is described in
[RELEASE.md](docs/RELEASE.md).

### Stable

`stable` releases have a support duration of **three months**. In this period, the release will not have any breaking
changes. It will receive bug fixes, security fixes, performance fixes and new non-breaking features on a **two week**
cadence.

### Nightly

`nightly` releases are released every night from the `master` branch, potentially with breaking changes. They have
pre-release version numbers in the format `major.0.0-nightlyYYMMDD`.

## Upstream Dependencies

Below are the primary upstream dependencies utilized in this project:

- [`parity-scale-codec`](https://crates.io/crates/parity-scale-codec)
- [`parity-db`](https://crates.io/crates/parity-db)
- [`parity-common`](https://github.com/paritytech/parity-common)
- [`trie`](https://github.com/paritytech/trie)

## Security

The security policy and procedures can be found in [docs/contributor/SECURITY.md](./docs/contributor/SECURITY.md).

## Contributing & Code of Conduct

Ensure you follow our [contribution guidelines](./docs/contributor/CONTRIBUTING.md). In every interaction and
contribution, this project adheres to the [Contributor Covenant Code of Conduct](./docs/contributor/CODE_OF_CONDUCT.md).

## Additional Resources

- For monitoring upcoming changes and current proposals related to the technical implementation of the Polkadot network,
  visit the [`Requests for Comment (RFC)`](https://github.com/polkadot-fellows/RFCs) repository. While it's maintained
  by the Polkadot Fellowship, the RFC process welcomes contributions from everyone.
