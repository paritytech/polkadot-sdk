// psm_extended.qnt
//
// Faithful PSM model combining everything probed in isolation so far:
//   - multi-asset (3 asset slots, one each: same-decimal, scale-up, scale-down)
//   - multi-decimal: per-asset (factor, scaleDown) modeling USDX/USDT/DAI cases
//   - mul_ceil fees, distinct mint vs redeem rates per asset
//   - round-trip dust on redeem (truncation stays with user)
//   - asset lifecycle (add / remove with PsmDebt-zero guard)
//   - donations (direct external transfer to PSM account, bypassing mint)
//   - manager privilege levels (Full vs Emergency) gating governance
//   - MinSwapAmount / ExceedsMaxIssuance boundaries
//   - multiple users (Alice and Bob) so account-level state is exercised
//
// All amounts are in internal units for PsmDebt and per-asset state that
// is internal-denominated; external-denominated state uses external units
// for that asset.

module psm_extended {

  // ---------- Types ----------

  type Asset = str
  type UserId = str

  type Status =
    | NotApproved        // asset is not in ExternalAssets
    | AllEnabled
    | MintingDisabled
    | AllDisabled

  type Level = Full | Emergency

  // ---------- Constants ----------

  pure val assetSlots: Set[Asset] = Set("A", "B", "C")
  pure val users: Set[UserId] = Set("alice", "bob")

  // Per-asset decimal mapping:
  //   factor[a]: 10^|ext_decimals - internal_decimals|, always >= 1
  //   scaleDown[a]: true if external decimals < internal (USDX-style),
  //                 false if external decimals >= internal (USDT, DAI).
  //
  // The pallet's external_to_internal in the `Less` branch (ext < internal)
  // MULTIPLIES by 10^diff (no truncation possible). In the `Greater` branch
  // (ext > internal) it FLOORS by 10^diff.
  //
  // We model:
  //   A: scaleDown=false, factor=1   (same decimals, e.g., USDT)
  //   B: scaleDown=false, factor=10  (scale-up: ext > internal, e.g., DAI-like)
  //   C: scaleDown=true,  factor=10  (scale-down: ext < internal, USDX-style)
  pure val factor: Asset -> int =
    Map("A" -> 1, "B" -> 10, "C" -> 10)

  pure val scaleDown: Asset -> bool =
    Map("A" -> false, "B" -> false, "C" -> true)

  pure val maximumIssuance: int = 100000
  pure val minSwapAmount: int = 1
  pure val maxExternalAssets: int = 3

  // ---------- Storage state ----------

  var status: Asset -> Status        // ExternalAssets[a]
  var debt: Asset -> int             // PsmDebt[a], in internal units
  var weight: Asset -> int           // AssetCeilingWeight[a], permill 0..1000
  var mintingFeeRate: Asset -> int   // MintingFee[a], permill 0..1000
  var redeemFeeRate: Asset -> int    // RedemptionFee[a], permill 0..1000
  var reserveExt: Asset -> int       // external balance held by PSM, in external units
  var maxPsmRatio: int               // MaxPsmDebtOfTotal, permill 0..1000

  // Per-user state:
  var userInternal: UserId -> int            // internal balance per user
  var userExt: UserId -> (Asset -> int)      // external balance per user per asset
  var feeDestInternal: int                   // FeeDestination's internal balance
  var totalIssuance: int                     // T::InternalAsset::total_issuance()

  // ---------- Decimal conversion (matching lib.rs::external_to_internal etc.) ----------

  // external_to_internal: convert external units to internal units.
  // - scaleDown=false (ext >= internal): floor-divide by factor
  // - scaleDown=true  (ext  < internal): multiply by factor (no truncation)
  pure def extToInt(amount: int, a: Asset): int =
    if (scaleDown.get(a)) amount * factor.get(a)
    else amount / factor.get(a)

  // internal_to_external: inverse direction.
  // - scaleDown=false (ext >= internal): multiply by factor
  // - scaleDown=true  (ext  < internal): floor-divide by factor (truncation possible)
  pure def intToExt(amount: int, a: Asset): int =
    if (scaleDown.get(a)) amount / factor.get(a)
    else amount * factor.get(a)

  // ---------- Helpers (pure) ----------

  pure def mulCeil(rate: int, x: int): int =
    if (x == 0) 0
    else (rate * x + 999) / 1000

  pure def isApproved(s: Status): bool =
    s != NotApproved

  pure def allowsMinting(s: Status): bool =
    s == AllEnabled

  pure def allowsRedemption(s: Status): bool =
    s != AllDisabled and s != NotApproved

  pure def maxPsmDebt(r: int): int =
    r * maximumIssuance / 1000

  pure def totalDebt(d: Asset -> int): int =
    assetSlots.fold(0, (acc, a) => acc + d.get(a))

