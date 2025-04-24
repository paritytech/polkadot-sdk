
function display {
  subweight compare files \
    --method asymptotic \
    --new $1 \
    --old $1 \
    --unit proof \
    --verbose \
    --threshold 0

  # subweight compare files \
  #   --method asymptotic \
  #   --new $1 \
  #   --old $1 \
  #   --unit time \
  #   --verbose \
  #   --threshold 0
}

## Polkadot

echo "#### polkadot/pallet_election_provider_multi_block.rs"
display "polkadot/measured/pallet_election_provider_multi_block.rs"

echo "#### polkadot/pallet_election_provider_multi_block_signed.rs"
display "polkadot/measured/pallet_election_provider_multi_block_signed.rs"

echo "#### polkadot/pallet_election_provider_multi_block_unsigned.rs"
display "polkadot/measured/pallet_election_provider_multi_block_unsigned.rs"

echo "#### polkadot/pallet_election_provider_multi_block_verifier.rs"
display "polkadot/measured/pallet_election_provider_multi_block_verifier.rs"

## Kusama

echo "#### kusama/pallet_election_provider_multi_block.rs"
display "kusama/measured/pallet_election_provider_multi_block.rs"

echo "#### kusama/pallet_election_provider_multi_block_signed.rs"
display "kusama/measured/pallet_election_provider_multi_block_signed.rs"

echo "#### kusama/pallet_election_provider_multi_block_unsigned.rs"
display "kusama/measured/pallet_election_provider_multi_block_unsigned.rs"

echo "#### kusama/pallet_election_provider_multi_block_verifier.rs"
display "kusama/measured/pallet_election_provider_multi_block_verifier.rs"
