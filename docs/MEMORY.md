# Engineering Memory & `iced 0.14` Gotchas

Lessons learned, subtle invariants, and framework pitfalls verified in the Photon codebase.

## `iced 0.14` UI & Event Gotchas

### 1. `mouse_area::on_move` vs Global `mouse::Event::CursorMoved`
- `mouse_area::on_move` delivers **widget-local** coordinates, which are wrong for viewport-anchored overlays (`with_context_menu` expects window-space coordinates).
- **Pattern**: Subscribe globally in `app::subscription` via `iced::event::listen_with` for `iced::Event::Mouse(mouse::Event::CursorMoved { position }) -> Message::WindowCursorMoved(position)` and store `state.last_cursor`. Context menus in `browser_view.rs` read `state.last_cursor` on right-click.

### 2. `iced::widget::responsive` `Length::Fill` Height Trap
- `iced::widget::responsive(...)` defaults to `Length::Shrink` rather than filling its parent. Inside `stack!` or split containers, omitting `.width(Length::Fill).height(Length::Fill)` collapses the overlay/content area to zero or minimum height.
- **Pattern**: Always chain `.width(Length::Fill).height(Length::Fill)` on `responsive(...)` (see `crates/photon-dialogs/src/overlay.rs` `Cutout` scrim, `crates/photon-desktop/src/browser_view.rs` `build_browser_grid`, and `crates/photon-ui/src/menu.rs` `with_responsive_context_menu`).

### 3. `16px` Default Widget Font Fallback
- In `iced 0.14`, `checkbox`, `text_input`, and `pick_list` default to `16px` text if not explicitly overridden, breaking compact `12px` studio density.
- **Pattern**: Always apply `TEXT_SIZE_CONTROL` (`12.0` from `photon_ui::theme`) via:
  - `checkbox(...).text_size(TEXT_SIZE_CONTROL)`
  - `text_input(...).size(TEXT_SIZE_CONTROL)`
  - `pick_list(...).text_size(TEXT_SIZE_CONTROL)`

### 4. `opaque(...)` Popup & Scrim Shields (and Deliberate Holes)
- `iced::widget::stack!` passes pointer, hover, and scroll events through to underlying layers unless a foreground layer is wrapped in `iced::widget::opaque(...)`.
- **Pattern**:
  - Wrap dialog cards (`photon-dialogs/src/frame.rs`), `DimmedFrosted` scrims, `Cutout` perimeter `strip_widget`s (`photon-dialogs/src/overlay.rs`), and context menus (`photon-ui/src/menu.rs`) in `opaque(...)` and absorb stray scrolls with `.on_scroll(|_| on_noop)` where needed.
  - **Deliberate pass-through holes**:
    - `OverlayMode::Cutout` leaves the center `Space` **unwrapped** (no `opaque`, no `mouse_area`) so pan/zoom/wheel events reach the `Shader` canvas beneath.
    - `with_anchored_menu` leaves the top menubar band (`0..top_offset`) uncovered by its dismiss layer so menubar clicks and hover-to-switch (`menubar_row_hoverable`) continue working while a menu is open.

### 5. Content-Auto-Fit (`Length::Shrink`) vs Manual Dialog Resize
- Pre-allocating fixed dialog heights clips or leaves awkward dead space across DPI/translations.
- **Pattern**: `build_dialog_frame` (`crates/photon-dialogs/src/frame.rs`) renders centered unresized dialogs (`!user_resized`) with `Length::Shrink` body/card capped at `max_card_h`, then switches to `Length::Fixed(rect.height)` + `Length::Fill` scrollable body once the user drags any `ResizeEdge`.
- When `!user_moved && !user_resized`, `render_managed_dialog` uses `.center_x(Length::Fill).center_y(Length::Fill)` rather than fixed `(x, y)` padding so auto-fitted heights stay truly centered.

## State & Core Invariants

### 6. Preventing Layout Roundtrip Races (`commit_layout_with`)
- GUI layout edits (`SetLayout`) go asynchronously through `CoreHandle::dispatch` while `watch` (`state_watch_stream`) pushes snapshots from unrelated commands. Without tracking in-flight layout commands, an older snapshot can overwrite newer local panel toggles/orders/widths.
- **Pattern** (`crates/photon-desktop/src/app.rs`):
  - `commit_layout_with` mutates `state.snapshot.meta.layout` optimistically, increments `state.pending_layout_commands`, and dispatches `Command::SetLayout`.
  - `Message::LayoutCommandCompleted` decrements `pending_layout_commands`.
  - `Message::StateUpdated` only syncs local panel flags/orders from `snap.meta.layout` when `state.pending_layout_commands == 0 && snap.meta.layout != state.last_core_layout`. While `pending_layout_commands > 0`, `apply_live_layout_fields` re-applies local layout state onto the incoming snapshot.

### 7. Dual-Path Panel State (`LayoutState` + `SidebarOrderState`)
- Panel order vectors in `LayoutState` can contain duplicates, missing variants, or legacy fields from older `config.json` files.
- **Pattern**: Both `LayoutState::sanitized()` (`photon-protocol`) and `SidebarOrderState::normalize_order()` (`photon-panels`) deduplicate, drop unknown entries, and append missing canonical variants (`BrowserLeftPanel::ALL`, `ViewerRightPanel::ALL`, `EditorRightPanel::ALL`). Legacy removed fields in `config.json` are tolerated via `#[serde(default)]`.

### 8. `photon-i18n` Key & Placeholder Parity
- `Core` never formats translated prose; it emits `UiText { code, args }` using `snake_case` codes. `tr_ui_text` converts `snake_case` to `kebab-case` Fluent IDs and looks them up in `en-US/main.ftl` / `vi-VN/main.ftl`.
- Any new Fluent key or `$variable` must exist identically in both `locales/en-US/main.ftl` and `locales/vi-VN/main.ftl`, verified by `en_and_vi_catalogues_match_keys_and_placeholders` and `every_core_and_image_ui_text_code_translates_in_en_and_vi`.

### 9. Serde Compatibility for Disabled Features
- Keep disabled protocol variants (`Command::ViewerTranslate`, etc.) and `Config.translate` (`TranslateSettings`) deserializable so historical `commands.jsonl` journals and user configs load cleanly; reject execution at `Core::dispatch` with `ProtocolError::line("feature_disabled", UiText::new("ai_feature_disabled"))`.
