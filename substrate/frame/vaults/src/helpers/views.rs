use super::*;

pub fn view_vault_status<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
) -> Option<VaultStatus> {
	let vault = Vaults::<T>::get(collateral_id, owner)?;
	Some(vault.status::<T>(collateral_id, owner))
}
pub fn view_vault_cr<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
) -> Option<FixedU128> {
	let vault = Vaults::<T>::get(collateral_id, owner)?;
	let bs = BranchStates::<T>::get(collateral_id)?;
	let now = T::TimeProvider::now();
	let coll = T::CollateralAssets::balance_on_hold(
		collateral_id.clone(),
		&HoldReason::VaultCollateral.into(),
		owner,
	);
	let price = T::Oracle::provide_price(collateral_id).ok()?.price;
	let pending = pending_touch_for::<T>(&vault, &bs, now);
	let total_coll = coll.saturating_add(pending.collateral);
	let total_debt = vault
		.debt
		.total()
		.saturating_add(pending.principal)
		.saturating_add(pending.interest);
	math::collateralization_ratio::<BalanceOf<T>>(total_coll, total_debt, price)
}

pub fn view_branch_tcr<T: Config>(collateral_id: &T::AssetId) -> Option<FixedU128> {
	let bs = BranchStates::<T>::get(collateral_id)?;
	let price = T::Oracle::provide_price(collateral_id).ok()?.price;
	let now = T::TimeProvider::now();
	compute_tcr::<T>(&bs, price, now).ok()
}

pub fn view_redemption_queue_head<T: Config>(
	collateral_id: &T::AssetId,
	n: u32,
) -> Vec<T::AccountId> {
	let mut out: Vec<T::AccountId> = Vec::with_capacity(n as usize);
	out.extend(recovery::queue_head::<T>(collateral_id, n));
	if out.len() as u32 >= n {
		return out;
	}
	if let Some(bs) = BranchStates::<T>::get(collateral_id) {
		if let Some(owner) = bs.last_dormant_vault_owner {
			out.push(owner);
		}
	}
	let remaining = n.saturating_sub(out.len() as u32);
	if remaining > 0 {
		out.extend(T::VaultLists::iter_from_tail(&rate_list_id(collateral_id), remaining));
	}
	out
}

pub fn view_debt_in_front<T: Config>(collateral_id: &T::AssetId, rate: FixedU128) -> BalanceOf<T> {
	// Walk tail-first; sum interest_bearing_debt while node.priority < rate.
	let mut total = BalanceOf::<T>::zero();
	let rate_list = rate_list_id(collateral_id);
	let mut cursor = T::VaultLists::tail(&rate_list);
	while let Some(o) = cursor {
		let priority = match T::VaultLists::priority(&rate_list, &o) {
			Some(p) => p,
			None => break,
		};
		if priority >= rate {
			break;
		}
		if let Some(v) = Vaults::<T>::get(collateral_id, &o) {
			total = total.saturating_add(v.debt.principal);
		}
		cursor = match T::VaultLists::neighbors(&rate_list, &o) {
			Some(p) => p.prev,
			None => break,
		};
	}
	total
}

pub fn predict_upfront_fee_open<T: Config>(
	collateral_id: &T::AssetId,
	initial_debt: BalanceOf<T>,
	annual_rate: FixedU128,
) -> BalanceOf<T> {
	match (BranchConfigs::<T>::get(collateral_id), BranchStates::<T>::get(collateral_id)) {
		(Some(cfg), Some(bs)) => open_upfront_fee::<T>(&bs, &cfg, initial_debt, annual_rate),
		_ => BalanceOf::<T>::zero(),
	}
}

pub fn predict_upfront_fee_borrow<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
	debt_increase: BalanceOf<T>,
	maybe_new_rate: Option<FixedU128>,
) -> BalanceOf<T> {
	let (cfg, bs, vault) = match predict_inputs::<T>(collateral_id, owner) {
		Some(t) => t,
		None => return BalanceOf::<T>::zero(),
	};
	let new_rate = maybe_new_rate.unwrap_or(vault.annual_rate);
	let now = T::TimeProvider::now();
	let cooldown_elapsed =
		now.saturating_sub(vault.last_rate_update) >= cfg.rate_adjustment_cooldown;
	let rate_change_fee_base = if maybe_new_rate.is_some() && !cooldown_elapsed {
		vault.debt.principal
	} else {
		BalanceOf::<T>::zero()
	};
	simulate_borrow::<T>(&bs, &cfg, &vault, debt_increase, new_rate, rate_change_fee_base).1
}

pub fn predict_upfront_fee_rate_change<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
	new_rate: FixedU128,
) -> BalanceOf<T> {
	let (cfg, bs, vault) = match predict_inputs::<T>(collateral_id, owner) {
		Some(t) => t,
		None => return BalanceOf::<T>::zero(),
	};
	let now = T::TimeProvider::now();
	let cooldown_elapsed =
		now.saturating_sub(vault.last_rate_update) >= cfg.rate_adjustment_cooldown;
	simulate_change_rate::<T>(&bs, &cfg, &vault, new_rate, cooldown_elapsed).1
}

/// Read the `(cfg, branch state, vault)` triple for a `predict_*` view.
/// Returns `None` if any row is missing — the predict APIs treat that as
/// "no fee" rather than an error.
fn predict_inputs<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
) -> Option<(
	BranchConfig<BalanceOf<T>, MomentOf<T>>,
	BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	Vault<BalanceOf<T>, MomentOf<T>>,
)> {
	Some((
		BranchConfigs::<T>::get(collateral_id)?,
		BranchStates::<T>::get(collateral_id)?,
		Vaults::<T>::get(collateral_id, owner)?,
	))
}
