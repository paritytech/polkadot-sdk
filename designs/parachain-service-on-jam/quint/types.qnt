// Foundational types and protocol constants for the Parachain Service.
//
// Cryptographic primitives (hashes, keys, signatures) are modeled as opaque
// integers — we only care about equality, not their byte layout. Where the
// gray paper or design doc imposes a size constraint that matters for
// state-balance accounting, the constant is named below.
module types {

  // --- Opaque cryptographic / identifier types -----------------------------
  //
  // The three hash families (`Hash`, `ValidationCodeHash`, `AuthorizerHash`)
  // and the two identifier families (`ParaId`, `ServiceId`) are nominally
  // distinct: each is wrapped so the typechecker rejects accidental
  // cross-type comparisons. Hashes use record wrappers with distinct field
  // names; IDs use 1-constructor variants. All other domain integers stay
  // as `int` because nothing in the spec is at risk of confusing them.

  type Hash = { hashBytes: int }
  type ValidationCodeHash = { vchBytes: int }
  type AuthorizerHash = { authBytes: int }

  /// Opaque 336-octet validator-key blob; see design §5.3.
  type ValidatorKey = int

  type ParaId = MkParaId(int)
  type ServiceId = MkServiceId(int)

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

  // --- Protocol constants (gray paper / design doc) -----------------------

  /// Each validator key blob is 336 octets.
  pure val ValidatorKeySize: int = 336

  /// Valid `stagingset` lengths (gray paper Safrole, eq. valcount):
  /// `valcount = { 3c : c ∈ N_{2 .. C_corecount+1} } = {6, 9, …, 1023}`.
  /// The gray paper's `N_{a .. b}` is the half-open interval `[a, b)`, so the
  /// max is `3 * C_corecount = 1023`. Quint's `.to` is inclusive, hence
  /// `2.to(CoreCount)` (NOT `CoreCount + 1`) to model `c ∈ [2, 342)`.
  pure val CoreCount: int = 341
  pure val ValCount: Set[int] =
  2.to(CoreCount).map(c => 3 * c)

  /// Combined cap on a work-report's auth-trace + result blobs.
  pure val MaxReportVarSize: int = 48 * 1024

  /// Per-call cap on `set_validator_keys` chunks (design §4.3).
  pure val SetValidatorKeysMaxKeys: int = 30

  /// Worst-case staging footprint when an Asset Hub rotation is in flight.
  pure val MaxStagedValidatorKeysFootprint: int =
  CoreCount * 3 * ValidatorKeySize // 1023 * 336 = 343 728 B

  /// §5.2 — pending-upgrade deadline relative to current timeslot (24 h at 6 s).
  pure val UpgradeTimeoutTimeslots: int = 24 * 3600 / 6

  /// Per-parachain log capacity (design §3.1).
  pure val ParachainLogCapacity: int = 8

  /// §3.1: at most 30 AccumulateLog entries per timeslot per para.
  pure val AccumulateLogCapacity: int = 30

  /// §3.1: report_error opaque payload cap.
  pure val ReportErrorPayloadCap: int = 1024

  /// §3.1: 80-slot authorizer queue length.
  pure val AuthorizerQueueLength: int = 80

  /// §3.1: bounded cap on `staged_validator_keys` (= `CoreCount * 3 = 1023`).
  pure val MaxStagedValidatorKeys: int = CoreCount * 3

  /// §3.1: bounded cap on `incoming_transfers`.
  pure val IncomingTransfersMaxLen: int = 1000

  // --- Polymorphic Option ------------------------------------------------

  type Option[a] = Some(a) | None

  // --- Helpers -----------------------------------------------------------

  /// Quint has no built-in map-remove; rebuild without the key.
  pure def mapRemove(m: a -> b, k: a): a -> b =
  m.keys().exclude(Set(k)).mapBy(x => m.get(x))

  // --- Abstract hash function ---------------------------------------------
  //
  // We model `hash(head_data)` as an injective function — we only need
  // equality semantics for the parent-head check (§5.1, step 1).

  pure def headHash(h: HeadData): Hash = { hashBytes: h }
}
