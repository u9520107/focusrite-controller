# Licensing

## Project license

Focusrite Controller is licensed under the MIT License. The authoritative text
is in the repository-root [LICENSE](../LICENSE). Rust metadata uses the SPDX
identifier `MIT`; future web package metadata must do the same.

`Focusrite` and `Scarlett` remain trademarks of Focusrite Audio Engineering
Limited. The project license grants no trademark rights and does not imply
affiliation or endorsement.

## Dependency policy

Every direct dependency added in any phase must have a reviewed license that is
compatible with MIT distribution. Do not assume a dependency is MIT because it
is published on a public registry.

Before distributing a binary, `.deb`, container, static web bundle, or source
archive:

1. inventory direct and transitive dependencies for Rust and web artifacts;
2. review licenses, exceptions, notices, and bundled assets;
3. include a generated `THIRD_PARTY_NOTICES` file in the distributed artifact
   when required by those licenses;
4. explicitly review copyleft, non-commercial, source-available, or unknown
   licenses before accepting them.

Do not commit a static third-party notice inventory during early development:
it becomes stale as lockfiles change. Generate and verify it as part of
packaging in Phase 8.

## Upstream patches and contributions

Preserve upstream copyright and license notices when patching or temporarily
forking a dependency. Prefer upstream pull requests and released fixes over a
permanent fork. Only contribute code to this repository when authorized to
license that contribution under MIT.

## Phase records

Each active phase execution plan records licensing decisions and dependency
blockers. Update it whenever a dependency introduces a non-routine license or
notice obligation.
