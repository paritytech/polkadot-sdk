> NOTE: We have recently made significant changes to our repository structure. In order to 
streamline our development process and foster better contributions, we have merged three separate 
repositories Cumulus, Substrate and Polkadot into this repository. Read more about the changes [
here](https://polkadot-public.notion.site/Polkadot-SDK-FAQ-fbc4cecc2c46443fb37b9eeec2f0d85f).

# Polkadot SDK

![](https://cms.polkadot.network/content/images/2021/06/1-xPcVR_fkITd0ssKBvJ3GMw.png)

The Polkadot SDK repository provides all the resources needed to start building on the Polkadot 
network, a multi-chain blockchain platform that enables different blockchains to interoperate and 
share information in a secure and scalable way. The Polkadot SDK comprises three main pieces of 
software:

### [Polkadot](./polkadot/)

Implementation of a https://polkadot.network node in Rust based on the Substrate framework. This 
directory contains runtimes for the Polkadot, Kusama, and Westend networks. 

### [Substrate](./substrate/)

Substrate is the primary blockchain SDK used by developers to create the parachains that make up 
the Polkadot network.

### [Cumulus](./cumulus/)
Cumulus is a set of tools for writing Substrate-based Polkadot parachains.

## Contributing

Ensure you follow our [contribution guidelines](./docs/CONTRIBUTING.md). In every interaction and contribution, this project adheres to the [Contributor Covenant Code of Conduct](./docs/CODE_OF_CONDUCT.md).