  pure def totalUserInternal(u: UserId -> int): int =
    users.fold(0, (acc, k) => acc + u.get(k))

  pure def totalWeightSum(w: Asset -> int): int =
    assetSlots.fold(0, (acc, a) => acc + w.get(a))

  pure def maxAssetDebt(a: Asset, w: Asset -> int, r: int): int =
    if (w.get(a) == 0) 0
    else
      val total = totalWeightSum(w)
      if (total == 0) 0
      else w.get(a) * maxPsmDebt(r) / total

  // ---------- Hard invariants (mirroring deployed try_state) ----------

  // Check 8: reserve >= debt expressed in external units, per approved asset.
  val check8 =
    assetSlots.forall(a =>
      not(isApproved(status.get(a))) or reserveExt.get(a) >= intToExt(debt.get(a), a)
    )

  // The pallet had a check that compared the sum of per-asset PsmDebt with
  // total_psm_debt(). The multi-instance redesign turned total_psm_debt()
  // into a fold over the same rows, so the comparison holds by construction.
  // The pallet and this model both dropped that check.

  // Check 6: total internal issuance >= total PSM debt.
  val check6 = totalIssuance >= totalDebt(debt)

  // Check 14: no non-zero PsmDebt for a non-approved asset.
  val check14 =
    assetSlots.forall(a =>
      isApproved(status.get(a)) or debt.get(a) == 0
    )

  // Conservation: total internal in existence = sum(user balances) + feeDest.
  // (Only PSM mints/burns internal in this model.)
  val conservation =
    totalIssuance == totalUserInternal(userInternal) + feeDestInternal

  val hardInvariant =
    check8 and check6 and check14 and conservation

  // ---------- Stronger probes (not deployed; some legitimately fail) ----------

  // (a) Strict issuance equality. Should hold because every internal mint
  //     corresponds 1:1 to a PsmDebt increase, and every burn to a decrease.
  val strictIssuanceEqualsDebt = totalIssuance == totalDebt(debt)

  // (b) Strict reserve equality (no donations, no truncation slack).
  //     Donations break this; so does scale-down redeem truncation in some
  //     traces (dust stays with the user, but the pallet's accounting keeps
  //     reserve and debt-as-external aligned because `effective_internal_net`
  //     is what gets subtracted from PsmDebt). Probing it tells us whether
  //     any path leaves stale external residue in the reserve unaccounted.
  val reserveEqualsDebt =
    assetSlots.forall(a =>
      not(isApproved(status.get(a))) or reserveExt.get(a) == intToExt(debt.get(a), a)
    )

  // (c) Orphaned reserves: after remove_external_asset, reserveExt[a] may be
  //     non-zero (donations). The pallet has no try_state check for this.
  val noOrphanedReserves =
    assetSlots.forall(a => isApproved(status.get(a)) or reserveExt.get(a) == 0)

  // (d) Cross-user safety: no user can end up with more internal than the
  //     mint operations attributed to them. This is harder to express
  //     without per-user mint/burn ledgers, so we use a weaker form: every
  //     user's internal balance is bounded by total issuance.
  val perUserBounded =
    users.forall(u => userInternal.get(u) <= totalIssuance)

  // ---------- Initialization ----------

  action init = all {
    status'         = assetSlots.mapBy(_ => NotApproved),
    debt'           = assetSlots.mapBy(_ => 0),
    weight'         = assetSlots.mapBy(_ => 0),
    mintingFeeRate' = assetSlots.mapBy(_ => 5),
    redeemFeeRate'  = assetSlots.mapBy(_ => 5),
    reserveExt'     = assetSlots.mapBy(_ => 0),
    maxPsmRatio'    = 1000,
    userInternal'   = users.mapBy(_ => 0),
    feeDestInternal'= 0,
    totalIssuance'  = 0,
    userExt'        = users.mapBy(_ => assetSlots.mapBy(_ => 10000)),
  }

  // ---------- Actions: swaps ----------

  action mint(u: UserId, a: Asset, externalAmount: int): bool = all {
    externalAmount > 0,
    allowsMinting(status.get(a)),
    externalAmount <= userExt.get(u).get(a),
    val internalEquivalent = extToInt(externalAmount, a)
    val effectiveExternal = intToExt(internalEquivalent, a)
    val fee = mulCeil(mintingFeeRate.get(a), internalEquivalent)
    val toUser = internalEquivalent - fee
    all {
      internalEquivalent > 0,
      internalEquivalent >= minSwapAmount,
      totalIssuance + internalEquivalent <= maximumIssuance,
      totalDebt(debt) + internalEquivalent <= maxPsmDebt(maxPsmRatio),
      debt.get(a) + internalEquivalent <= maxAssetDebt(a, weight, maxPsmRatio),
      debt'            = debt.set(a, debt.get(a) + internalEquivalent),
      reserveExt'      = reserveExt.set(a, reserveExt.get(a) + effectiveExternal),
      userExt'         = userExt.set(u, userExt.get(u).set(a, userExt.get(u).get(a) - effectiveExternal)),
      userInternal'    = userInternal.set(u, userInternal.get(u) + toUser),
      feeDestInternal' = feeDestInternal + fee,
      totalIssuance'   = totalIssuance + internalEquivalent,
      status'          = status,
      weight'          = weight,
      mintingFeeRate'  = mintingFeeRate,
      redeemFeeRate'   = redeemFeeRate,
      maxPsmRatio'     = maxPsmRatio,
    }
  }

