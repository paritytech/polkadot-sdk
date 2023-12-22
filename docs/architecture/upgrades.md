# Upgrades

The Polkadot side of our bridge is easily upgradable using forkless runtime upgrades. On the Ethereum side, it is more complicated, since smart contracts are immutable.

The gateway contract on Ethereum consists of a proxy and an implementation contract. Polkadot governance can send a cross-chain message to the Gateway, instructing it to upgrade to a new implementation contract.

##
