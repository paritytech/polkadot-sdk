# DoS attacks resilience

_This page is at the same time a small guide explaining how to handle DoS attacks, and explains the motivation behind some design decisions._

A denial-of-service (DoS) attack consists in consuming all the resources (bandwidth, CPU, or memory) of the target in order to prevent the target from properly serving legitimate users. From the point of view of the target, a DoS attack is the same thing as being under very heavy load.

Because the resources of the target are bounded, it is fundamentally impossible to claim to be able to resist to all DoS attacks. It is, however, possible to optimize and properly distribute resources consumption in order to increase the load that the target is capable of handling.

Additionally, and maybe more importantly, it is important to guarantee a good quality of service even when under heavy load.

## Bounded queues

In order to provide a good quality of service, the time between the moment a request is received by the server and the moment the response is sent back must be short. In order to achieve this, all queues should be small and bounded. The number of simultaneous I/O operations such as disk accesses must be small and bounded as well. As an example, if you try to read 5000 files at once, some of these file reads will take a long time, and the only way to guarantee that a file read will be short is to not start too many at the same time.

This is generally achieved by creating a certain number of threads (lightweight or not) dedicated to handling a specific I/O operation. By doing so, you are guaranteed that the number of these I/O operations that are being executed simultaneously will never exceed the number of threads.

When a queue of operations is full, the dispatcher should simply wait for some space to be available. This is called _back-pressure_, as the dispatcher is intentionally slowed down as well. If the dispatcher was responsible for processing some operations in a queue, that queue is slowed down as well, which might cause more back-pressure to be propagated.

This back-pressure must be propagated all the way to the TCP socket. When the code receiving requests from the TCP socket is unable to move forward because a queue is full, it should stop receiving data from the TCP socket. This will in turn cause the Linux kernel to not increase the TCP window size, which propagates the back-pressure to the JSON-RPC client.

Queues should be bounded such that, even in the worst case scenario, all requests are still being processed in a reasonable time. A bound that is too low can potentially lead to a decrease in performance due to an excess of context switches, but this is in practice rarely a problem.

In multiplexing situations, where multiple threads (lightweight or not) all try to insert elements in a single bounded queue, it is important for the distribution of items to have an acceptable distribution. It must not happen that some threads are able to insert elements very quickly while some others need to sleep for a long time before their element is inserted.

## Dispatching a notification shouldn't wait

Some JSON-RPC functions in the API lets clients subscribe to some event, such as a new block, meaning that the server must send a notification to the client when that specific event happens.

One important thing to keep in mind is that these events shouldn't be back-pressured. We do not want to slow down the reception of blocks by the node because the JSON-RPC server isn't capable of following the rhythm.

The events that happen on the node can be seen as a stream. This stream can briefly be back-pressured, but ideally as little as possible. Whatever tasks is on the receiving side of this stream of events should therefore consist only of CPU-only non-blocking operations.

However, sending a message to a client might take a long time, in case the client has (intentionally or not) little bandwidth. The tasks that are receiving the stream of events should never wait for a client to be ready before sending a notification to it. If the client isn't ready, then the notification must either be queued or simply discarded. Because queues must be bounded, it is unavoidable to have to discard some notifications.

Consequently, all functions that consist in sending notifications must be designed having in mind that the queue of notifications to send out must be bounded to a certain value. For example, the queue of notifications for `extrinsic_unstable_submitAndWatch` must have a size of 3. When the queue is full, new notifications must overwrite the notifications already in the queue. The design of all JSON-RPC functions should take into account the fact that this shouldn't result in a loss of important information for the JSON-RPC client.

## Distinguishing between light and heavy calls

When implementing bounded queues, one should avoid a situation where some elements in the queue are very quick to be processed while some others very slow.

During normal operations one will decide on the size of the queue and number of processing threads based on the time it takes to process an average queue element, but during a DoS attack the attacker can deliberately push a larger number of elements that are heavy to process. For example, with a single queue of incoming JSON-RPC requests, an attacker can intentionally submit requests that take a long time to process, in order to fill the queue more quickly.

For this reason, it is a good idea to split some queues, where light operations go on one queue and heavy operations on a different one.

This is part of the reason why functions are split by groups. In particular, the `archive`-prefixed JSON-RPC functions are, due to their disk access, considered as heavy and should be processed on a different queue than the other JSON-RPC functions. Under heavy load, it is likely that the processing of `archive`-prefixed JSON-RPC functions will be slowed down a lot, while the processing of other functions will be less impacted.

## Enforced limits

In order to limit the memory consumption and the latency of processing requests, it is important that requests that involve storing some state on the JSON-RPC server in the long term are subject to limits.

In particular:

- The number of JSON-RPC clients simultaneously connected must be bounded.
- The number of active subscriptions (i.e. where the server sends a notification to the client when something happens) must be bounded.
- The number of pinned blocks (in the context of `chainHead_unstable_follow`) must be bounded.

The limits on the number of active subscriptions and pinned block should be enforced per client, as it would be undesirable to limit the number of subscriptions/pinned blocks available to some clients just because other clients are using a lot of them. Since the number of clients is bounded, enforcing these limits per client also automatically enforces these limits globally.
