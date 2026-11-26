//! Photoshop-aligned Editor keyboard shortcut table and resolver.
//!
//! Single source of truth for all Editor workspace tool & operation bindings so
//! `photon-desktop` key handling, menu shortcut labels, and automated conflict
//! tests stay in lockstep.

/// Named non-character keys supported by the Editor shortcut table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutNamedKey {
    Delete,
    Backspace,
    Enter,
    Space,
    Tab,
}

/// Normalized key identifier (character keys are always stored lowercase ASCII).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutKey {
    Char(char),
    Named(ShortcutNamedKey),
}

impl ShortcutKey {
    /// Normalize an input character to the canonical lowercase shortcut key.
    #[must_use]
    pub fn from_char(ch: char) -> Self {
        Self::Char(ch.to_ascii_lowercase())
    }
}

/// Modifier + key combination for a shortcut entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: ShortcutKey,
}

impl KeyChord {
    #[must_use]
    pub const fn key(ch: char) -> Self {
        Self {
            ctrl: false,
            shift: false,
            alt: false,
            key: ShortcutKey::Char(ch),
        }
    }

    #[must_use]
    pub const fn shift_key(ch: char) -> Self {
        Self {
            ctrl: false,
            shift: true,
            alt: false,
            key: ShortcutKey::Char(ch),
        }
    }

    #[must_use]
    pub const fn ctrl(ch: char) -> Self {
        Self {
            ctrl: true,
            shift: false,
            alt: false,
            key: ShortcutKey::Char(ch),
        }
    }

    #[must_use]
    pub const fn ctrl_shift(ch: char) -> Self {
        Self {
            ctrl: true,
            shift: true,
            alt: false,
            key: ShortcutKey::Char(ch),
        }
    }

    #[must_use]
    pub const fn named(key: ShortcutNamedKey) -> Self {
        Self {
            ctrl: false,
            shift: false,
            alt: false,
            key: ShortcutKey::Named(key),
        }
    }
}

/// High-level Editor action triggered by a keyboard shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorShortcutAction {
    // --- Photoshop-aligned Tools ---
    ToolMove,
    ToolMarquee,
    ToolMarqueeCycle,
    ToolLasso,
    ToolLassoCycle,
    ToolMagicWand,
    ToolMagicWandCycle,
    ToolCrop,
    ToolEyedropper,
    ToolBrush,
    ToolEraser,
    ToolFillGroup,
    ToolFillGroupCycle,
    ToolPen,
    ToolPathSelect,
    ToolShape,
    ToolShapeCycle,
    ToolText,
    ToolHealingBrush,
    ToolHand,
    ToolZoom,
    ToolLiquify,
    SwapForegroundBackground,
    ToggleQuickMask,
    BrushSizeDecrease,
    BrushSizeIncrease,

    // --- File / Document Operations ---
    NewDocument,
    Save,
    SaveAs,
    Export,
    CloseTab,
    ImageSize,
    CanvasSize,

    // --- Edit / Selection / Transform ---
    Undo,
    Redo,
    Cut,
    Copy,
    CopyMerged,
    Paste,
    PasteIntoSelection,
    FreeTransform,
    SelectAll,
    Deselect,
    Reselect,
    InvertSelection,
    ClearSelectedPixels,

    // --- Layer Operations ---
    NewLayer,
    DuplicateLayer,
    MergeDown,

    // --- Viewport Zoom ---
    ZoomIn,
    ZoomOut,
    ZoomFit,
    ZoomActual,
}

/// Declarative entry in [`EDITOR_SHORTCUT_TABLE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutEntry {
    pub chord: KeyChord,
    pub action: EditorShortcutAction,
    pub display: &'static str,
    pub description: &'static str,
}

