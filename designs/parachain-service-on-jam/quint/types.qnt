// Foundational types and protocol constants for the Parachain Service.
//
// Cryptographic primitives (hashes, keys, signatures) are modeled as opaque
// integers; we only care about equality, not their byte layout. Where the
// gray paper or design doc imposes a size constraint that matters for
// state-balance accounting, the constant is named below.
module types {

  // --- Opaque cryptographic / identifier types -----------------------------
  //
  // The hash families and the two identifier families (`ParaId`, `ServiceId`)
  // are nominally distinct: each is wrapped so the typechecker rejects
  // accidental cross-type comparisons. Hashes use record wrappers with
  // distinct field names; IDs use 1-constructor variants. All other domain
  // integers stay as `int` because nothing in the spec is at risk of
  // confusing them.

  /// One hash type per hashing domain. Distinct domains must not share
  /// digests, so each domain gets its own wrapper rather than a runtime domain
  /// tag: comparing a head hash against a preimage hash is a type error the
  /// typechecker catches, not a discipline new code has to remember. An ITF
  /// trace still names the domain, since the field name is the domain.
  ///
  /// `Hash` keys `preimage_registry` (§6.1), `HeadHash` is `hash(head_data)`
  /// (§5.1), `KvKeyHash` is the `blake2_256` key hash a rejected `kv_set` logs
  /// (§6.1), and `MerkleHash` is a head-commitment tree element (§5.5).
  type Hash = { hashBytes: int }
  type HeadHash = { headBytes: int }
  type KvKeyHash = { kvKeyBytes: int }
  type MerkleHash = { merkleBytes: int }
  type ValidationCodeHash = { vchBytes: int }
  type AuthorizerHash = { authBytes: int }

  /// Opaque 336-byte validator-key blob; see design §5.3.
  type ValidatorKey = int

  // Sentinels exist only to force closure of the row-polymorphic variant
  // type so Apalache's QuintIR converter accepts these in `quint verify`
  // (Quint's typechecker accepts single-constructor variants as open).
  // The sentinel constructors are never constructed by the model.
  type ParaId = MkParaId(int) | _ParaIdSentinel
  type ServiceId = MkServiceId(int) | _ServiceIdSentinel

  /// The two privileged chains, modeled as fixed parachain IDs. The
  /// Coretime chain owns parachain management (§6); Asset Hub owns
  /// validator-key updates and service self-upgrade (§5.3, §5.4). Defined
  /// here so the whole model refers to the same IDs instead of scattering
  /// `MkParaId(1)` / `MkParaId(2)` literals.
  pure val CoretimeParaId: ParaId = MkParaId(1)
  pure val AssetHubParaId: ParaId = MkParaId(2)

  /// The Parachain Service's own JAM service id. JAM's `assign` always writes an
  /// assigner, so applying a core's queue re-asserts this id as the assigner.
  pure val ParachainServiceId: ServiceId = MkServiceId(1)

  type CoreIndex = int
  type Timeslot = int
  type Balance = int

  /// Head data is opaque to the service; the design caps it at 4 KiB but
  /// we only model equality and a derived hash.
  type HeadData = int
  type Memo = int

  // --- Hash conversion helpers -------------------------------------------
  //
  // The cross-parachain `preimage_registry` keys preimages by `Hash`, so a
  // validation-code hash needs an explicit conversion to the registry's key
  // type whenever it's stored or looked up.

  pure def vchAsHash(vch: ValidationCodeHash): Hash = { hashBytes: vch.vchBytes }

  /// The wrapped integer, for the places the spec orders by `para_id`, the
  /// head-commitment leaves (§5.5). The sentinel is never constructed.
  pure def paraIdInt(p: ParaId): int =
  match p {
    | MkParaId(n) => n
    | _ParaIdSentinel => 0
  }

  // --- Protocol constants (gray paper / design doc) -----------------------

  /// Each validator key blob is 336 bytes.
  pure val ValidatorKeySize: int = 336

  pure val CoreCount: int = 341
  /// Valid `stagingset` lengths (gray paper Safrole, eq. valcount).
  /// Quint's `.to` is inclusive, so the upper bound is `CoreCount` not
  /// `CoreCount + 1`; this `2.to(CoreCount)` matches the gray paper's
  /// half-open `N_{2 .. C_corecount+1}`.
  pure val ValCount: Set[int] =
  2.to(CoreCount).map(c => 3 * c)

  /// Combined cap on a work-report's auth-trace + result blobs.
  pure val MaxReportVarSize: int = 48 * 1024

  /// Per-call cap on `set_validator_keys` chunks (design §4.3).
  pure val SetValidatorKeysMaxKeys: int = 30

