## Summary
- accept EX type 1 on the existing excitation and Hallen RHS path as a staged portability fallback
- emit an explicit CLI warning that EX type 1 is currently treated like EX type 0 while current-source semantics remain pending
- add solver, CLI, and corpus regressions plus support-matrix and README/docs updates

## Validation
- cargo fmt
- cargo test -p nec-cli ex_type1 -- --nocapture
- cargo test -p nec-cli --test corpus_validation -- --nocapture
- ./scripts/validate-doc-frontmatter.sh
- pre-commit hook suite (`cargo test --workspace`)

## Scope notes
- intentionally excluded unrelated local modification in apps/nec-cli/tests/corpus_validation.rs
- left local tmp/ workspace artifacts untracked
