
function display {
  subweight compare files \
    --method asymptotic \
    --new $1 \
    --old $2 \
    --unit proof --verbose --threshold 0

  # subweight compare files \
  #   --method asymptotic \
  #   --new $1 \
  #   --old $2 \
  #   --unit time --verbose --threshold 0
}

## Polkadot

echo "#### new: polkadot/pallet_election_provider_multi_block.rs old: kusama"
display "polkadot/measured/pallet_election_provider_multi_block.rs" "kusama/measured/pallet_election_provider_multi_block.rs"

echo "#### new: polkadot/pallet_election_provider_multi_block_signed.rs old: kusama"
display "polkadot/measured/pallet_election_provider_multi_block_signed.rs" "kusama/measured/pallet_election_provider_multi_block_signed.rs"

echo "#### new: polkadot/pallet_election_provider_multi_block_unsigned.rs old: kusama"
display "polkadot/measured/pallet_election_provider_multi_block_unsigned.rs" "kusama/measured/pallet_election_provider_multi_block_unsigned.rs"

echo "#### new: polkadot/pallet_election_provider_multi_block_verifier.rs old: kusama"
display "polkadot/measured/pallet_election_provider_multi_block_verifier.rs" "kusama/measured/pallet_election_provider_multi_block_verifier.rs"
