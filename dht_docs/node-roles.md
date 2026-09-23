# Node roles

**Full node** — participates in all four affinity aspects: DHT routing, DHT
storing, explicit routing, and explicit storing. Declares explicit affinity.

**Light node** — pure consumer for constrained environments (browser, mobile).
Declares explicit affinity and stores only the topics the user wants; does
**not** route or store on behalf of others, and has no DHT affinity. See
[[light-node-protocol]].

| Aspect | Full | Light |
| :-- | :-- | :-- |
| DHT routing | yes | no |
| DHT storing | yes | no |
| Explicit routing | yes | no |
| Explicit storing | configurable* | yes |
| Declares explicit affinity | yes | yes |

\*Sensible for statically configured affinity, not for short-lived RPC
subscriptions ([[explicit-affinity-sources]]).

Whether the local node is DHT-eligible feeds [[peers-topology]].
