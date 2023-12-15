# Validator Disabling

## Background

As established in the [approval process](protocol-approval.md) dealing with bad parablocks is a three step process:

1. Detection
1. Escalation
1. Consequences

The main system responsible for dispensing **consequences** for malicious actors is the [dispute system](protocol-disputes.md) which eventually dispenses slash events. It is important to note the **high latency** of the punishment as it is only effective after 27 eras (27 days in Polkadot) and does not immediately remove the validator from the active validator set. 

There is a need to have a more immediate way to deal with malicious validators. This is where the validator disabling comes in. It is focused on dispensing **low latency** consequences for malicious actors. It is important to note that the validator disabling is not a replacement for the dispute or slashing systems. It is a complementary system that is focused on lighter but immediate consequences usually in the form of restricted validator privileges.

The primary goals are:
- Eliminate cases where attackers can get free attempts at attacking the network
- Eliminate or minimize the risks of honest nodes being pushed out of consensus by getting unjustly slashed

The above two goals are generally at odds so a careful balance has to be struck between them. We will achieve them by sacrificing some **liveness** in favor of **soundness** when the network is under stress. Maintaining  some liveness but absolute soundness ia paramount.

Side goals are:
- Reduce the damages to honest nodes that had a fault which might cause repeated slashes

> **Note:** \
> Liveness = Valid candidates can go through (at a decent pace) \
> Security = Invalid candidates cannot go through (or are statistically very improbable)       

## System Overview

High level assumptions and goals of the validator disabling system that will be further discussed in the following sections:

- If validator gets slashed (even 0%) we disable him in the runtime and on the node side.
- We only disable up to 1/3 of the validators.
- If there are more offenders than 1/3 of the set disable only the highest offenders. (Some will get re-enabled.)
- Disablement lasts for 1 era.
- Disabled validators remain in the active validator set but have some limited permissions
- Disabled validators can no longer back candidates
- Disabled validators can participate in approval checking and their 'valid' votes behave normally. 'invalid' - votes do not automatically escalate into disputes but they are logged and stored so they will be taken into account if a dispute arises from at least 1 honest non-disabled validator.
- Disabling does not affect GRANDPA at all.
- Disabling affects Block Authoring. (Both ways: block authoring equivocation disables and disabling stops block authoring)

> **Note:** \
> Having the above elements allows us to simplify the design: 
> - No chilling of validators.
> - No Im-Online slashing.
> - No force new era logic.
> - No slashing spans

<br/><br/>

# Design

To better understand the design we will first go through what without validator disabling and what issues it can bring.

## Risks of NOT having validator disabling

A simple argument for disabling is that if someone is already slashed 100% and they have nothing to loose they could cause harm to the network and should be silenced.

What harm could they cause?

**1. Liveness attacks:**

- 1.1. Break sharding (with mass no-shows or mass disputes): It forces everyone to do all the work which affects liveness but doesn't kill it completely. The chain can progress at a slow rate.

- 1.2. Mass invalid candidate backing: Spawns a lot of worthless work that needs to be done but it is bounded by backing numbers. Honest backers will still back valid candidates and that cannot be stopped. Honest block authors will eventually select valid candidates and even if disputed they will win and progress the chain.

**2. Soundness attacks:**

- 2.1. The best and possibly only way to affect soundness is by getting lucky in the approval process. If by chance all approval voters would be malicious, the attackers could get a single invalid candidate through. Their chances would be relatively low but in general this risk has to be taken seriously as it significantly reduces the safety buffer around approval checking.

> **Note:**                
> With 30 approvals needed chance that a malicious candidate going through is around 4\*10^-15. Assuming attackers can back invalid candidates on 50 cores for 48 hours straight and only those candidates get included it still gives a 7\*10^-9 chance of success which is still relatively small considering the cost (all malicious stake slashed).

Attacks 1.2 and 2.1 should generally be pretty futile as a solo attacker while 1.1 could be possible with mass disputes even from a single attacker. Nevertheless whatever the attack vector within the old system* the attackers would get eventually get slashed and pushed out of the active validator set.

> **Note:**                
> \* In the old design validators were chilled in the era after committing an offense. Chilled validators were excluded from NPoS elections which resulted in them getting pushed out of the validator set within 1-2 eras. This was risky as it could push out honest nodes out of consensus if they were unjustly slashed but gives some time to react through governance or community action.

## Risks of having validator disabling

The primary risk behind having any sort of disabling is that it is a double-edged sword that in case of any dispute bugs or sources of PVF non-determinism could disable honest nodes or be abused by attackers to specifically silence honest nodes. Disabling honest nodes could tip the scales between honest and dishonest nodes and destabilize the protocol. Honest nodes being pushed out of consensus is primarily a problem for approval voting and disputes where a supermajority is required.

> **Note:**
> It is worth noting that is is fundamentally a defense in depth strategy because if we assume disputes are perfect it should not be a real concern. In reality disputes are difficult to get right, and non-determinism and happen so defense in depth is crucial when handling those subsystems.

## Addressing the risks TODO

**Risks of having validator disabling:**


**Risks of NOT having validator disabling:**

# ===============================================
# Other things I need to put somewhere
# ===============================================

Things to add:
- optional re-enabling (in what cases it helps)
- reasons why we disable for a full era
- confirmation trumping disablement
- reasons for not affecting grandpa
- reasons for affecting BA
- uncertainties around BEEFY
- problems with forcing new eras
- no-showing when disabled and similarity to a security analysis of a DoS attack on approvals
- accumulating slashes vs max slashing and disabling
- example attacks and how we defend from them
---

Above can be summarized as follows:

- Disputes & Slashing are a soundness requirement.

- Validator Disabling is **not** a soundness requirement but a liveness optimization.
 
---

Validator disabling and getting forced ouf of NPoS elections due to slashes have similar outcomes but there are a few differences:

- **latency** (next few blocks for validator disabling and 27 days for getting pushed out organically)
- **pool restriction** (validator disabling could effectively lower the number of active validators if we fully disable)
- **granularity** (validator disabling could remove only a portion of validator privileges instead of all)    

---

Disabling on minor slashes and accumulating slashes should both provide enough security as a deterrent against repeating offences, but disabling for minor offences is more lenient for honest faulty nodes and that's why we prefer it. Ideally we'd have both disabling AND accumulating as attackers can still commit multiple minor offences (for instance invalid on valid disputes) in the same block before they get punished and disabled, but damages done should be minimal so it's not a huge priority.

---

(not here but revise rest of guide)\
**Relevant Slashes: **
- backing invalid -> 100%
- valid on invalid -> 100%/k
- invalid on valid -> 0% (or very small slash)
- BA equivocation -> ? (w/e it is currently)