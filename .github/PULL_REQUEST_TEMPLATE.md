## Summary

Describe the problem and the intended change in a few precise sentences.

## Boundary affected

Which Phantom subsystem, crate, invariant or public contract changes?

## Verification

List the commands, tests, fixtures or evaluations used to verify the change.

```text
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Add focused tests or other gates when applicable.

## Security impact

- Trust boundary affected:
- New privileged capability or side effect: yes / no
- New dependency or supply-chain impact: yes / no
- Adversarial input considered:

## Compatibility impact

Describe any intentional change in HTML/CSS/network/browser behavior or state that there is none.

## Review checklist

- [ ] The change is small and responsibility-focused.
- [ ] No unrelated refactor is bundled with security-sensitive work.
- [ ] New failure modes are bounded and use typed errors where applicable.
- [ ] New public behavior has tests.
- [ ] Documentation or ADRs were updated when contracts changed.
- [ ] No `unwrap()`, `expect()`, `panic!()`, `todo!()` or `unimplemented!()` was added to production-critical paths.
- [ ] No new `unsafe` code was introduced without an explicit approved safety boundary.
