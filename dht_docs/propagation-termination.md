# Propagation termination

§3.4.3. DHT routing converges because each node forwards to its k closest
connected peers ([[dht-affinity-routing]]), whose sets increasingly overlap as
the statement nears the topic's neighbourhood. Propagation stops when receivers
have already seen the statement and drop it ([[duplicate-tracking]]).

To keep hop count low, nodes should be well-connected to their own XOR
neighbourhood, minimising sends to peers that won't store the statement.
