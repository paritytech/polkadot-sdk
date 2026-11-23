# Pallet JAM Coretime

`pallet-broker` on a Coretime chain running on JAM.

There is no relay chain on JAM. `JamCoretime` is the broker's `CoretimeInterface`: it sends every
core assignment to the Parachain Service as an `AssignCore` upward message. `AssignCore` queues
authorizer hashes instead of tasks, so every task registers the authorizer hash its cores are
queued with (`register_authorizer`), or schedules one for a JAM slot (`schedule_authorizer`).
Assignments the service rejects are sent again.

A runtime composes this pallet next to `pallet-broker` and sets the broker's `Coretime` to
`JamCoretime`. The sale and region logic is `pallet-broker`'s, unchanged.
