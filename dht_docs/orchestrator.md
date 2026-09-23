# Orchestrator

Issue #11937. The glue module (today's `substrate/client/network/statement`,
adjusted) that drives the new modules. Responsibilities:

- receive statements and decide store/forward using [[peers-topology]] and
  [[explicit-affinity]] ([[statement-storage]], [[forwarding-decision]]),
- feed peer events into [[peers-topology]] to keep it current,
- update [[explicit-affinity]] when an RPC subscription opens or closes,
- call [[peer-steering]] periodically to choose connect/disconnect,
- decide which peers to accept statement-protocol connections from
  ([[peer-connection-model]], #12272),
- migrate statements as nodes join or are detected gone ([[node-join-sync]]).
