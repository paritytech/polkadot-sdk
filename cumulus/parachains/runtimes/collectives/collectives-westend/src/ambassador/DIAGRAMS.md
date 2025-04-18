# Polkadot Ambassador Fellowship UML Diagrams

## System Architecture Diagram

```mermaid
graph TB
    subgraph "Polkadot Ambassador Fellowship System"
        GOV[OpenGov System]
        RANK[Ranked Collective Pallet]
        REG[Ambassador Registration Pallet]
        FUND[Optimistic Funding Pallet]
        TREAS[Treasury Pallet]
        SALARY[Salary Pallet]
        CONTENT[Collective Content Pallet]
        CORE[Core Fellowship Pallet]

        GOV --> |Controls| RANK
        GOV --> |Funds| TREAS
        TREAS --> |Allocates| FUND
        RANK --> |Determines| SALARY
        REG --> |Onboards to| RANK
        CORE --> |Tracks Activity for| RANK
        CONTENT --> |Manages Charter for| RANK
        FUND --> |Distributes to| SALARY
    end

    subgraph "External Systems"
        DOT[DOT Token Holders]
        COMMUNITY[Community Members]
    end

    DOT --> |Vote through| GOV
    COMMUNITY --> |Register via| REG
```

## Rank System Class Diagram

```mermaid
classDiagram
    class RankedCollective {
        +addMember(who: AccountId, rank: u16)
        +removeMember(who: AccountId)
        +promote(who: AccountId, to_rank: u16)
        +demote(who: AccountId, to_rank: u16)
        +isMember(who: AccountId): bool
        +memberRank(who: AccountId): Option~u16~
        +rankMembers(rank: u16): Vec~AccountId~
    }

    class AmbassadorRegistration {
        +lock_dot(who: AccountId)
        +verify_introduction(who: AccountId, verifier: AccountId)
        +registrationStatus(who: AccountId): RegistrationStatus
        +completeRegistration(who: AccountId)
    }

    class CoreFellowship {
        +trackActivity(who: AccountId, activity: Activity)
        +calculateRankScore(who: AccountId): Score
        +checkPromotionEligibility(who: AccountId, target_rank: u16): bool
        +trackVotingAttendance(who: AccountId, voted: bool)
    }

    class Salary {
        +registerPayee(who: AccountId, payee: AccountId)
        +calculateSalary(rank: u16): Balance
        +paySalary(who: AccountId)
    }

    class OptimisticFunding {
        +createOpBlock(value: Balance)
        +requestFunding(who: AccountId, amount: Balance, memo: Vec~u8~)
        +voteOnFunding(voter: AccountId, target: AccountId, approve: bool)
        +calculateVotingPower(who: AccountId): u32
        +distributeFunds()
    }

    class CollectiveContent {
        +setCharter(content: Vec~u8~)
        +setAnnouncement(content: Vec~u8~)
        +getCharter(): Vec~u8~
        +getAnnouncement(): Vec~u8~
    }

    RankedCollective <|-- AmbassadorRegistration : onboards to
    RankedCollective <|-- CoreFellowship : tracks for
    RankedCollective <|-- Salary : pays based on
    RankedCollective <|-- OptimisticFunding : voting power from
    RankedCollective <|-- CollectiveContent : manages for
```

## Onboarding Sequence Diagram

```mermaid
sequenceDiagram
    participant User as New User
    participant Reg as Ambassador Registration Pallet
    participant Rank as Ranked Collective Pallet
    participant Verify as Verifier (Existing Ambassador)

    User->>Reg: lock_dot()
    Reg->>Reg: Check identity verification
    Reg->>User: Set RegistrationStatus::PendingIntroduction

    User->>Verify: Introduce in designated channel
    Verify->>Reg: verify_introduction(user, verifier)

    alt Self-onboarding path
        Reg->>Reg: Check if identity verified & DOT locked
        Reg->>Reg: Check if introduction verified
        Reg->>Reg: Set RegistrationStatus::Complete
    else Member-sponsored path
        Verify->>Reg: sponsor_member(user)
        Reg->>Reg: Check if introduction verified
        Reg->>Reg: Set RegistrationStatus::Complete
    end

    Reg->>Rank: addMember(user, 0)
    Rank->>User: Assign Rank 0 (Advocate Ambassador)
```

## Promotion Flow Diagram

