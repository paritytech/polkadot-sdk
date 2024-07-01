use polkadot_node_subsystem_util::runtime::ProspectiveParachainsMode;
use polkadot_primitives::{CollatorId, Id as ParaId};

use sp_core::sr25519;

use super::Collations;

#[test]
fn cant_add_more_than_claim_queue() {
	let para_a = ParaId::from(1);
	let para_b = ParaId::from(2);
	let assignments = vec![para_a, para_b, para_a];
	let relay_parent_mode =
		ProspectiveParachainsMode::Enabled { max_candidate_depth: 4, allowed_ancestry_len: 3 };

	let mut collations = Collations::new(&assignments);

	// first collation for `para_a` is in the limit
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_a, 0));
	collations.note_fetched(para_a);
	// and `para_b` is not affected
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_b, 0));

	// second collation for `para_a` is also in the limit
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_a, 0));
	collations.note_fetched(para_a);

	// `para_b`` is still not affected
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_b, 0));

	// third collation for `para_a`` will be above the limit
	assert!(collations.is_collations_limit_reached(relay_parent_mode, para_a, 0));

	// one fetch for b
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_b, 0));
	collations.note_fetched(para_b);

	// and now both paras are over limit
	assert!(collations.is_collations_limit_reached(relay_parent_mode, para_a, 0));
	assert!(collations.is_collations_limit_reached(relay_parent_mode, para_b, 0));
}

#[test]
fn pending_fetches_are_counted() {
	let para_a = ParaId::from(1);
	let collator_id_a = CollatorId::from(sr25519::Public::from_raw([10u8; 32]));
	let para_b = ParaId::from(2);
	let assignments = vec![para_a, para_b, para_a];
	let relay_parent_mode =
		ProspectiveParachainsMode::Enabled { max_candidate_depth: 4, allowed_ancestry_len: 3 };

	let mut collations = Collations::new(&assignments);
	collations.fetching_from = Some((collator_id_a, None));

	// first collation for `para_a` is in the limit
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_a, 1));
	collations.note_fetched(para_a);

	// second collation for `para_a`` is not in the limit due to the pending fetch
	assert!(collations.is_collations_limit_reached(relay_parent_mode, para_a, 1));
}
