# Engineering & Agent Verification Workflow

See [SPEC.md](file:///home/huynhat/Projects/photon/docs/SPEC.md), [STATE.md](file:///home/huynhat/Projects/photon/docs/STATE.md), and [MEMORY.md](file:///home/huynhat/Projects/photon/docs/MEMORY.md) for current feature specifications, verified status, and `iced 0.14` invariants.

## Verification Tiers
1. **Tier 1 (Fast, during iteration)**:
   - `just test-core` + `just fmt-check`
2. **Tier 2 (Pre-commit / full workspace)**:
   - `just fmt-check && cargo clippy --workspace --all-targets -- -D warnings && just test`
3. **Tier 3 (End-to-End Automation & Real Window)**:
   - Launch `photon --headless` or `photon` GUI -> drive via `photonctl` -> verify `revision` delta (`after != before`), `photonctl debug trace`, `photonctl debug ui-tree`, `photonctl debug screenshot`, and `photonctl debug replay`.

## Bug Triage Order
1. **Domain / State bug**: run `photonctl debug trace --request-id <id>`, inspect `commands.jsonl` and `photon.jsonl.<date>`. Reproduce with `photonctl debug replay --journal <file>`.
2. **UI / Layout / GPU bug**: run `photonctl debug ui-tree` and `photonctl debug screenshot`. Check `presentation.gpuBackend` and widget bounds.
3. **i18n bug**: verify `cargo test -p photon-i18n` and ensure Core emits `UiText { code, args }` rather than literal English sentences.
