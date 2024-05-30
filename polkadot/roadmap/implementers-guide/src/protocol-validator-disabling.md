# Validator Disabling

## Background

As established in the [approval process](protocol-approval.md) dealing with bad parablocks is a three step process:

1. Detection
1. Escalation
1. Consequences

The main system responsible for dispensing **consequences** for malicious actors is the [dispute
system](protocol-disputes.md) which eventually dispenses slash events. The slashes itself can be dispensed quickly (a
matter of blocks) but for an extra layer of auditing all slashes are deferred for 27 days (in Polkadot/Kusama) which
gives time for Governance to investigate and potentially alter the punishment. Dispute concluding by itself does not
immediately remove the validator from the active validator set.

> **Note:** \
> There was an additional mechanism of automatically chilling the validator which removed their intent to participate in
> the next election, but the removed validator could simply re-register his intent to validate.

There is a need to have a more immediate way to deal with malicious validators. This is where the validator disabling
comes in. It is focused on dispensing **low latency** consequences for malicious actors. It is important to note that
the validator disabling is not a replacement for the dispute or slashing systems. It is a complementary system that is
focused on lighter but immediate consequences usually in the form of restricted validator privileges.

The primary goals are:
- Eliminate or minimize cases where attackers can get free attempts at attacking the network
- Eliminate or minimize the risks of honest nodes being pushed out of consensus when getting unjustly slashed (defense
  in depth)

The above two goals are generally at odds so a careful balance has to be struck between them. We will achieve them by
sacrificing some **liveness** in favor of **soundness** when the network is under stress. Maintaining some liveness but
absolute soundness is paramount.

> **Note:** \
> Liveness = Valid candidates can go through (at a decent pace) \
> Security = Invalid candidates cannot go through (or are statistically very improbable)

Side goals are:
- Reduce the damages to honest nodes that had a fault which might cause repeated slashes
- Reduce liveness impact of individual malicious attackers

## System Overview

High level assumptions and goals of the validator disabling system that will be further discussed in the following
sections:

1. If validator gets slashed (even 0%) we mark them as disabled in the runtime and on the node side.
1. We only disable up to byzantine threshold of the validators.
1. If there are more offenders than byzantine threshold disable only the highest offenders. (Some might get re-enabled.)
1. Disablement lasts for 1 era.
1. Disabled validators remain in the active validator set but have some limited permissions.
1. Disabled validators can get re-elected.
1. Disabled validators can participate in approval checking.
1. Disabled validators can participate in GRANDPA/BEEFY, but equivocations cause disablement.
1. Disabled validators cannot author blocks.
1. Disabled validators cannot back candidates.
1. Disabled validators cannot initiate disputes, but their votes are still counted if a dispute occurs.
1. Disabled validators making dispute statements no-show in approval checking.

</br></br></br>

# Risks

## Risks of NOT having validator disabling

Assume that if an offense is committed a slash is deposited but the perpetrator can still act normally. He will be
slashed 100% with a long delay (slash deferral duration which is 27 days). This is akin to the current design.

A simple argument for disabling is that if someone is already slashed 100% and they have nothing to lose they could
cause harm to the network and should be silenced.

What harm could they cause?

**1. Liveness attacks:**

- 1.1. Break sharding (with mass no-shows or mass disputes): It forces everyone to do all the work which affects
  liveness but doesn't kill it completely. The chain can progress at a slow rate.

- 1.2. Mass invalid candidate backing: Spawns a lot of worthless work that needs to be done but it is bounded by backing
  numbers. Honest backers will still back valid candidates and that cannot be stopped. Honest block authors will
  eventually select valid candidates and even if disputed they will win and progress the chain.

**2. Soundness attacks:**

- 2.1. The best and possibly only way to affect soundness is by getting lucky in the approval process. If by chance all
  approval voters would be malicious, the attackers could get a single invalid candidate through. Their chances would be
  relatively low but in general this risk has to be taken seriously as it significantly reduces the safety buffer around
  approval checking.

> **Note:** With 30 approvals needed chance that a malicious candidate going through is around 4\*10^-15. Assuming
> attackers can back invalid candidates on 50 cores for 48 hours straight and only those candidates get included it
> still gives a 7\*10^-9 chance of success which is still relatively small considering the cost (all malicious stake
> slashed).

Attacks 1.2 and 2.1 should generally be pretty futile as a solo attacker while 1.1 could be possible with mass disputes
even from a single attacker. Nevertheless whatever the attack vector within the old system the attackers would get
*eventually* get slashed and pushed out of the active validator set but they had plenty of time to wreck havoc.

