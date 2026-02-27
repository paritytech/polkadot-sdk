# Substrate statement store implementation.

> License: GPL-3.0-or-later WITH Classpath-exception-2.0

The statement store is an off-chain, decentralized data store for cryptographically signed statements.  It enables accounts to publish arbitrary data that can be queried and propagated across the network without consuming on-chain storage. Statement store designed around two fundamental pillars: Scalability and Graceful Degradation. It expressly avoids the properties of centralized data structures. Instead, it prioritizes privacy, scalability, and decentralization by deliberately reducing other guarantees.

**Unlike centralized services, the Statement Store is a  weakly coherent system and does not guarantee message delivery or specific delivery times.**


### How do I run a local statement-store node for development?
> This starts standalone substrate node with StatementStore turned on, and quota is set directly in the storage in the next step, without individuallity runtime
1. Build substrate-node

```bash
cargo build --profile production --locked --bin substrate-node --target x86_64-unknown-linux-gnu
```

2. Run it with:
```bash
RUST_LOG=info,statement-store=trace ./target/x86_64-unknown-linux-gnu/debug/substrate-node
```

3. Set quota using sudo, you can use this [extrinsic as template](https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/extrinsics/decode/0x160000040468555345525f594f55525f4143434f554e545f4b45595f48455245200a00000000500000)
> NOTE: link assumes that you are running node locally on 127.0.0.1:9944. Use this temaple if not `https://polkadot.js.org/apps/?rpc=wss://{URL_GOES_HERE}/relay/alice/extrinsics/decode/0x160000040468555345525f594f55525f4143434f554e545f4b45595f48455245200a00000000500000#/explorer`


### How do I get statement-store allowance for accounts?

1. Identify the account you use, it needs to be the same bytes you put in [here](https://github.com/paritytech/polkadot-sdk/blob/cac11f4a5325b217ca74b0c339459597daf03838/substrate/primitives/statement-store/src/lib.rs#L217)
2. Obtain the storage key by running this Python code. You would need your account ID from the previous step.
```python
>>> statement_allowance_key = lambda account_id_hex: "0x" + (b":statement_allowance:" + bytes.fromhex(account_id_hex.removeprefix("0x"))).hex()
>>> statement_allowance_key("YOUR_ACCOUNT_BYTES_IN_HEX")
```
3. Call sudo->system->set_storage, with obtained account key and StatementAllowance, scale encoded.  
For example, to allow an account to store 10 statements and a maximum of 20k you can use `0x0a00000000500000`. LLMs know how to answer this question if you want to use a different quota. 

> **Warning:** Use carefully. Do not set big quotas on test environments with SUDO because then they won't match production quotas.


### How to use SS with [subxt](https://github.com/paritytech/subxt)


**Submission**
`   fn submit(&self, encoded: Bytes) -> RpcResult<()>;`
```rust
let result: SubmitResult = rpc_client
					.request("statement_submit", rpc_params![encoded])
					.await
					.with_context(|| format!("Client {client_id}: Failed to submit statement"))?;
```
**Subscription**
`fn subscribe_statement(&self, topic_filter: TopicFilter);`

Parameters:
- topic_filter: Which topics to match. 
    - Use `TopicFilter::Any` to match all topics, `TopicFilter::MatchAll(vec)` to match statements that include all provided topics, 
    - or `TopicFilter::MatchAny(vec)` to match statements that include any of the provided topics.

```rust
let mut subscription: Subscription<Bytes> = rpc_client
			.subscribe(
				"statement_subscribeStatement",
				rpc_params![TopicFilter::MatchAny(bounded_topics)],
				"statement_unsubscribeStatement",
			)
			.await
			.with_context(|| format!("Client {client_id}: Failed to subscribe"))?;
```

##### Returns
When there are no matching statements in the store you first receive an empty array and as new matching statements arrive in the node they get forwarded to the client.
```json
{
    "jsonrpc": "2.0",
    "method": "statement_statement",
    "params": {
        "subscription": 4851578855668545,
        "result": {
            "event": "newStatements",
            "data": {
                "statements": [],
                "remaining": 0
            }
        }
    }
```    
If there are matching statements in the store you receive them in batches of newStatements events, with remaining telling you how many statements you have remaining, this guarantees you that the subscription will receive at least this amount of statements.
```json
{
    "jsonrpc": "2.0",
    "method": "statement_statement",
    "params": {
        "subscription": 1710164133533157,
        "result": {
            "event": "newStatements",
            "data": {
                "statements": [
                    "0x1000010000",
                    "0x100001000000"
                ],
                "remaining": 10
            }
        }
    }
```
If new statements arrive in the store they get delivered as they are without any remaining information.
```json
{
    "jsonrpc": "2.0",
    "method": "statement_statement",
    "params": {
        "subscription": 2661920166788434,
        "result": {
            "event": "newStatements",
            "data": {
                "statements": [                 
"0x100001000...000"
                ]
            }
        }
```

### Expiration and Maintenance:
##### Statement::expiry field
Message expiry field `Statement::expiry`, used for determining which statements to keep. The most significant 32 bits represent the expiration timestamp (in seconds since UNIX epoch) after which the statement gets removed. These ensure that statements with a higher expiration time have a higher priority.
The lower (LSB) 32 bits represent an arbitrary sequence number used to order statements with the same expiration time. Higher values indicate a higher priority.
This is used in two cases:
1) When an account exceeds its quota and some statements need to be removed. Statements with the lowest `expiry` are removed first.
	2) When multiple statements are submitted on the same channel, the one with the highest expiry replaces the one with the same channel.
Statements can be removed from the store in according to eviction policy:
Eviction: A higher-priority statement replaces a lower-priority one when constraints are exceeded. 
When removed, statements are marked as expired but remain in the index and database. Actual deletion occurs during the maintenance process, which runs every 30 seconds in a background task using Store::maintain(). Expired statements remain in the database for a configurable period (default 48 hours) to prevent resubmission during gossip propagation. 