  action redeem(u: UserId, a: Asset, amount: int): bool = all {
    amount > 0,
    allowsRedemption(status.get(a)),
    amount <= userInternal.get(u),
    amount >= minSwapAmount,
    val fee = mulCeil(redeemFeeRate.get(a), amount)
    val internalNet = amount - fee
    val externalOut = intToExt(internalNet, a)
    val effectiveInternalNet = extToInt(externalOut, a)
    all {
      // AmountTooSmallAfterConversion: reject only when truncation wipes a non-zero net.
      internalNet == 0 or externalOut > 0,
      effectiveInternalNet <= debt.get(a),
      externalOut <= reserveExt.get(a),
      debt'            = debt.set(a, debt.get(a) - effectiveInternalNet),
      reserveExt'      = reserveExt.set(a, reserveExt.get(a) - externalOut),
      userExt'         = userExt.set(u, userExt.get(u).set(a, userExt.get(u).get(a) + externalOut)),
      // User pays fee (transfer) + effectiveInternalNet (burn). Dust stays.
      userInternal'    = userInternal.set(u, userInternal.get(u) - fee - effectiveInternalNet),
      feeDestInternal' = feeDestInternal + fee,
      totalIssuance'   = totalIssuance - effectiveInternalNet,
      status'          = status,
      weight'          = weight,
      mintingFeeRate'  = mintingFeeRate,
      redeemFeeRate'   = redeemFeeRate,
      maxPsmRatio'     = maxPsmRatio,
    }
  }

  // ---------- Donations ----------

  action donate(u: UserId, a: Asset, amount: int): bool = all {
    amount > 0,
    isApproved(status.get(a)),
    amount <= userExt.get(u).get(a),
    reserveExt'      = reserveExt.set(a, reserveExt.get(a) + amount),
    userExt'         = userExt.set(u, userExt.get(u).set(a, userExt.get(u).get(a) - amount)),
    debt'            = debt,
    status'          = status,
    weight'          = weight,
    mintingFeeRate'  = mintingFeeRate,
    redeemFeeRate'   = redeemFeeRate,
    maxPsmRatio'     = maxPsmRatio,
    userInternal'    = userInternal,
    feeDestInternal' = feeDestInternal,
    totalIssuance'   = totalIssuance,
  }

  // ---------- Asset lifecycle ----------

  action addExternalAsset(a: Asset, level: Level): bool = all {
    level == Full,
    status.get(a) == NotApproved,
    assetSlots.filter(x => isApproved(status.get(x))).size() < maxExternalAssets,
    status'         = status.set(a, AllEnabled),
    debt'           = debt,
    weight'         = weight,
    mintingFeeRate' = mintingFeeRate,
    redeemFeeRate'  = redeemFeeRate,
    reserveExt'     = reserveExt,
    maxPsmRatio'    = maxPsmRatio,
    userInternal'   = userInternal,
    feeDestInternal'= feeDestInternal,
    totalIssuance'  = totalIssuance,
    userExt'        = userExt,
  }

  action removeExternalAsset(a: Asset, level: Level): bool = all {
    level == Full,
    isApproved(status.get(a)),
    debt.get(a) == 0,
    status'         = status.set(a, NotApproved),
    weight'         = weight.set(a, 0),
    mintingFeeRate' = mintingFeeRate.set(a, 5),
    redeemFeeRate'  = redeemFeeRate.set(a, 5),
    debt'           = debt,
    reserveExt'     = reserveExt,      // NOT cleared
    maxPsmRatio'    = maxPsmRatio,
    userInternal'   = userInternal,
    feeDestInternal'= feeDestInternal,
    totalIssuance'  = totalIssuance,
    userExt'        = userExt,
  }

  // ---------- Governance ----------

  action setMintingFee(a: Asset, rate: int, level: Level): bool = all {
    level == Full,
    rate >= 0,
    rate <= 1000,
    isApproved(status.get(a)),
    mintingFeeRate' = mintingFeeRate.set(a, rate),
    status'         = status,
    debt'           = debt,
    weight'         = weight,
    redeemFeeRate'  = redeemFeeRate,
    reserveExt'     = reserveExt,
    maxPsmRatio'    = maxPsmRatio,
    userInternal'   = userInternal,
    feeDestInternal'= feeDestInternal,
    totalIssuance'  = totalIssuance,
    userExt'        = userExt,
  }