/// Canonical Photoshop-aligned Editor shortcut table.
///
/// Every `(ctrl, shift, alt, key)` chord in this table is guaranteed unique by
/// unit test [`editor_shortcut_table_has_no_conflicts`].
pub const EDITOR_SHORTCUT_TABLE: &[ShortcutEntry] = &[
    // --- Tools ---
    ShortcutEntry {
        chord: KeyChord::key('v'),
        action: EditorShortcutAction::ToolMove,
        display: "V",
        description: "Move / Pointer Tool",
    },
    ShortcutEntry {
        chord: KeyChord::key('m'),
        action: EditorShortcutAction::ToolMarquee,
        display: "M",
        description: "Marquee Tool",
    },
    ShortcutEntry {
        chord: KeyChord::shift_key('m'),
        action: EditorShortcutAction::ToolMarqueeCycle,
        display: "Shift+M",
        description: "Cycle Marquee Tools",
    },
    ShortcutEntry {
        chord: KeyChord::key('l'),
        action: EditorShortcutAction::ToolLasso,
        display: "L",
        description: "Lasso Tool",
    },
    ShortcutEntry {
        chord: KeyChord::shift_key('l'),
        action: EditorShortcutAction::ToolLassoCycle,
        display: "Shift+L",
        description: "Cycle Lasso Tools",
    },
    ShortcutEntry {
        chord: KeyChord::key('w'),
        action: EditorShortcutAction::ToolMagicWand,
        display: "W",
        description: "Magic Wand / Quick Select Tool",
    },
    ShortcutEntry {
        chord: KeyChord::shift_key('w'),
        action: EditorShortcutAction::ToolMagicWandCycle,
        display: "Shift+W",
        description: "Cycle Smart Selection Tools",
    },
    ShortcutEntry {
        chord: KeyChord::key('c'),
        action: EditorShortcutAction::ToolCrop,
        display: "C",
        description: "Crop Tool",
    },
    ShortcutEntry {
        chord: KeyChord::key('i'),
        action: EditorShortcutAction::ToolEyedropper,
        display: "I",
        description: "Eyedropper Tool",
    },
    ShortcutEntry {
        chord: KeyChord::key('b'),
        action: EditorShortcutAction::ToolBrush,
        display: "B",
        description: "Brush Tool",
    },
    ShortcutEntry {
        chord: KeyChord::key('e'),
        action: EditorShortcutAction::ToolEraser,
        display: "E",
        description: "Eraser Tool",
    },
    ShortcutEntry {
        chord: KeyChord::key('g'),
        action: EditorShortcutAction::ToolFillGroup,
        display: "G",
        description: "Gradient / Paint Bucket Tool",
    },
    ShortcutEntry {
        chord: KeyChord::shift_key('g'),
        action: EditorShortcutAction::ToolFillGroupCycle,
        display: "Shift+G",
        description: "Cycle Gradient / Paint Bucket",
    },
    ShortcutEntry {
        chord: KeyChord::key('p'),
        action: EditorShortcutAction::ToolPen,
        display: "P",
        description: "Pen / Bezier Tool",
    },
    ShortcutEntry {
        chord: KeyChord::key('a'),
        action: EditorShortcutAction::ToolPathSelect,
        display: "A",
        description: "Path / Node Selection Tool",
    },
    ShortcutEntry {
        chord: KeyChord::key('u'),
        action: EditorShortcutAction::ToolShape,
        display: "U",
        description: "Shape Tool",
    },
    ShortcutEntry {
        chord: KeyChord::shift_key('u'),
        action: EditorShortcutAction::ToolShapeCycle,
        display: "Shift+U",
        description: "Cycle Shape Tools",
    },
    ShortcutEntry {
        chord: KeyChord::key('t'),
        action: EditorShortcutAction::ToolText,
        display: "T",
        description: "Text Tool",
    },
    ShortcutEntry {
        chord: KeyChord::key('j'),
        action: EditorShortcutAction::ToolHealingBrush,
        display: "J",
        description: "Healing Brush Tool",
    },
    ShortcutEntry {
        chord: KeyChord::key('h'),
        action: EditorShortcutAction::ToolHand,
        display: "H",
        description: "Hand Tool",
    },
    ShortcutEntry {
        chord: KeyChord::key('z'),
        action: EditorShortcutAction::ToolZoom,
        display: "Z",
        description: "Zoom Tool",
    },
    ShortcutEntry {
        chord: KeyChord::key('x'),
        action: EditorShortcutAction::SwapForegroundBackground,
        display: "X",
        description: "Swap Foreground / Background Colors",
    },
    ShortcutEntry {
        chord: KeyChord::key('q'),
        action: EditorShortcutAction::ToggleQuickMask,
        display: "Q",
        description: "Toggle Quick Mask Mode",
    },
    ShortcutEntry {
        chord: KeyChord::key('['),
        action: EditorShortcutAction::BrushSizeDecrease,
        display: "[",
        description: "Decrease Brush Size",
    },
    ShortcutEntry {
        chord: KeyChord::key(']'),
        action: EditorShortcutAction::BrushSizeIncrease,
        display: "]",
        description: "Increase Brush Size",
    },
    // --- File & Image ---
    ShortcutEntry {
        chord: KeyChord::ctrl('n'),
        action: EditorShortcutAction::NewDocument,
        display: "Ctrl+N",
        description: "New Document",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('s'),
        action: EditorShortcutAction::Save,
        display: "Ctrl+S",
        description: "Save",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('s'),
        action: EditorShortcutAction::SaveAs,
        display: "Ctrl+Shift+S",
        description: "Save As",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('e'),
        action: EditorShortcutAction::Export,
        display: "Ctrl+Shift+E",
        description: "Export",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('w'),
        action: EditorShortcutAction::CloseTab,
        display: "Ctrl+W",
        description: "Close Active Tab",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('r'),
        action: EditorShortcutAction::ImageSize,
        display: "Ctrl+R",
        description: "Image Size",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('r'),
        action: EditorShortcutAction::CanvasSize,
        display: "Ctrl+Shift+R",
        description: "Canvas Size",
    },
    // --- Edit / Selection / Transform ---
    ShortcutEntry {
        chord: KeyChord::ctrl('z'),
        action: EditorShortcutAction::Undo,
        display: "Ctrl+Z",
        description: "Undo",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('z'),
        action: EditorShortcutAction::Redo,
        display: "Ctrl+Shift+Z",
        description: "Redo",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('y'),
        action: EditorShortcutAction::Redo,
        display: "Ctrl+Y",
        description: "Redo (Alternate)",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('x'),
        action: EditorShortcutAction::Cut,
        display: "Ctrl+X",
        description: "Cut",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('x'),
        action: EditorShortcutAction::ToolLiquify,
        display: "Ctrl+Shift+X",
        description: "Liquify Tool",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('c'),
        action: EditorShortcutAction::Copy,
        display: "Ctrl+C",
        description: "Copy",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('c'),
        action: EditorShortcutAction::CopyMerged,
        display: "Ctrl+Shift+C",
        description: "Copy Merged",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('v'),
        action: EditorShortcutAction::Paste,
        display: "Ctrl+V",
        description: "Paste",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('v'),
        action: EditorShortcutAction::PasteIntoSelection,
        display: "Ctrl+Shift+V",
        description: "Paste Into Selection",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('t'),
        action: EditorShortcutAction::FreeTransform,
        display: "Ctrl+T",
        description: "Free Transform",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('a'),
        action: EditorShortcutAction::SelectAll,
        display: "Ctrl+A",
        description: "Select All",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('d'),
        action: EditorShortcutAction::Deselect,
        display: "Ctrl+D",
        description: "Deselect",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('d'),
        action: EditorShortcutAction::Reselect,
        display: "Ctrl+Shift+D",
        description: "Reselect",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('i'),
        action: EditorShortcutAction::InvertSelection,
        display: "Ctrl+Shift+I",
        description: "Invert Selection",
    },
    ShortcutEntry {
        chord: KeyChord::named(ShortcutNamedKey::Delete),
        action: EditorShortcutAction::ClearSelectedPixels,
        display: "Del",
        description: "Clear Selected Pixels",
    },
    ShortcutEntry {
        chord: KeyChord::named(ShortcutNamedKey::Backspace),
        action: EditorShortcutAction::ClearSelectedPixels,
        display: "Backspace",
        description: "Clear Selected Pixels",
    },
    // --- Layer Operations ---
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('n'),
        action: EditorShortcutAction::NewLayer,
        display: "Ctrl+Shift+N",
        description: "New Layer",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('j'),
        action: EditorShortcutAction::DuplicateLayer,
        display: "Ctrl+J",
        description: "Duplicate Layer / Layer via Copy",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('e'),
        action: EditorShortcutAction::MergeDown,
        display: "Ctrl+E",
        description: "Merge Down",
    },
    // --- Viewport Zoom ---
    ShortcutEntry {
        chord: KeyChord::ctrl('+'),
        action: EditorShortcutAction::ZoomIn,
        display: "Ctrl++",
        description: "Zoom In",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('='),
        action: EditorShortcutAction::ZoomIn,
        display: "Ctrl+=",
        description: "Zoom In",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('+'),
        action: EditorShortcutAction::ZoomIn,
        display: "Ctrl++",
        description: "Zoom In (Shifted +)",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl_shift('='),
        action: EditorShortcutAction::ZoomIn,
        display: "Ctrl++",
        description: "Zoom In (Shifted =)",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('-'),
        action: EditorShortcutAction::ZoomOut,
        display: "Ctrl+-",
        description: "Zoom Out",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('0'),
        action: EditorShortcutAction::ZoomFit,
        display: "Ctrl+0",
        description: "Fit to Screen",
    },
    ShortcutEntry {
        chord: KeyChord::ctrl('1'),
        action: EditorShortcutAction::ZoomActual,
        display: "Ctrl+1",
        description: "100% Actual Size",
    },
    // Quick plain-key zoom conveniences (no conflict with tool letters).
    ShortcutEntry {
        chord: KeyChord::key('+'),
        action: EditorShortcutAction::ZoomIn,
        display: "+",
        description: "Zoom In",
    },
    ShortcutEntry {
        chord: KeyChord::key('='),
        action: EditorShortcutAction::ZoomIn,
        display: "=",
        description: "Zoom In",
    },
    ShortcutEntry {
        chord: KeyChord::key('-'),
        action: EditorShortcutAction::ZoomOut,
        display: "-",
        description: "Zoom Out",
    },
    ShortcutEntry {
        chord: KeyChord::key('0'),
        action: EditorShortcutAction::ZoomFit,
        display: "0",
        description: "Fit to Screen",
    },
    ShortcutEntry {
        chord: KeyChord::key('1'),
        action: EditorShortcutAction::ZoomActual,
        display: "1",
        description: "100% Actual Size",
    },
];