## Risks of having validator disabling

Assume we fully push out validator when they commit offenses.

The primary risk behind having any sort of disabling is that it is a double-edged sword that in case of any dispute bugs
or sources of PVF non-determinism could disable honest nodes or be abused by attackers to specifically silence honest
nodes.

Validators being pushed out of the validator set are an issue because that can greatly skew the numbers game in approval
checking (% for 30-ish malicious in a row).

There are also censorship or liveness issues if backing is suddenly dominated by malicious nodes but in general even if
some honest blocks get backed liveness should be preserved.

> **Note:** It is worth noting that is is fundamentally a defense in depth strategy because if we assume disputes are
> perfect it should not be a real concern. In reality disputes and determinism are difficult to get right, and
> non-determinism and happen so defense in depth is crucial when handling those subsystems.

</br></br></br>

# Risks Mitigation

## Addressing the risks of having validator disabling

One safety measure is bounding the disabled number to 1/3 ([**Point 2.**](#system-overview)) or to be exact the
byzantine threshold. If for any reason more than 1/3 of validators are getting disabled it means that some part of the
protocol failed or there is more than 1/3 malicious nodes which breaks the assumptions.

Even in such a dire situation where more than 1/3 got disabled the most likely scenario is a non-determinism bug or
sacrifice attack bug. Those attacks generally cause minor slashes to multiple honest nodes. In such a case the situation
could be salvaged by prioritizing highest offenders for disabling ([**Point 3.**](#system-overview)).

> **Note:** \
> System can be launched with re-enabling and will still provide some security improvements. Re-enabling will be
> launched in an upgrade after the initial deployment.

Fully pushing out offending validator out of the validator set it too risky in case of a dispute bug, non-determinism or
sacrifice attacks. Main issue lies in skewing the numbers in approval checking so instead of fully blocking disabled
nodes a different approach can be taken - one were only some functionalities are disabled ([**Point
5.**](#system-overview)). Once of those functionalities can be approval voting which as pointed above is so crucial that
even in a disabled state nodes should be able to participate in it ([**Point 7.**](#system-overview)).

> **Note:** \
> Approval Checking statement are implicitly valid. Sending a statement for an invalid candidate is a part of the
> dispute logic which we did not yet discuss. For now we only allow nodes to state that a candidate is valid or remain
> silent. But this solves the main risk of disabling.

Because we capped the number of disabled nodes to 1/3 there will always be at least 1/3 honest nodes to participate in
backing so liveness should be preserved. That means that backing **COULD** be safely disabled for disabled nodes
([**Point 10.**](#system-overview)).


## Addressing the risks of NOT having validator disabling

To determine if backing **SHOULD** be disabled the attack vector of 1.2 (Mass invalid candidate backing) and 2.1
(Getting lucky in approval voting) need to be considered. In both of those cases having extra backed malicious
candidates gives attackers extra chances to get lucky in approval checking. The solution is to not allow for backing in
disablement. ([**Point 10.**](#system-overview))

The attack vector 1.1 (Break sharding) requires a bit more nuance. If we assume that the attacker is a single entity and
that he can get a lot of disputes through he could potentially incredibly easily break sharding. This generally points
into the direction of disallowing that during disablement ([**Point 11.**](#system-overview)).

This might seem like an issue because it takes away the escalation privileges of disabled approval checkers but this is
NOT true. By issuing a dispute statement those nodes remain silent in approval checking because they skip their approval
statement and thus will count as a no-show. This will create a mini escalation for that particular candidate. This means
that disabled nodes maintain just enough escalation that they can protect soundness (same argument as soundness
protection during a DoS attack on approval checking) but they lose their extreme escalation privilege which are only
given to flawlessly performing nodes ([**Point 12.**](#system-overview)).

As a defense in depth measure dispute statements from disabled validators count toward confirming disputes (byzantine
threshold needed to confirm). If a dispute is confirmed everyone participates in it. This protects us from situations
where due to a bug more than byzantine threshold of validators would be disabled.

> **Note:** \
> The way this behavior is achieved easily in implementation is that honest nodes note down dispute statements from
> disabled validators just like they would for normal nodes, but they do not release their own dispute statements unless
> the dispute is confirmed already. This simply stops the escalation process of disputes.

</br></br>

# Disabling Duration

## Context

A crucial point to understand is that as of the time of writing all slashing events as alluded to in the begging are
delayed for 27 days before being executed. This is primarily because it gives governance enough time to investigate and
potentially intervene. For that duration when the slash is pending the stake is locked and cannot be moved. Time to
unbond you stake is 28 days which ensures that the stake will eventually be slashed before being withdrawn.

## Design

A few options for the duration of disablement were considered:
- 1 epoch (4h in Polkadot)
- 1 era (24h in Polkadot)
- 2-26 eras
- 27 eras

1 epoch is a short period and between a few epochs the validator will most likely be exactly the same. It is also very
difficult to fix any local node issues for honest validator in such a short time so the chance for a repeated offense is
high.

1 era gives a bit more time to fix any minor issues. Additionally, it guarantees a validator set change at so many of
the currently disabled validator might no longer be present anyway. It also gives the time for the validator to chill
themselves if they have identified a cause and want to spend more time fixing it. ([**Point 4.**](#system-overview))

Higher values could be considered and the main arguments for those are based around the fact that it reduces the number
of repeated attacks that will be allowed before the slash execution. Generally 1 attack per era for 27 eras resulting in
27 attacks at most should not compromise our safety assumptions. Although this direction could be further explored and
might be parametrized for governance to decide.

</br></br></br>

# Economic consequences of Disablement

Disablement is generally a form of punishment and that will be reflected in the rewards at the end of an era. A disabled
validator will not receive any rewards for backing or block authoring. which will reduce its profits.

That means that the opportunity cost of being disabled is a punishment by itself and thus it can be used for some cases
where a minor punishment is needed. Current implementation was using 0% slashes to mark nodes for chilling and similar
approach of 0% slashes can be used to mark validators for disablement. ([**Point 1.**](#system-overview)) 0% slashes
could for instance be used to punish approval checkers voting invalid on valid candidates.

Anything higher than 0% will of course also lead to a disablement.

> **Notes:** \
> Alternative designs incorporating disabling proportional to offenses were explored but they were deemed too complex
> and not worth the effort. Main issue with those is that proportional disabling would cause back and forth between
> disabled and enabled which complicated tracking the state of disabled validators and messes with optimistic node
> optimizations. Main benefits were that minor slashes will be barely disabled which has nice properties against
> sacrifice attacks.

</br></br></br>

# Redundancy

Some systems can be greatly simplified or outright removed thanks to the above changes. This leads to reduced complexity
around the systems that were hard to reason about and were sources of potential bugs or new attack vectors.

## Automatic Chilling

Chilling is process of a validator dropping theirs intent to validate. This removes them from the upcoming NPoS
elections and effectively pushes them out of the validator set as quickly as of the next era (or 2 era in case of late
offenses). All nominators of that validator were also getting unsubscribed from that validator. Validator could
re-register their intent to validate at any time. The intent behind this logic was to protect honest stakes from
repeated slashes caused by unnoticed bugs. It would give time for validators to fix their issue before continuing as a
validator.

Chilling had a myriad of problems. It assumes that validators and nominators remain very active and monitor everything.
If a validator got slashed he was getting automatically chilled and his nominators were getting unsubscribed. This was
an issue because of minor non-malicious slashes due to node operator mistakes or small bugs. Validators got those bugs
fixed quickly and were reimbursed but nominator had to manually re-subscribe to the validator, which they often
postponed for very lengthy amounts of time most likely due to simply not checking their stake. **This forced
unsubscribing of nominators was later disabled.**

Automatic chilling was achieving its goals in ideal scenarios (no attackers, no lazy nominators) but it opened new
vulnerabilities for attackers. The biggest issue was that chilling in case of honest node slashes could lead to honest
validators being quickly pushed out of the next validator set within the next era. This retains the validator set size
but gives an edge to attackers as they can more easily win slots in the NPoS election.

Disabling allows for punishment that limits the damages malicious actors can cause without having to resort to kicking
them out of the validator set. This protects us from the edge case of honest validators getting quickly pushed out of
the set by slashes. ([**Point 6.**](#system-overview))

> **Notes:** \
> As long as honest slashes absolutely cannot occur automatic chilling is a sensible and desirable. This means it could
> be re-enabled once PolkaVM introduces deterministic gas metering. Then best of both worlds could be achieved.

## Forcing New Era

Previous implementation of disabling had some limited mechanisms allowing for validators disablement and if too many
were disabled forcing a new era (new election). Frame staking pallet offered the ability to force a new era but it was
also deemed unsafe as it could be abused and compromised the security of the network for instance by weakening the
randomness used throughout the protocol.

</br></br></br>

# Other types of slashing

Above slashes were specifically referring to slashing events coming from disputes against candidates, but in Polkadot
other types of offenses exist for example GRANDPA equivocations or block authoring offenses. Question is if the above
defined design can handle those offenses.

## GRANDPA/BEEFY Offenses

The main offences for GRANDPA/BEEFY are equivocations. It is not a very serious offense and some nodes committing do not
endanger the system and performance is barely affected. If more than byzantine threshold of nodes equivocate it is a
catastrophic failure potentially resulting in 2 finalized blocks on the same height in the case of GRANDPA.

Honest nodes generally should not commit those offenses so the goal of protecting them does not apply here.

> **Note:** \
> A validator running multiple nodes with the same identity might equivocate. Doing that is highly not advised but it
> has happened before.

It's not a game of chance so giving attackers extra chances does not compromise soundness. Also it requires a
supermajority of honest nodes to successfully finalize blocks so any disabling of honest nodes from GRANDPA might
compromise liveness.

Best approach is to allow disabled nodes to participate in GRANDPA/BEEFY as normal and as mentioned before
GRANDPA/BABE/BEEFY equivocations should not happen to honest nodes so we can safely disable the offenders. Additionally
the slashes for singular equivocations will be very low so those offenders would easily get re-enabled in the case of
more serious offenders showing up. ([**Point 8.**](#system-overview))

## Block Authoring Offenses (BABE Equivocations)

Even if all honest nodes are disabled in Block Authoring (BA) liveness is generally preserved. At least 50% of blocks
produced should still be honest. Soundness wise disabled nodes can create a decent amount of wasted work by creating bad
blocks but they only get to do it in bounded amounts.

Disabling in BA is not a requirement as both liveness and soundness are preserved but it is the current default behavior
as well as it offers a bit less wasted work.

Offenses in BA just like in backing can be caused by faulty PVFs or bugs. They might happen to honest nodes and
disabling here while not a requirement can also ensure that this node does not repeat the offense as it might not be
trusted with it's PVF anymore.

Both points above don't present significant risks when disabling so the default behavior is to disable in BA and because
of offenses in BA. ([**Point 9.**](#system-overview)) This filters out honest faulty nodes as well as protects from some
attackers.

</br></br></br>

# Extra Design Considerations

## Disabling vs Accumulating Slashes

Instant disabling generally allows us to remove the need for accumulating slashes. It is a more immediate punishment and
it is a more lenient punishment for honest nodes.

The current architecture of using max slashing can be used and it works around the problems of delaying the slash for a
long period.

An alternative design with immediate slashing and acclimating slashing could relevant to other systems but it goes
against the governance auditing mechanisms so it's not be suitable for Polkadot.

## Disabling vs Getting Pushed Out of NPoS Elections

Validator disabling and getting forced ouf of NPoS elections (1 era) due to slashes are actually very similar processes
in terms of outcomes but there are some differences:

- **latency** (next few blocks for validator disabling and 27 days for getting pushed out organically)
- **pool restriction** (validator disabling could effectively lower the number of active validators during an era if we
  fully disable)
- **granularity** (validator disabling could remove only a portion of validator privileges instead of all)

Granularity is particularly crucial in the final design as only a few select functions are disabled while others remain.

## Enabling Approval Voter Slashes

The original Polkadot 1.0 design describes that all validators on the loosing side of the dispute are slashed. In the
current system only the backers are slashed and any approval voters on the wrong side will not be slashed. This creates
some undesirable incentives:

- Lazy approval checkers (approvals yay`ing everything)
- Spammy approval checkers (approval voters nay`ing everything)

Initially those slashes were disabled to reduce the complexity and to minimize the risk surface in case the system
malfunctioned. This is especially risky in case any nondeterministic bugs are present in the system. Once validator
re-enabling is launched approval voter slashes can be re-instated. Numbers need to be further explored but slashes
between 0-2% are reasonable. 0% would still disable which with the opportunity cost consideration should be enough.

 > **Note:** \
> Spammy approval checkers are in fact not a big issue as a side effect of the offchain-disabling introduced by the
> Defense Against Past-Era Dispute Spam (**Node**) [#2225](https://github.com/paritytech/polkadot-sdk/issues/2225). It
> makes it so all validators loosing a dispute are locally disabled and ignored for dispute initiation so it effectively
> silences spammers. They can still no-show but the damage is minimized.


## Interaction with all types of misbehaviors

With re-enabling in place and potentially approval voter slashes enabled the overall misbehaviour-punishment system can
be as highlighted in the table below:

|Misbehaviour                         |Slash %   |Onchain Disabling  |Offchain Disabling |Chilling |Reputation Costs  |
|------------                         |-------   |-----------------  |------------------ |-------- |----------------- |
|Backing Invalid                      |100%      |Yes (High Prio)    |Yes   (High Prio)  |No       |No                |
|ForInvalid Vote                      |2%        |Yes (Mid Prio)     |Yes   (Mid Prio)   |No       |No                |
|AgainstValid Vote                    |0%        |Yes (Low Prio)     |Yes   (Low Prio)   |No       |No                |
|GRANDPA / BABE / BEEFY Equivocations |0.01-100% |Yes (Varying Prio) |No                 |No       |No                |
|Seconded + Valid Equivocation        |-         |No                 |No                 |No       |No                |
|Double Seconded Equivocation         |-         |No                 |No                 |No       |Yes               |


*Ignoring AURA offences.

**There are some other misbehaviour types handled in rep only (DoS prevention etc) but they are not relevant to this strategy.

*** BEEFY will soon introduce new slash types so this strategy table will need to be revised but no major changes are expected.

</br></br></br>

# Implementation

Implementation of the above design covers a few additional areas that allow for node-side optimizations.

## Core Features

1. Disabled Validators Tracking (**Runtime**) [#2950](https://github.com/paritytech/polkadot-sdk/issues/2950)
    - Expose a ``disabled_validators`` map through a Runtime API
1. Enforce Backing Disabling (**Runtime**) [#1592](https://github.com/paritytech/polkadot-sdk/issues/1592)
    - Filter out votes from ``disabled_validators`` in ``BackedCandidates`` in ``process_inherent_data``
1. Substrate Byzantine Threshold (BZT) as Limit for Disabling
   [#1963](https://github.com/paritytech/polkadot-sdk/issues/1963)
    - Can be parametrized but default to BZT
    - Disable only up to 1/3 of validators
1. Respect Disabling in Backing Statement Distribution (**Node**)
   [#1591](https://github.com/paritytech/polkadot-sdk/issues/1951)
    - This is an optimization as in the end it would get filtered in the runtime anyway
    - Filter out backing statements coming from ``disabled_validators``
1. Respect Disablement in Backing (**Node**) [#2951](https://github.com/paritytech/polkadot-sdk/issues/2951)
    - This is an optimization as in the end it would get filtered in the runtime anyway
    - Don't start backing new candidates when disabled
    - Don't react to backing requests when disabled
1. Stop Automatic Chilling of Offenders [#1962](https://github.com/paritytech/polkadot-sdk/issues/1962)
    - Chilling still persists as a state but is no longer automatically applied on offenses
1. Respect Disabling in Dispute Participation (**Node**) [#2225](https://github.com/paritytech/polkadot-sdk/issues/2225)
    - Receive dispute statements from ``disabled_validators`` but do not release own statements
    - Ensure dispute confirmation when BZT statements from disabled
1. Remove Liveness Slashes [#1964](https://github.com/paritytech/polkadot-sdk/issues/1964)
    - Remove liveness slashes from the system
    - The are other incentives to be online and they could be abused to attack the system
1. Defense Against Past-Era Dispute Spam (**Node**) [#2225](https://github.com/paritytech/polkadot-sdk/issues/2225)
    - This is needed because runtime cannot disable validators which it no longer knows about
    - Add a node-side parallel store of ``disabled_validators``
    - Add new disabled validators to node-side store when they loose a dispute in any leaf in scope
    - Runtime ``disabled_validators`` always have priority over node-side ``disabled_validators``
    - Respect the BZT threshold
    > **Note:** \
    > An alternative design here was considered where instead of tracking new incoming leaves a relay parent is used.
    > This would guarantee determinism as different nodes can see different leaves, but this approach was leaving too
    > wide of a window because of Async-Backing. Relay Parent could have been significantly in the past and it would
    > give a lot of time for past session disputes to be spammed.
1. Do not block finality for "disabled" disputes [#3358](https://github.com/paritytech/polkadot-sdk/pull/3358)
    - Emergency fix to not block finality for disputes initiated only by disabled validators
1. Re-enable small offender when approaching BZT (**Runtime**) #TODO
    - When BZT limit is reached and there are more offenders to be disabled re-enable the smallest offenders to disable
      the biggest ones
