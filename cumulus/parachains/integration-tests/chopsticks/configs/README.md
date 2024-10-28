This folder serves as a reference documentation for chopstick tests and the Parity-owned ecosystems,  
providing a working set of configuration files together with the WASM BLOBs, specifically for the Westend ecosystem.  
For additional resources and original Acala configurations, including documentation, refer to the [Acala repository](https://github.com/AcalaNetwork/chopsticks/tree/master/configs) or [Papermoon chopsticks overview](https://papermoonio.github.io/polkadot-ecosystem-docs-draft/dev-tools/chopsticks/overview/#using-a-configuration-file)

Config files, especially `wasm-override:` fields there, assume that there is a `wasm` folder within the same parent directory, and it contains pre-built WASM BLOBs of the 
ecosystem under tests.