  action setRedemptionFee(a: Asset, rate: int, level: Level): bool = all {
    level == Full,
    rate >= 0,
    rate <= 1000,
    isApproved(status.get(a)),
    redeemFeeRate'  = redeemFeeRate.set(a, rate),
    status'         = status,
    debt'           = debt,
    weight'         = weight,
    mintingFeeRate' = mintingFeeRate,
    reserveExt'     = reserveExt,
    maxPsmRatio'    = maxPsmRatio,
    userInternal'   = userInternal,
    feeDestInternal'= feeDestInternal,
    totalIssuance'  = totalIssuance,
    userExt'        = userExt,
  }

  action setMaxPsmRatio(r: int, level: Level): bool = all {
    level == Full,
    r >= 0,
    r <= 1000,
    maxPsmRatio'    = r,
    status'         = status,
    debt'           = debt,
    weight'         = weight,
    mintingFeeRate' = mintingFeeRate,
    redeemFeeRate'  = redeemFeeRate,
    reserveExt'     = reserveExt,
    userInternal'   = userInternal,
    feeDestInternal'= feeDestInternal,
    totalIssuance'  = totalIssuance,
    userExt'        = userExt,
  }

  action setWeight(a: Asset, w: int, level: Level): bool = all {
    w >= 0,
    w <= 1000,
    isApproved(status.get(a)),
    weight'         = weight.set(a, w),
    status'         = status,
    debt'           = debt,
    mintingFeeRate' = mintingFeeRate,
    redeemFeeRate'  = redeemFeeRate,
    reserveExt'     = reserveExt,
    maxPsmRatio'    = maxPsmRatio,
    userInternal'   = userInternal,
    feeDestInternal'= feeDestInternal,
    totalIssuance'  = totalIssuance,
    userExt'        = userExt,
  }

  action setCircuitBreaker(a: Asset, lvl: int, mlevel: Level): bool = all {
    lvl >= 0,
    lvl <= 2,
    isApproved(status.get(a)),
    val newStatus =
      if (lvl == 0) AllEnabled
      else if (lvl == 1) MintingDisabled
      else AllDisabled
    all {
      status'         = status.set(a, newStatus),
      debt'           = debt,
      weight'         = weight,
      mintingFeeRate' = mintingFeeRate,
      redeemFeeRate'  = redeemFeeRate,
      reserveExt'     = reserveExt,
      maxPsmRatio'    = maxPsmRatio,
      userInternal'   = userInternal,
      feeDestInternal'= feeDestInternal,
      totalIssuance'  = totalIssuance,
      userExt'        = userExt,
    }
  }

  // ---------- Step ----------

  action step = any {
    nondet u = oneOf(users);
      nondet a = oneOf(assetSlots);
      nondet amt = oneOf(Set(1, 5, 10, 50, 100, 500));
      mint(u, a, amt),
    nondet u = oneOf(users);
      nondet a = oneOf(assetSlots);
      nondet amt = oneOf(Set(1, 5, 10, 50, 100, 500));
      redeem(u, a, amt),
    nondet u = oneOf(users);
      nondet a = oneOf(assetSlots);
      nondet amt = oneOf(Set(1, 10, 100));
      donate(u, a, amt),
    nondet a = oneOf(assetSlots);
      nondet lvl = oneOf(Set(Full, Emergency));
      addExternalAsset(a, lvl),
    nondet a = oneOf(assetSlots);
      nondet lvl = oneOf(Set(Full, Emergency));
      removeExternalAsset(a, lvl),
    nondet a = oneOf(assetSlots);
      nondet r = oneOf(Set(0, 5, 50, 500));
      nondet lvl = oneOf(Set(Full, Emergency));
      setMintingFee(a, r, lvl),
    nondet a = oneOf(assetSlots);
      nondet r = oneOf(Set(0, 5, 50, 500));
      nondet lvl = oneOf(Set(Full, Emergency));
      setRedemptionFee(a, r, lvl),
    nondet r = oneOf(Set(0, 100, 500, 1000));
      nondet lvl = oneOf(Set(Full, Emergency));
      setMaxPsmRatio(r, lvl),
    nondet a = oneOf(assetSlots);
      nondet w = oneOf(Set(0, 100, 500, 1000));
      nondet lvl = oneOf(Set(Full, Emergency));
      setWeight(a, w, lvl),
    nondet a = oneOf(assetSlots);
      nondet l = oneOf(0.to(2));
      nondet mlvl = oneOf(Set(Full, Emergency));
      setCircuitBreaker(a, l, mlvl),
  }
}
