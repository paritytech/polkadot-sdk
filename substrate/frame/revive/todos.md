# TODOs and Commented Code Added in PR

## Summary
Found 4 new TODO comments and 1 commented code line added in the `pg/revm-refactor` PR.

## Newly Added TODO Comments

1. **Gas limit setting**: `Weight::from_parts(u64::MAX, u64::MAX), // TODO: set the right limit`
2. **Gas limit control flow**: `ControlFlow::Continue(u64::MAX) // TODO: Set the right gas limit`
3. **Input data type**: `pub input: Vec<u8>, // TODO maybe just &'a[u8]`
4. **Memory slice method**: `/// TODO same as slice_mut?`

## Newly Added Commented Code

1. **Disabled Solidity tests**: `// mod sol;` - commented out module import

## Notes
- Most TODOs are related to gas handling optimization
- The commented `mod sol;` appears to intentionally disable Solidity test module
- No critical issues found, mainly optimization reminders