  /// Max upward messages a PVF may emit in one Refine invocation (design §4.3).
  pure val MaxUpwardMessages: int = 1024

  /// §5.2: pending-upgrade deadline relative to current timeslot (24 h at 6 s).
  pure val UpgradeTimeoutTimeslots: int = 24 * 3600 / 6

  /// Per-para log byte budget (exact encoded size of all entries must stay ≤ this).
  pure val ParachainLogByteCap: int = 64 * 1024

  /// Max age (in timeslots) of a work-package's lookup-anchor; gray paper
  /// `L` (~24h). The lookup-anchor may sit this far before the accumulating
  /// block; distinct from the recent-history *anchor* window (8 blocks).
  pure val MaxLookupAge: int = 24 * 3600 / 6

  /// Gray paper `C_expungeperiod = C_maxlookupanchorage + 4800 = 19 200`
  /// (~32 h). A preimage forgotten at timeslot `y` may only be expunged by a
  /// second `forget` once `y < now - ExpungePeriod`. See §6.1.
  pure val ExpungePeriod: int = MaxLookupAge + 4800

  /// §3.1: 80-slot authorizer queue length.
  pure val AuthorizerQueueLength: int = 80

  /// §3.1: bounded cap on `staged_validator_keys` (= `CoreCount * 3 = 1023`).
  pure val MaxStagedValidatorKeys: int = CoreCount * 3

  /// §3.1 cap on `incoming_transfers`. PROVISIONAL: the real bound must be
  /// derived from the benchmarked cost of processing one transfer, so that a
  /// single work-report's accumulation gas (`C_reportaccgas`) cannot deliver
  /// more transfers than the queue can hold (§5.1). Asset Hub's baseline
  /// footprint is computed from this constant, so changing it moves that too.
  pure val MaxIncomingTransfers: int = 1000

  /// Value octets of one `incoming_transfers` entry:
  /// `ServiceId 4 + Amount (Compact<u64> worst case 9) + Memo 128`.
  pure val IncomingTransferValueOctets: int = 4 + 9 + 128

  /// Octets of one worst-case `incoming_transfers` bucket: a bucket holding a
  /// single transfer (maximal fragmentation, §6.1): JAM's per-entry overhead (34),
  /// the key (1 B tag + 4 B `Timeslot`), the `Vec` length prefix (1), the transfer
  /// itself, and `Some(next_slot)` (5).
  pure val IncomingTransferBucketOctets: int =
    34 + 5 + 1 + IncomingTransferValueOctets + 5

  /// Full balance-unit cost of one worst-case bucket (§6.1): its octets plus the
  /// one storage item it occupies.
  pure val IncomingTransferEntryFootprint: int =
    10 + IncomingTransferBucketOctets


  // --- Polymorphic Option ------------------------------------------------

  type Option[a] = Some(a) | None

  // --- Helpers -----------------------------------------------------------

  /// Quint has no built-in map-remove; rebuild without the key.
  pure def mapRemove(m: a -> b, k: a): a -> b =
  m.keys().exclude(Set(k)).mapBy(x => m.get(x))

  /// Rebuild a map keeping only the keys that satisfy `keep`.
  pure def mapFilterKeys(m: a -> b, keep: a => bool): a -> b =
  m.keys().filter(keep).mapBy(x => m.get(x))

  /// Append `opt`'s element to `l` when present; leave `l` unchanged on `None`.
  pure def appendOption(l: List[a], opt: Option[a]): List[a] =
  match opt {
    | Some(e) => l.append(e)
    | None => l
  }

  /// `Option.map`: apply `f` to the wrapped value, or propagate `None`.
  pure def optionMap(opt: Option[a], f: a => b): Option[b] =
  match opt {
    | Some(e) => Some(f(e))
    | None => None
  }

  /// `Option`'s value or a default when `None`.
  pure def optionGetOr(opt: Option[a], fallback: a): a =
  match opt {
    | Some(e) => e
    | None => fallback
  }

  /// `Some(e)` -> singleton `{e}`; `None` -> empty set.
  pure def optionToSet(opt: Option[a]): Set[a] =
  match opt {
    | Some(e) => Set(e)
    | None => Set()
  }

  // --- Abstract hash function ---------------------------------------------
  //
  // We model `hash(head_data)` as an injective function; we only need
  // equality semantics for the parent-head check (§5.1, step 1).

  pure def headHash(h: HeadData): HeadHash = { headBytes: h }

  /// Abstract hash of a byte list, modelling blake2-256 for KV key hashing.
  /// Injective within the model's small key space.
  pure def listHash(key: List[int]): KvKeyHash =
    { kvKeyBytes: key.foldl(0, (acc, b) => acc * 257 + b) }
}