```mermaid
graph TD
    START[Ambassador Requests Promotion] --> CHECK{Check Eligibility}
    CHECK --> |Not Eligible| REJECT[Reject Request]
    CHECK --> |Eligible| VOTING[Start 28-day Voting Period]

    VOTING --> TALLY{Tally Rank-Weighted Votes}

    TALLY --> |Majority Approve| PROMOTE[Promote to Next Rank]
    TALLY --> |Majority Reject| REJECT

    subgraph "Promotion Requirements"
        REQ_SAME[Same Tier Promotion:<br>- Online Engagement Level<br>- Offline Engagement Level]
        REQ_TIER[Cross Tier Promotion:<br>- Higher Online Engagement<br>- Higher Offline Engagement<br>- Governance Participation<br>- Community Growth]
    end

    subgraph "Rank Structure"
        TIER1[Tier 1 - Learners:<br>Rank I: Associate Ambassador<br>Rank II: Lead Ambassador]
        TIER2[Tier 2 - Engagers:<br>Rank III: Senior Ambassador<br>Rank IV: Principal Ambassador]
        TIER3[Tier 3 - Drivers:<br>Rank V: Global Ambassador<br>Rank VI: Global Head Ambassador]
    end

    PROMOTE --> |To Highest Rank| REFERENDUM[Public Referendum Required]
    PROMOTE --> |To Other Ranks| UPDATE[Update Rank in System]

    REFERENDUM --> |Approved| UPDATE
    REFERENDUM --> |Rejected| REJECT

    UPDATE --> END[Promotion Complete]
```

## Voting Weight System

```mermaid
graph LR
    subgraph "Voting Weight by Rank"
        R0[Rank 0: No Vote]
        R1[Rank I: Weight 1]
        R2[Rank II: Weight 3]
        R3[Rank III: Weight 6]
        R4[Rank IV: Weight 10]
        R5[Rank V: Weight 15]
        R6[Rank VI: Weight 21]
    end

    subgraph "Voting Attendance Requirements"
        A1[Rank I: >30%]
        A2[Rank II: >30%]
        A3[Rank III: >45%]
        A4[Rank IV: >60%]
        A5[Rank V: >75%]
        A6[Rank VI: >90%]
    end

    R1 --- A1
    R2 --- A2
    R3 --- A3
    R4 --- A4
    R5 --- A5
    R6 --- A6
```

## Optimistic Funding Sequence Diagram

```mermaid
sequenceDiagram
    participant DOT as DOT Token Holders
    participant Gov as OpenGov
    participant Treasury as Treasury Pallet
    participant OpFund as Optimistic Funding Pallet
    participant Ambassador as Ambassador

    DOT->>Gov: Vote on monthly OpBlock allocation
    Gov->>Treasury: Request funds for OpBlocks
    Treasury->>OpFund: Transfer funds (OpBlocks × $10,000)

    Ambassador->>OpFund: Request funding with memo
    OpFund->>OpFund: Verify ambassador rank (I-VI)

    DOT->>OpFund: Vote on funding requests using Phragmén voting

    loop Each pro-rata period
        OpFund->>OpFund: Tally votes with rank weights
        OpFund->>OpFund: Calculate fund allocation
        OpFund->>Ambassador: Distribute approved funds
    end

    Ambassador->>OpFund: Submit spending report
    DOT->>OpFund: Update nominations based on spending reports
```

## Fellowship Vertical Structure

```mermaid
graph TB
    subgraph "Ambassador Fellowship Structure"
        MAIN[Ambassador Fellowship]

        INTERNAL[Internal Verticals]
        EXTERNAL[External Verticals]

        MAIN --> INTERNAL
        MAIN --> EXTERNAL

        INTERNAL --> DEV[Ambassador Development & Recognition]
        INTERNAL --> IMPACT[Programme Impact & Success]
        INTERNAL --> RECRUIT[Recruitment & Retention]

        EXTERNAL --> BIZ[Business Development]
        EXTERNAL --> INV[Investor Relations]
        EXTERNAL --> EDU[Education & Awareness]
        EXTERNAL --> DEVREL[Developer Recruitment]
        EXTERNAL --> GOV[Decentralization & Governance]
    end

    subgraph "Governance Structure"
        FELLOWSHIP[Fellowship Internal Voting]
        OPENGOV[OpenGov Root Track]

        FELLOWSHIP --> |Minor Changes| MAIN
        OPENGOV --> |Major Changes| MAIN
    end
```

## Decision Making System

```mermaid
flowchart TD
    START[Issue Requires Decision] --> CONSENSUS{Social Consensus Possible?}

    CONSENSUS -->|Yes| IMPLEMENT[Implement Decision]
    CONSENSUS -->|No| VOTE[Initiate On-Chain Vote]

    VOTE --> CATEGORY{Decision Category}

    CATEGORY -->|Promotion/Demotion| RANK_VOTE[Rank-Weighted Voting]
    CATEGORY -->|Treasury Spending| TREASURY_VOTE[Fellowship Treasury Vote]
    CATEGORY -->|Minor Manifesto Changes| INTERNAL_VOTE[Fellowship Internal Vote]
    CATEGORY -->|Major Changes| OPENGOV[OpenGov Root Referendum]

    RANK_VOTE --> TALLY[Tally Votes with Rank Weights]
    TREASURY_VOTE --> TALLY
    INTERNAL_VOTE --> TALLY

    TALLY --> MAJORITY{Majority Achieved?}

    MAJORITY -->|Yes| IMPLEMENT
    MAJORITY -->|No| REJECT[Reject Proposal]

    OPENGOV --> DOT[DOT Holder Vote]
    DOT --> IMPLEMENT
```