/// Resolve a keyboard press inside the Editor workspace using [`EDITOR_SHORTCUT_TABLE`].
#[must_use]
pub fn resolve_editor_shortcut(
    ctrl: bool,
    shift: bool,
    alt: bool,
    key: ShortcutKey,
) -> Option<EditorShortcutAction> {
    let normalized = match key {
        ShortcutKey::Char(c) => ShortcutKey::Char(c.to_ascii_lowercase()),
        other => other,
    };
    // Match exact chord first.
    let chord = KeyChord {
        ctrl,
        shift,
        alt,
        key: normalized,
    };
    if let Some(entry) = EDITOR_SHORTCUT_TABLE.iter().find(|e| e.chord == chord) {
        return Some(entry.action);
    }
    // For shifted symbol keys like `+` on US keyboards where Shift+= produces '+',
    // fall back to ignoring Shift when ctrl is held or not held if exact symbol matches.
    if shift && matches!(normalized, ShortcutKey::Char('+' | '[' | ']')) {
        let unshifted = KeyChord {
            ctrl,
            shift: false,
            alt,
            key: normalized,
        };
        if let Some(entry) = EDITOR_SHORTCUT_TABLE.iter().find(|e| e.chord == unshifted) {
            return Some(entry.action);
        }
    }
    None
}

