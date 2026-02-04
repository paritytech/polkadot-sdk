
## Future Work / Pending Tasks

- **POV Space Reservation**: Limit the POV space usable by blocks to reserve space for scheduling proof data. This is crucial for resubmission support - otherwise resubmission might fail due to insufficient space. Must be enforced via the runtime.
- **Collation Resubmission**: there are a bunch of cases where a collator can resubmit a block built by another collator, which was not backed in time.
