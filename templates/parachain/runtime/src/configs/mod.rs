mod aura;
mod authorship;
mod balances;
mod collator_selection;
mod cumulus_parachain_system;
mod cumulus_xcmp_queue;
mod message_queue;
mod session;
mod sudo;
mod system;
mod timestamp;
mod transaction_payment;

use crate::*;

impl parachain_info::Config for Runtime {}
impl cumulus_pallet_aura_ext::Config for Runtime {}
