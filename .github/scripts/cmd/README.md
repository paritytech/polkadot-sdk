# Command Bot Documentation

The command bot allows contributors to perform self-service actions on PRs using comment commands.

## Available Commands

### Label Command (Self-service)

Add labels to your PR without requiring maintainer intervention:

```bash
/cmd label T1-FRAME                                    # Add single label
/cmd label T1-FRAME R0-no-crate-publish-required      # Add multiple labels
/cmd label T1-FRAME A2-substantial D3-involved       # Add multiple labels
```

**Available Labels:**
The bot dynamically fetches all current labels from the repository, ensuring it's always up-to-date. For label meanings and descriptions, see the [official label documentation](https://paritytech.github.io/labels/doc_polkadot-sdk.html).

**Features**:
- **Auto-Correction**: Automatically fixes high-confidence typos (e.g., `T1-FRAM` → `T1-FRAME`)
- **Case Fixing**: Handles case variations (e.g., `I2-Bug` → `I2-bug`)
- **Smart Suggestions**: For ambiguous inputs, provides multiple options to choose from

### Other Commands

```bash
/cmd fmt                           # Format code (cargo +nightly fmt and taplo)
/cmd prdoc                         # Generate PR documentation
/cmd bench                         # Run benchmarks
/cmd update-ui                     # Update UI tests
/cmd --help                        # Show help for all commands
```

### Common Flags

- `--quiet`: Don't post start/end messages in PR
- `--clean`: Clean up previous bot comments
- `--image <image>`: Override docker image

## How It Works

1. **Command Detection**: The bot listens for comments starting with `/cmd` on PRs
2. **Permission Check**: Verifies if the user is an organization member
3. **Command Execution**: Runs the specified command in a containerized environment
4. **Result Handling**:
   - For label commands: Applies labels via GitHub API
   - For other commands: Commits changes back to the PR branch
5. **Feedback**: Posts success/failure messages in the PR

## Security

- Organization member check prevents unauthorized usage
- Commands from non-members run using bot scripts from master branch

## Troubleshooting

If a command fails:
1. Check the GitHub Actions logs linked in the bot's comment
2. Verify the command syntax matches the examples
3. Ensure you have permission to perform the action
4. For label commands, verify the label names are in the allowed list
