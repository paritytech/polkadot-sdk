<div align="center">

![SDK Logo](./docs/images/Polkadot_Logo_Horizontal_Pink_White.png#gh-dark-mode-only)
![SDK Logo](./docs/images/Polkadot_Logo_Horizontal_Pink_Black.png#gh-light-mode-only)

# Polkadot SDK

![GitHub stars](https://img.shields.io/github/stars/paritytech/polkadot-sdk)&nbsp;&nbsp;![GitHub
forks](https://img.shields.io/github/forks/paritytech/polkadot-sdk)

<!-- markdownlint-disable-next-line MD013 -->
[![StackExchange](https://img.shields.io/badge/StackExchange-Community%20&%20Support-222222?logo=stackexchange)](https://substrate.stackexchange.com/)&nbsp;&nbsp;![GitHub contributors](https://img.shields.io/github/contributors/paritytech/polkadot-sdk)&nbsp;&nbsp;![GitHub commit activity](https://img.shields.io/github/commit-activity/m/paritytech/polkadot-sdk)&nbsp;&nbsp;![GitHub last commit](https://img.shields.io/github/last-commit/paritytech/polkadot-sdk)

> The Polkadot SDK repository provides all the components needed to start building on the
> [Polkadot](https://polkadot.network) network, a multi-chain blockchain platform that enables
> different blockchains to interoperate and share information in a secure and scalable way.

</div>

## ⚡ Quickstart
If you want to get an example node running quickly you can execute the following getting started script:

```
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/paritytech/polkadot-sdk/master/scripts/getting-started.sh | bash
```

## 📚 Documentation

* [🦀 rust-docs](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/index.html)
  * [Introduction](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/polkadot_sdk/index.html)
	to each component of the Polkadot SDK: Substrate, FRAME, Cumulus, and XCM
  * [Guides](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/index.html),
	namely how to build your first FRAME pallet
  * [Templates](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/polkadot_sdk/templates/index.html)
    for starting a new project.
  * [External Resources](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/external_resources/index.html)

## 🚀 Releases

<!-- markdownlint-disable-next-line MD013 -->
![Current Stable Release](https://raw.githubusercontent.com/paritytech/release-registry/main/badges/polkadot-sdk-latest.svg)&nbsp;&nbsp;![Next Stable Release](https://raw.githubusercontent.com/paritytech/release-registry/main/badges/polkadot-sdk-next.svg)

The Polkadot SDK is released every three months as a `stableYYMMDD` release. They are supported for
one year with patches. See the next upcoming versions in the [Release
Registry](https://github.com/paritytech/release-registry/).

You can use [`psvm`](https://github.com/paritytech/psvm) to update all dependencies to a specific
version without needing to manually select the correct version for each crate.

## 🛠️ Tooling

[Polkadot SDK Version Manager](https://github.com/paritytech/psvm):
A simple tool to manage and update the Polkadot SDK dependencies in any Cargo.toml file.
It will automatically update the Polkadot SDK dependencies to their correct crates.io version.

## 🔐 Security

The security policy and procedures can be found in
[docs/contributor/SECURITY.md](./docs/contributor/SECURITY.md).

## 🤍 Contributing & Code of Conduct

Ensure you follow our [contribution guidelines](./docs/contributor/CONTRIBUTING.md). In every
interaction and contribution, this project adheres to the [Contributor Covenant Code of
Conduct](./docs/contributor/CODE_OF_CONDUCT.md).

### 👾 Ready to Contribute?

Take a look at the issues labeled with [`mentor`](https://github.com/paritytech/polkadot-sdk/labels/C1-mentor)
(or alternatively [this](https://mentor.tasty.limo/) page, created by one of the maintainers) label to get started!
We always recognize valuable contributions by proposing an on-chain tip to the Polkadot network as a token of our
appreciation.

## Polkadot Fellowship

Development in this repo usually goes hand in hand with the `fellowship` organization. In short,
this repository provides all the SDK pieces needed to build both Polkadot and its parachains. But,
the actual Polkadot runtime lives in the `fellowship/runtimes` repository. Read more about the
fellowship, this separation, the RFC process
[here](https://polkadot-fellows.github.io/dashboard/).

## History

This repository is the amalgamation of 3 separate repositories that used to make up Polkadot SDK,
namely Substrate, Polkadot and Cumulus. Read more about the merge and its history
[here](https://polkadot-public.notion.site/Polkadot-SDK-FAQ-fbc4cecc2c46443fb37b9eeec2f0d85f).
