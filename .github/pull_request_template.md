## Description

Describe the changes in this PR. Reference relevant sections of `docs/PRD.md` or `docs/ARCHITECTURE.md` if applicable.

## Related issues

Closes #issue_number (if applicable)

## Type of change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to change)
- [ ] Documentation or commentary

## Checklist

### Code quality

- [ ] `just ci` passes green (build, test, check-json, check-emdash, check-microcopy, check-airgap)
- [ ] No em-dashes in any text (use hyphens: -, or colons: :)
- [ ] Tests are added or updated (or marked as TODO with a brief reason)

### Contracts and fixtures

- [ ] Contracts are append-only (no renames or removals of existing fields)
- [ ] Fixtures are regenerated if behavior changed: `just fixtures`
- [ ] All contract JSONs are valid: included in `just ci`

### Default-mode UI

- [ ] Default-mode microcopy contains no internal terms from `contracts/microcopy_glossary.json`
- [ ] Copy is reviewed against the glossary and the non-coder design bar (see CONTRIBUTING.md)

### Safety and security

- [ ] If this adds a capability or action: permissions are checked at execution time
- [ ] If this touches a hard safety invariant: regression test is added and documented
- [ ] If this modifies the audit log or kill switch: latency or halt behavior is verified

### Commits

- [ ] Commits use conventional commit format (feat:, fix:, test:, docs:, etc.)
- [ ] Commits are authored as `Josef Long <Josefdean@protonmail.com>`
- [ ] No "Co-Authored-By" or AI attribution lines

## Testing

Describe how you tested this change:
- Did you run the full test suite?
- Did you test the zero-code default-mode path (if UI-related)?
- Did you verify drift repair, if touching the trajectory compiler?

## Notes

Any additional context for reviewers?
