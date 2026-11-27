# Reusable Components Layer (`crates/components/*`)

This layer hosts **domain-reusable components** (similar to Django reusable apps) shared by multiple desktop/headless applications.

## Non-negotiable invariants
1. **Depend downward only**: Component crates may depend on `crates/framework/*` and lightweight utility crates, NEVER on any concrete app crate in `crates/apps/*`.
2. **Adapter traits over concrete domain structs**: Expose traits (e.g. `FileEntryLike`) or generic parameters so apps adapt their own domain types without coupling the component to one app.
3. **Headless-safe logic first**: Keep data models, filtering, sorting, and viewport math in pure Rust without pulling `iced`/`wgpu` unless the component is explicitly a UI widget.

## Verification
- Check dependency isolation: `cargo tree -p comp-<name> --depth 1`
- Run workspace checks: `just fmt-check && just clippy && just test`
