# GitButler Agent Instructions

GitButler is a Rust/Svelte/React/TypeScript monorepo.

## Repo Map

- `crates/` - Rust crates.
- `crates/but-gpui/` - the GPUI desktop client.
- `apps/desktop/` - Tauri/Svelte desktop app.
- `apps/web/` - Svelte web app.
- `apps/but-ui-pantry/` - the pantry, the design system's site.
- `packages/` - shared TypeScript packages, including the SDK.
- `packages/ui-svelte/` - the components the desktop and web apps use.
- `packages/but-ui/`, `packages/but-ui-react/`, `crates/but-ui-gpui/` - the design system:
  tokens, React components, and their GPUI counterparts.
- `e2e/` - Playwright, WebdriverIO, and blackbox end-to-end tests.

## Commits and PRs

Do not leave agent creditation in PRs or Commit messages. These are disallowed
in GitButler repos.

## The Pantries

- "The pantry" is the design system's site: the design notes and every
  component in its states. Run it with `pnpm -F @gitbutler/but-ui-pantry dev`
  (port 6007). It's hosted at <https://pantry.gitbutler.com> from master, and
  a pull request that changes it gets a Vercel preview.
- "The GPUI pantry" is the GPUI client's window of the shared controls. Drive
  it with the `gpui-screenshots` skill (`drive.sh run --stories`).

## Common Commands

Run from the repository root unless stated otherwise. See `DEVELOPMENT.md` for
native prerequisites and platform-specific setup.

### Setup and development

```bash
pnpm install                 # Install JS dependencies and the managed Node runtime
cargo build -p but           # Build only the CLI and its Rust dependencies
cargo run -p but -- --help   # Run the CLI
pnpm dev:desktop             # Run the Tauri/Svelte desktop app
pnpm dev:web                 # Run the Svelte web app
pnpm dev:ui                  # Run the pantry on port 6007
```

Desktop development may require a full `cargo build` first so supplementary
binaries such as `gitbutler-git-askpass` exist.

### Build, check, and lint

```bash
pnpm build                   # Build JS/TS workspace packages
pnpm build:desktop           # Build the desktop frontend
pnpm check                   # Typecheck JS/TS packages
pnpm lint                    # Prettier, ESLint, and oxlint checks
pnpm prettier                # Check formatting without modifying files
pnpm format                  # Format JS/TS files
pnpm fix                     # Apply ESLint fixes

cargo check -p <crate> --all-targets
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
```

Use `cargo fmt` to format Rust code. The nightly `pnpm rustfmt` configuration is
not the default formatting workflow.

### Tests

```bash
pnpm test                    # JS/TS unit tests
pnpm test:ct                 # Svelte Playwright component tests
pnpm test:e2e:playwright     # Playwright E2E tests
pnpm test:e2e                # WebdriverIO non-Tauri E2E tests
pnpm test:e2e:blackbox       # Blackbox WebdriverIO tests

cargo test -p <crate>                       # One Rust crate
cargo test -p <crate> <test-name>           # Filter Rust tests by name
SNAPSHOTS=overwrite cargo test -p <crate> <test-name>
```

Prefer targeted validation before workspace-wide checks.

## TypeScript SDK Generation

`packages/but-sdk` ships TypeScript types only, with no JavaScript. Its
declaration files are generated from Rust JSON schemas by `but-ts`, committed,
and checked in CI. Generated files live in `packages/but-sdk/src/generated`;
do not hand-edit them. After changing a registered Rust type, run:

```bash
pnpm build:sdk
pnpm --filter @gitbutler/but-sdk check
```

The SDK has separate `linear` and `graph` workspace-projection flavors; see
`packages/but-sdk/README.md` for details.

## Working Style

- Before declaring shared behavior done, check each applicable surface and contract
  (desktop, the GPUI client, web, CLI/TUI, SDK types, and docs) and update it or
  explicitly determine that it is unaffected.
- Run targeted validation for the area touched.
- Before adding new machinery to fix a behavior bug, reproduce the bug in a failing
  test and survey the target file's existing loops and classifications as candidate
  hosts; let the tests, not the diagnosis, set how much implementation the fix needs.
- When a fix calls for a new mechanism (a new module, a new public API, or a parallel
  walk where one already exists), propose the intended shape before building it.

## Further Reading

- For Rust work under `crates/`, follow `crates/AGENTS.md`.
- For setup, native dependencies, debugging, and platform-specific builds, see
  `DEVELOPMENT.md`.
- For SDK generation and generated-file rules, see `packages/but-sdk/README.md`.
- Pull-request commits must be signed and show as Verified; see `CONTRIBUTING.md`.
