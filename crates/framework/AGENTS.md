# Framework Layer (`crates/framework/*`)

This layer is the **domain-agnostic application framework** shared across Photon and future apps (notes, vault, office, etc.).

## Non-negotiable invariants
1. **Zero app/component dependencies**: `fw-*` crates MUST NEVER depend on `crates/apps/*`, `crates/components/*`, or domain-specific media/GPU crates (`image`, `psd`, `rawler`, `resvg`, `jxl-oxide`, `wgpu`).
2. **Parameterize identity & branding**: Never hardcode `"photon"`, `"io.github.tienv2i.Photon"`, or `PHOTON_*` env prefixes inside `fw-*`. Pass `AppBranding`, `AppCore`, or `ProtocolCommand` from the app layer.
3. **Pure-Rust core**: `fw-protocol`, `fw-storage`, `fw-runtime`, and `fw-ipc` must remain GUI-free (`iced`-free). UI primitives live in `fw-ui`, `fw-panels`, `fw-dialogs`, and `fw-shell`.

## Verification
- Check dependency isolation after editing any `fw-*` crate:
  `cargo tree -p fw-<crate> --depth 1`
- Run workspace checks before finishing:
  `just fmt-check && just clippy && just test`
