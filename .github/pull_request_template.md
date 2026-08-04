## Pull request

Thanks for contributing to TLUW.

### Summary
<!-- What does this PR change? -->

### Semver (used when this merges to `main`)
Commit / PR titles should follow [Conventional Commits](https://www.conventionalcommits.org/):

| Prefix | Release bump |
|--------|----------------|
| `feat:` | **minor** (new feature) |
| `fix:` / `perf:` / `chore:` / … | **patch** |
| `feat!:` / `fix!:` / `BREAKING CHANGE:` | **major** |

Example titles: `feat: add cleanup schedule`, `fix: tray quit confirmation`.

Merging to `main` bumps `Cargo.toml`, builds the MSI, and publishes a GitHub Release automatically.
