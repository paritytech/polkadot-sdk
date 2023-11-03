frame_support::parameter_types! {
	/// User fee for ERC20 token transfer back to Ethereum.
	/// (initially was calculated by test `OutboundQueue::calculate_fees` - ETH/ROC 1/400 and fee_per_gas 15 GWEI = 22698000000 + *25%)
	/// Needs to be more than fee calculated from DefaultFeeConfig FeeConfigRecord in snowbridge:parachain/pallets/outbound-queue/src/lib.rs
	pub const BridgeHubEthereumBaseFeeInRocs: u128 = 28372500000;
}
