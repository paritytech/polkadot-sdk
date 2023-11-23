# Validator Disabling

## Background

As established in the [approval process](protocol-approval.md) dealing with bad parablocks is a three step process:

1. Detection
1. Escalation
1. Consequences

The main system responsible for dispensing consequences for malicious actors is the [dispute system](protocol-disputes.md) which eventually dispenses slash events which will be applied in the next era. It is important to note the **high latency** of the punishment as it is only effective at the start of the next era (24h in Polkadot) and does not immediately remove the validator from the active validator set. 

There is a need to have a more immediate way to deal with malicious validators. This is where the validator disabling comes in. It is focused on dispensing **low latency** consequences for malicious actors. It is important to note that the validator disabling is not a replacement for the dispute system. It is a complementary system that is focused on lighter but immediate consequences usually in the form of restricted validator privileges.

Validator disabling and getting forced out at the end of an era due to slashes have similar outcomes but there are a few differences:

- **latency** (next few blocks for validator disabling and 24-48h for getting pushed out organically)
- **pool restriction** (validator disabling can lower the number of active validators if we fully disable)
- **granularity** (validator disabling could remove only a portion of validator privileges instead of all)

## Risks of NOT having validator disabling

A simple argument for disabling is that if someone is already slashed 100% and they have nothing to loose they could cause harm to the network and should be silenced.

What harm could they cause?

**1. Liveness attacks:**

- Break sharding (with mass no-shows or mass disputes): It forces everyone to do all the work which affects liveness but doesn't kill it completely. The chain can progress at a slow rate.
- Mass invalid candidate backing: Spawns a lot of worthless work that needs to be done but it is bounded by backing numbers. Honest backers will still back valid candidates and that cannot be stopped. Honest block authors will eventually select valid candidates and even if disputed they will win and progress the chain.

**2. Security attacks:**

- The best and possibly only way to affect security is by getting lucky in the approval process. If by chance all approval voters would be malicious, the attackers could get a single invalid candidate through. Their chances would be relatively low but in general this risk has to be taken seriously as it significantly reduces the safety buffer around approval checking.

> **Note:**                
> With 30 approvals needed chance for that a malicious candidate going through is around 4\*10^-15. Assuming attackers can back invalid candidates on 50 cores for 48 hours straight and only those candidates get included it still gives a 7\*10^-9 chance of success which is still relatively small considering the cost (all malicious stake slashed).

The risk of above attacks can be possibly mitigated with more immediate measures such as validator disabling but the strategy has to be very carefully designed to not introduce new attack vectors.

## Risks of validator disabling

The primary risk behind having any sort of disabling is that it is a double-edged sword that in case of any dispute bugs could disable honest nodes or be abused by attackers to specifically silence honest nodes. Disabling honest nodes could tip the scales between honest and dishonest nodes and destabilize the protocol. Honest nodes being pushed out of consensus is primarily a problem for approval voting and disputes where a supermajority is required.

It is worth noting that is is fundamentally a defense in depth strategy because if we assume disputes are perfect it should not be a real concern. In reality disputes are difficult to get right, and non-determinism and happen so defense in depth is crucial when handling those subsystems.

> **Note:**               
> What about slashes with no validator direct disabling?   
> Slashing by itself is less of a problem due to its high latency of getting pushed out of the validator set. It still affects the honest slashed node in the short term (lost funds), but if the slash was truly unjustified the governance should refund the tokens after an investigation. So generally in the long term no harm will be done. It gives 24-48 hours to react in those cases which is at least a small buffer further escalate if an attack pushing out honest nodes out of consensus would show up. The pushed out validator will also be swapped out for another random validator which most likely will be honest.

# ===============================================

Above can be summarized as follows:

- Disputes & Slashing are a security requirement.

- Validator Disabling is **not** a security requirement but a liveness optimization.

> **Note:** 
> - Security = Invalid candidates cannot go through (or are statistically very improbable)
> - Liveness = Valid candidates can go through (at a decent pace)