/// Return the primary human-readable shortcut badge for an [`EditorShortcutAction`].
#[must_use]
pub fn shortcut_label(action: EditorShortcutAction) -> Option<&'static str> {
    EDITOR_SHORTCUT_TABLE
        .iter()
        .find(|e| e.action == action)
        .map(|e| e.display)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn editor_shortcut_table_has_no_conflicts() {
        let mut seen = HashSet::new();
        for entry in EDITOR_SHORTCUT_TABLE {
            assert!(
                seen.insert(entry.chord),
                "Duplicate shortcut chord {:?} ({}) in EDITOR_SHORTCUT_TABLE",
                entry.chord,
                entry.display
            );
        }
    }

    #[test]
    fn photoshop_tool_letter_shortcuts_match_spec() {
        let expected = [
            ('v', EditorShortcutAction::ToolMove),
            ('m', EditorShortcutAction::ToolMarquee),
            ('l', EditorShortcutAction::ToolLasso),
            ('w', EditorShortcutAction::ToolMagicWand),
            ('c', EditorShortcutAction::ToolCrop),
            ('i', EditorShortcutAction::ToolEyedropper),
            ('b', EditorShortcutAction::ToolBrush),
            ('e', EditorShortcutAction::ToolEraser),
            ('g', EditorShortcutAction::ToolFillGroup),
            ('p', EditorShortcutAction::ToolPen),
            ('a', EditorShortcutAction::ToolPathSelect),
            ('u', EditorShortcutAction::ToolShape),
            ('t', EditorShortcutAction::ToolText),
            ('h', EditorShortcutAction::ToolHand),
            ('z', EditorShortcutAction::ToolZoom),
            ('j', EditorShortcutAction::ToolHealingBrush),
            ('x', EditorShortcutAction::SwapForegroundBackground),
            ('q', EditorShortcutAction::ToggleQuickMask),
            ('[', EditorShortcutAction::BrushSizeDecrease),
            (']', EditorShortcutAction::BrushSizeIncrease),
        ];
        for (ch, action) in expected {
            assert_eq!(
                resolve_editor_shortcut(false, false, false, ShortcutKey::from_char(ch)),
                Some(action),
                "Tool letter '{ch}' must map to {action:?}"
            );
        }
    }

    #[test]
    fn photoshop_ctrl_operation_shortcuts_match_spec() {
        let cases = [
            (true, false, 'z', EditorShortcutAction::Undo),
            (true, true, 'z', EditorShortcutAction::Redo),
            (true, false, 'a', EditorShortcutAction::SelectAll),
            (true, false, 'd', EditorShortcutAction::Deselect),
            (true, true, 'i', EditorShortcutAction::InvertSelection),
            (true, false, 'j', EditorShortcutAction::DuplicateLayer),
            (true, false, 't', EditorShortcutAction::FreeTransform),
            (true, false, 'e', EditorShortcutAction::MergeDown),
            (true, true, 'n', EditorShortcutAction::NewLayer),
            (true, false, 'n', EditorShortcutAction::NewDocument),
            (true, false, 's', EditorShortcutAction::Save),
            (true, true, 's', EditorShortcutAction::SaveAs),
            (true, true, 'e', EditorShortcutAction::Export),
            (true, true, 'x', EditorShortcutAction::ToolLiquify),
            (true, false, '+', EditorShortcutAction::ZoomIn),
            (true, false, '-', EditorShortcutAction::ZoomOut),
            (true, false, '0', EditorShortcutAction::ZoomFit),
            (true, false, '1', EditorShortcutAction::ZoomActual),
        ];
        for (ctrl, shift, ch, expected) in cases {
            assert_eq!(
                resolve_editor_shortcut(ctrl, shift, false, ShortcutKey::from_char(ch)),
                Some(expected),
                "Shortcut ctrl={ctrl} shift={shift} key='{ch}' must map to {expected:?}"
            );
        }
    }

    #[test]
    fn uppercase_key_input_normalizes_cleanly() {
        assert_eq!(
            resolve_editor_shortcut(true, true, false, ShortcutKey::from_char('N')),
            Some(EditorShortcutAction::NewLayer)
        );
        assert_eq!(
            resolve_editor_shortcut(true, true, false, ShortcutKey::from_char('E')),
            Some(EditorShortcutAction::Export)
        );
        assert_eq!(
            resolve_editor_shortcut(true, false, false, ShortcutKey::from_char('E')),
            Some(EditorShortcutAction::MergeDown)
        );
        assert_eq!(
            resolve_editor_shortcut(false, true, false, ShortcutKey::from_char('L')),
            Some(EditorShortcutAction::ToolLassoCycle)
        );
    }
}
