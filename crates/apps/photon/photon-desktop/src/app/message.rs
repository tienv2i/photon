use super::*;

#[derive(Debug, Clone)]
pub enum Message {
    StateUpdated(Arc<UiStateSnapshot>),
    CoreEvent(photon_protocol::Event),
    KeyEvent(iced::keyboard::Event),
    WindowOpened(window::Id),
    WindowFocused(window::Id),
    WakeWindow(window::Id),
    WakeWindowRaise(window::Id),
    WakeWindowSettle(window::Id),
    WindowMonitorSizeCheck(window::Id, Option<Size>, bool),
    WindowClosed(window::Id),
    WindowCloseRequested(window::Id),
    RequestAppExit,
    RecoverySyncTick,
    RestoreRecoverySession,
    DiscardRecoverySession,
    WindowResized(window::Id, Size),
    WindowRescaled(window::Id, f32),
    PersistWindowSize(u64, u32, u32),
    PersistWindowSizeIfWindowed(u64, u32, u32, bool),
    Dispatch(Command),
    CommandCompleted(Result<CommandResponse, String>),
    LayoutCommandCompleted(Result<CommandResponse, String>),
    PickFolderDialog,
    FolderPicked(Option<PathBuf>),
    PickFavoriteFolderDialog,
    FavoriteFolderPicked(Option<PathBuf>),
    PickImagesDialog,
    ImagesPicked(Vec<PathBuf>),
    PickSaveAsDialog,
    PickSaveOpenRasterDialog,
    SaveAsPicked(Option<PathBuf>),
    OpenNewFolderDialog,
    NewFolderNameChanged(String),
    ConfirmNewFolder,
    OpenSelectionModifyDialog(SelectionModifyKind),
    SelectionModifyValueChanged(String),
    ConfirmSelectionModify,
    RequestResetLayout,
    ConfirmResetLayout,
    RequestEditorLeave {
        close_editor: bool,
    },
    RequestEditorLeaveToBrowser,
    DiscardAndLeaveEditor(bool),
    SaveAndLeaveEditor(bool),
    CloseDialog,
    OpenResizeDialog,
    ResizeDimAction(DimGridAction),
    ResizeFilterChanged(ResizeFilter),
    ResizeModeChanged(ResizeMode),
    ConfirmResize,
    OpenCanvasSizeDialog,
    CanvasSizeDimAction(DimGridAction),
    CanvasSizeAnchorChanged(photon_protocol::CanvasAnchor),
    ConfirmCanvasSize,
    ToggleCropMode,
    SetCropPreset(Option<f32>),
    SetCropField(photon_editor::CropRectField, String),
    ApplyCrop,
    CancelCrop,
    DismissToast(Option<u64>),
    ToggleMenu(ActiveMenu),
    HoverMenu(ActiveMenu),
    SetMenuSubmenu(&'static str),
    CloseMenu,
    OpenSliceImageDialog,
    SliceImageModeChanged(bool),
    SliceImageRowsChanged(String),
    SliceImageColsChanged(String),
    SliceImageStripHeightChanged(String),
    SliceImageOutputDirChanged(String),
    SliceImagePrefixChanged(String),
    SliceImageFormatChanged(String),
    ConfirmSliceImage,
    OpenShortcutsDialog,
    ToggleBrowserShowHidden,
    ToolGroupPressed(EditorToolGroup),
    ToolGroupReleased(EditorToolGroup),
    ToolGroupHoldTimeout(EditorToolGroup, u64),
    OpenToolGroupFlyout(EditorToolGroup),
    CloseToolGroupFlyout,
    OpenAboutDialog,
    WindowCursorMoved(Point),
    WindowMouseUp,
    WindowUnfocused,
    DialogEvent(DialogEvent<DesktopDialogId>),
    ToggleBrowserInspector,
    BrowserOpenSelectedInEditor,
    BrowserOpenSelectedInViewer,
    SetUiScalePct(u16),
    SetFontScalePct(u16),
    SetDisplayFont(DisplayFont),
    ToggleBrowserFilterBar,
    ToggleBrowserFilterCaseSensitive,
    ToggleBrowserFilterRegex,
    ClearBrowserFilter,
    BeginDragBrowserPanel(BrowserLeftPanel),
    HoverBrowserPanelDrop(BrowserLeftPanel),
    DropBrowserPanel(BrowserLeftPanel),
    SwapBrowserLeftPanels,
    ToggleBrowserPanelVisible(BrowserLeftPanel),
    ToggleBrowserLeftPanelCollapse(BrowserLeftPanel),
    CloseBrowserLeftPanel(BrowserLeftPanel),
    BeginDragBrowserRightPanel(BrowserRightPanel),
    HoverBrowserRightPanelDrop(BrowserRightPanel),
    DropBrowserRightPanel(BrowserRightPanel),
    SwapBrowserRightPanels,
    ToggleBrowserRightPanelVisible(BrowserRightPanel),
    ToggleBrowserRightPanelCollapse(BrowserRightPanel),
    CloseBrowserRightPanel(BrowserRightPanel),
    PanelPopupToggleBrowserLeft(BrowserLeftPanel),
    PanelPopupToggleBrowserRight(BrowserRightPanel),
    PanelPopupToggleEditorTools,
    PanelPopupToggleEditorPanel(EditorRightPanel),
    BeginDragViewerPanel(ViewerRightPanel),
    HoverViewerPanelDrop(ViewerRightPanel),
    DropViewerPanel(ViewerRightPanel),
    SwapViewerRightPanels,
    ToggleViewerPanelCollapse(ViewerRightPanel),
    CloseViewerPanel(ViewerRightPanel),
    SelectEditorToolTab(EditorToolTab),
    ToggleEditorToolsVisible,
    CloseEditorToolsPanel,
    ToggleEditorPanelVisible(EditorRightPanel),
    ToggleEditorPanelCollapse(EditorRightPanel),
    CloseEditorPanel(EditorRightPanel),
    BeginDragEditorPanel(EditorRightPanel),
    HoverEditorPanelDrop(EditorRightPanel),
    DropEditorPanel(EditorRightPanel),
    SwapEditorPanelOrder,
    EditorCancelLayerRename,
    OpenViewerContextMenu {
        path: Option<String>,
        pos: Option<[f32; 2]>,
    },
    OpenLayerContextMenu {
        layer_id: u64,
    },
    OpenBrowserContextMenu {
        path: String,
        x: f32,
        y: f32,
    },
    OpenQuickAccessContextMenu {
        path: String,
        x: f32,
        y: f32,
    },
    CloseContextMenu,
    ToggleContextPluginsSubmenu,
    OpenContextPluginsSubmenu,
    ToggleContextZoomSubmenu,
    OpenContextZoomSubmenu,
    ToggleContextPluginGroup(String),
    OpenContextPluginGroup(String),
    FilmstripWheelScroll(iced::mouse::ScrollDelta),
    ContextMenuDispatch(Command),
    CopyImageToClipboard(Option<String>),
    CopyImageCompleted(bool),
    RevealInFolder(String),
    ViewerCanvas(Option<String>, CanvasEvent),
    ViewerZoomActual,
    ViewerRotatePreview(i8),
    ToggleFullscreen,
    ToggleFilmstrip,
    CloseFilmstrip,
    /// Horizontal scroll offset of the filmstrip in logical pixels.
    FilmstripScrolled {
        offset: f32,
    },
    ToggleViewerInspector,
    CopyViewerPath,
    ToggleViewerColorPopover,
    SetViewerAdjExposure(f32),
    SetViewerAdjGamma(f32),
    SetViewerAdjBrightness(f32),
    SetViewerAdjContrast(f32),
    SetViewerAdjSaturation(f32),
    SetViewerAdjHighlights(f32),
    SetViewerAdjShadows(f32),
    ResetViewerAdj,
    ViewerAdjOpenInEditor,
    ViewerSplitterDrag(f32),
    ViewerSplitterRelease(f32),
    ViewerRightSplitDrag(f32),
    ViewerRightSplitRelease(f32),
    TogglePathEdit(bool),
    FolderInputChanged(String),
    SubmitFolderInput,
    BrowserFilterChanged(String),
    SetBrowserViewMode(BrowserViewMode),
    SetBrowserSortField(BrowserSortField),
    SetBrowserSortOrder(SortOrder),
    SetBrowserThumbSize(BrowserThumbSize),
    OpenBatchDialog(BatchDialogKind),
    BatchDialog(BatchDialogMsg),
    BatchInputFolderPicked(Option<PathBuf>),
    BatchOutputFolderPicked(Option<PathBuf>),
    BrowserSplitterDrag(f32),
    BrowserSplitterRelease(f32),
    BrowserRightSplitterDrag(f32),
    BrowserRightSplitterRelease(f32),
    BrowserCopyPath(String),
    BrowserStartRename(String),
    BrowserRenameChanged(String),
    BrowserConfirmRename,
    BrowserCopyTo(Vec<String>),
    BrowserMoveTo(Vec<String>),
    BrowserCopyMoveDestPicked {
        is_move: bool,
        paths: Vec<String>,
        dest: Option<PathBuf>,
    },
    BrowserConfirmDeletePermanently(Vec<String>),
    BrowserExecuteDeletePermanently(Vec<String>),
    DockSplitterDrag(DockSplitterKind, f32),
    DockSplitterRelease(DockSplitterKind, f32),
    SetViewerViewMode(ViewerViewMode),
    SetDefaultViewerViewMode(ViewerViewMode),
    DualStepSpread(isize),
    SetMangaRtl(bool),
    SetMangaSeparateCover(bool),
    SetMangaPageGap(u16),
    SetWebtoonFitMode(WebtoonFitMode),
    SetSystemOpenWindowMode(SystemOpenWindowMode),
    SetViewerFixedWidth(u32),
    SetViewerFixedHeight(u32),
    SetAppLaunchWindowMode(AppLaunchWindowMode),
    SetAntiAliasingMode(CanvasAntiAliasingMode),
    SetViewportDenoiseMode(ViewportDenoiseMode),
    WebtoonScrolled(scrollable::Viewport),
    WebtoonPagesLoaded(Vec<(String, u32, u32, u32, u32, Vec<u8>)>),
    Browser(BrowserMessage),
    BrowserVisibleRangeChanged {
        start: usize,
        end: usize,
    },
    Viewer(ViewerMessage),
    Editor(EditorMessage),
    BrowserEntryActivated {
        path: String,
        is_dir: bool,
        is_image: bool,
    },
    OpenExternally(String),
    OpenWith(String),
    ShowInBrowser(String),
    BrowserEditPath(String),
    ThumbnailsLoaded(Vec<String>, Vec<(String, u32, u32, Vec<u8>)>),
    ViewerImageSizesProbed(Vec<(String, (u32, u32))>),
    OpenNewDocumentDialog,
    NewDocDimAction(DimGridAction),
    NewDocApplyPreset(u32, u32),
    NewDocSetBackground(NewDocBackground),
    OpenNewDocColorPicker,
    ConfirmNewDocument,
    ToggleFolderExpand(String),
    FolderChildrenLoaded(String, Vec<(String, String)>),
    ZoomStep(f32),
    Canvas(CanvasEvent),
    SetBrightness(f32),
    SetContrast(f32),
    SetSaturation(f32),
    SetExposure(f32),
    SetGamma(f32),
    SetHighlights(f32),
    SetShadows(f32),
    ApplyColorAdjustments,
    EditorToggleApplyToSelection(bool),
    EditorToggleAdjustLivePreview,
    DenoiseDialogSelectionOnlyChanged(bool),
    EditorResetPendingAdjustments,
    EditorSelectTool(EditorTool),
    EditorOpenAdjustPopup(AdjustmentPopupTab),
    EditorCloseAdjustPopup {
        apply: bool,
    },
    EditorSetLiquifyMode(LiquifyMode),
    EditorSetLiquifyRadius(f32),
    EditorSetLiquifyStrength(f32),
    EditorCycleCompareMode,
    EditorSetCompareMode(CompareMode),
    EditorSwapColors,
    EditorResetColors,
    OpenColorPicker(ColorPickerTarget),
    ColorPickerSetRgb(u8, u8, u8),
    ColorPickerPickScreen,
    ColorPickerScreenPicked(Option<[u8; 3]>),
    ColorPickerHexChanged(String),
    ColorPickerToggleTransparent(bool),
    ResetViewportBackground,
    ConfirmColorPicker,
    SetViewerViewportBackground(Option<[u8; 3]>),
    ModifiersChanged(Modifiers),
    ShutdownAfterSave,
    SaveCurrentUiLayout,
    ApplySafeDefaultsToDisk,
    EditorSetBrushRadius(f32),
    EditorSetBrushOpacity(f32),
    EditorSetBrushHardness(f32),
    EditorSetBrushFlow(f32),
    EditorSetBrushSpacing(f32),
    EditorSetBrushTip(photon_protocol::BrushTip),
    EditorSetApplyTarget(ApplyTarget),
    EditorSetHsvHue(f32),
    EditorSetHsvSat(f32),
    EditorSetHsvVal(f32),
    EditorApplyHsv,
    EditorSetBalanceZone(ColorBalanceZone),
    EditorSetBalanceAxis(usize, f32),
    EditorTogglePreserveLuminosity(bool),
    EditorApplyColorBalance,
    EditorSelectCurvesChannel(CurvesChannel),
    EditorCurvesChanged(Vec<[f32; 2]>),
    EditorResetActiveCurvesChannel,
    EditorApplyCurves,
    EditorZoomActual,
    CanvasPanTo {
        pan_x: f32,
        pan_y: f32,
    },
    EditorSetFloodFillOpacity(f32),
    EditorToggleFloodFillSampleAll,
    EditorSetGradientOpacity(f32),
    EditorToggleGradientReverse,
    EditorToggleGradientDither,
    EditorSetGradientRepeat(GradientRepeat),
    EditorSetGradientBlendMode(BlendMode),
    EditorToggleGradientInteractive,
    EditorApplyGradientPreset(String),
    EditorSelectGradientStop(usize),
    EditorAddGradientStop(f64),
    EditorRemoveSelectedGradientStop,
    EditorSetGradientStopPos(usize, f64),
    EditorToggleGradientStopAlpha(usize),
    EditorSetGradientStopFg(usize),
    EditorApplyGradientDraft,
    EditorCancelGradientDraft,
    EditorSetHealingMode(HealingMode),
    EditorSetHealingRadius(f32),
    EditorSetHealingHardness(f32),
    EditorSetHealingStrength(f32),
    EditorToggleHealingSampleAll,
    EditorSetHealingSource(Option<[f32; 2]>),
    EditorSetDenoiseStrength(f32),
    EditorSetDenoiseDetail(f32),
    OpenDenoiseDialog,
    DenoiseDialogStrengthChanged(f32),
    DenoiseDialogDetailChanged(f32),
    DenoiseDialogTargetChanged(ApplyTarget),
    ConfirmDenoiseDialog,
    OpenSaveSelectionDialog,
    SaveSelectionNameChanged(String),
    SaveSelectionTargetChanged(Option<u64>),
    SaveSelectionOpChanged(SelectionBooleanOp),
    ConfirmSaveSelection,
    OpenLoadSelectionDialog,
    LoadSelectionSelectedChanged(u64),
    LoadSelectionOpChanged(SelectionBooleanOp),
    LoadSelectionInvertChanged(bool),
    DeleteSavedSelection(u64),
    ConfirmLoadSelection,
    OpenNewGuideDialog,
    NewGuideAxisChanged(photon_canvas::GuideAxis),
    NewGuideValueChanged(String),
    NewGuideUnitChanged(bool),
    ConfirmNewGuide,
    OpenGridSettingsDialog,
    GridSettingsSpacingChanged(String),
    GridSettingsSubdivisionsChanged(String),
    GridSettingsOpacityChanged(String),
    GridSettingsShowGridChanged(bool),
    GridSettingsSnapChanged(bool),
    ConfirmGridSettings,
    EditorSetTransformSpec(LayerTransformSpec),
    EditorTransformFramesLoaded(Option<EditorTransformFrames>),
    EditorTransformCommit,
    EditorTransformCancel,
    OpenManagePlugins,
    ManagePluginsSearchChanged(String),
    ManagePluginsSelect(String),
    PickPluginInstallFolder,
    PluginInstallFolderPicked(Option<PathBuf>),
    OpenPluginsFolder,
    RunPlugin(String),
    PluginParamChanged(String, PluginParamValue),
    PluginParamTargetChanged(ApplyTarget),
    ConfirmPluginParams,
    BrowserCopyFiles(Vec<String>),
    BrowserPasteFiles,
    BrowserPasteResolved(Option<BrowserClipboardPaste>),
    EditorCopyPixels {
        merged: bool,
    },
    EditorCutPixels,
    EditorPastePixels {
        into_selection: bool,
    },
    EditorPasteResolved {
        into_selection: bool,
        png_bytes: Option<Vec<u8>>,
        clipboard_empty: bool,
    },
    EditorPasteCompleted(Result<CommandResponse, String>),
    EditorNewFromClipboard,
    EditorNewFromClipboardResolved(Option<Vec<u8>>),
    NewDocClipboardProbed {
        dims: Option<(u32, u32)>,
        png: Option<Vec<u8>>,
    },
    NewDocUseClipboardSize,
    NewDocToggleCreateFromClipboard(bool),
    NewDocCreateFromClipboard,
    SmartPasteSetMode(SmartPasteFitMode),
    SmartPasteSetWidth(f32),
    SmartPasteSetHeight(f32),
    SmartPasteSetOffsetX(f32),
    SmartPasteSetOffsetY(f32),
    SmartPasteApply,
    SmartPasteDismiss,
    EditorPixelsCopiedSyncClipboard(Result<CommandResponse, String>),
    RecordSyncedClipboardPng(Vec<u8>),
    EditorSetRotateAngle(f64),
    EditorApplyRotateAngle,
    EditorSwitchTab(u64),
    EditorRequestCloseTab(u64),
    EditorRequestCloseTabs(Vec<u64>),
    EditorOpenTabContextMenu(u64),
    EditorDuplicateDocument(Option<u64>),
    EditorOpenNewTabDialog,
    EditorNewTabFilesPicked(Vec<PathBuf>),
    OpenInViewerWindow(String),
    OpenBrowserWindow,
    OpenEditorViewerWindow,
    ToggleQuickAccessEditing,
    OpenExportDialog,
    PickExportDialog,
    ExportPathPicked(Option<PathBuf>),
    ExportPathChanged(String),
    ExportQualityChanged(u8),
    ExportMaxEdgeChanged(String),
    ExportMetadataToggled(bool),
    ConfirmExport,
    EditorStartLayerRename(u64, String),
    EditorLayerRenameChanged(String),
    EditorCommitLayerRename,
    ViewerFrameLoaded(Option<CpuImageFrame>),
    ViewerPairFrameLoaded(Option<CpuImageFrame>),
    EditorFrameLoaded(Option<CpuImageFrame>),
    EditorOriginalFrameLoaded(Option<CpuImageFrame>),
    EditorSelectionMaskLoaded(Option<CpuMaskFrame>),
    MarchingAntsTick,
    EditorThumbnailsLoaded(u64, Vec<photon_image::LayerThumbnail>),
    EditorSetSelectOp(SelectionBooleanOp),
    EditorSetSelectFeather(f32),
    EditorToggleSelectAntiAlias,
    EditorSetWandTolerance(u8),
    EditorToggleWandContiguous,
    EditorToggleSelectSampleAll,
    EditorSetQuickSelectRadius(f32),
    EditorSetBrushSmoothingMode(photon_protocol::BrushSmoothingMode),
    EditorSetStabilizerDistance(f32),
    EditorSetShapeFillMode(photon_protocol::ShapeFillMode),
    EditorSetShapeStrokeWidth(f32),
    EditorSetFloodFillTolerance(u8),
    EditorToggleFloodFillContiguous,
    EditorSetGradientKind(photon_protocol::GradientKind),
    EditorSetGradientColorMode(photon_protocol::GradientColorMode),
    TriggerWindowScreenshot(window::Id),
    ScreenshotReady(window::Id, window::Screenshot),
    OpenSettings,
    CloseSettings,
    SelectSettingsTab(SettingsTab),
    SelectPluginSettingsTab(PluginSettingsTab),
    ToggleSettingsSection(&'static str),
    RequestFactoryReset,
    PluginsSearchChanged(String),
    PluginsSelect(String),
    SetTranslateProvider(TranslateProvider),
    TranslateEndpointChanged(String),
    TranslateApiKeyChanged(String),
    TranslateSourceLangChanged(String),
    TranslateTargetLangChanged(String),
    SaveTranslateSettings,
    PickCustomModelFile(String),
    CustomModelFilePicked(String, Option<PathBuf>),
    OpenPluginModels(String),
    OpenTranslateSettings,
    OpenTranslatePanel,
    CopyText(String),
    ViewerTranslate(ViewerMessage),
    EditorAi(EditorMessage),
    EditorVectorText(EditorMessage),
    ExportVectorSvgPathPicked(Option<PathBuf>),
    Noop,
}

/// Decide whether the initial window should start fullscreen and/or maximized.
///
/// Standalone Viewer keeps its own FitImage/Remembered sizing and only goes
/// fullscreen when a file was opened under the `Fullscreen` system mode.
/// Browser/Editor default to maximized per the app launch mode regardless of
/// whether a file was opened (so `photon edit file` from a Viewer maximizes).

pub(super) fn map_batch_dialog_spec<'a>(
    spec: ManagedDialogSpec<'a, DesktopDialogId, BatchDialogAction<DesktopDialogId>>,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    let map_act = |act: BatchDialogAction<DesktopDialogId>| match act {
        BatchDialogAction::Msg(m) => Message::BatchDialog(m),
        BatchDialogAction::Close => Message::CloseDialog,
        BatchDialogAction::DialogEvent(ev) => Message::DialogEvent(ev),
        BatchDialogAction::Noop => Message::Noop,
    };
    let on_ev = spec.on_event;
    ManagedDialogSpec {
        id: spec.id,
        open: spec.open,
        title: spec.title,
        defaults: spec.defaults,
        resizable: spec.resizable,
        draggable: spec.draggable,
        overlay: spec.overlay,
        body: spec.body.map(map_act),
        footer: spec.footer.map(|f| f.map(map_act)),
        close_button: spec.close_button.map(|b| b.map(map_act)),
        style: spec.style,
        on_event: Box::new(move |ev| map_act(on_ev(ev))),
        on_close: map_act(spec.on_close),
        on_noop: map_act(spec.on_noop),
    }
}

pub(super) fn map_browser_message(msg: BrowserMessage) -> Message {
    match msg {
        BrowserMessage::Noop => Message::Noop,
        BrowserMessage::Dispatch(cmd) => Message::Dispatch(cmd),
        BrowserMessage::ContextMenuDispatch(cmd) => Message::ContextMenuDispatch(cmd),
        BrowserMessage::ToggleMenu(kind) => {
            #[allow(unreachable_patterns)]
            let m = match kind {
                BrowserMenuKind::File => ActiveMenu::BrowserFile,
                BrowserMenuKind::View => ActiveMenu::BrowserView,
                BrowserMenuKind::Tools => ActiveMenu::BrowserTools,
                BrowserMenuKind::Plugins => ActiveMenu::BrowserPlugins,
                BrowserMenuKind::Window => ActiveMenu::BrowserWindow,
                BrowserMenuKind::Help => ActiveMenu::BrowserHelp,
                BrowserMenuKind::Sort => ActiveMenu::BrowserSort,
                BrowserMenuKind::WorkspaceSwitcher => ActiveMenu::WorkspaceSwitcher,
                _ => ActiveMenu::BrowserEdit,
            };
            Message::ToggleMenu(m)
        }
        BrowserMessage::ToggleWorkspacePanels => Message::ToggleMenu(ActiveMenu::WorkspacePanels),
        BrowserMessage::ToggleFullscreen => Message::ToggleFullscreen,
        BrowserMessage::OpenSettings => Message::OpenSettings,
        BrowserMessage::OpenShortcutsDialog => Message::OpenShortcutsDialog,
        BrowserMessage::OpenAboutDialog => Message::OpenAboutDialog,
        BrowserMessage::RequestResetLayout => Message::RequestResetLayout,
        BrowserMessage::OpenBatchDialog(k) => Message::OpenBatchDialog(k),
        BrowserMessage::TogglePathEdit(b) => Message::TogglePathEdit(b),
        BrowserMessage::FolderInputChanged(v) => Message::FolderInputChanged(v),
        BrowserMessage::SubmitFolderInput => Message::SubmitFolderInput,
        BrowserMessage::ToggleBrowserFilterBar => Message::ToggleBrowserFilterBar,
        BrowserMessage::BrowserFilterChanged(v) => Message::BrowserFilterChanged(v),
        BrowserMessage::ClearBrowserFilter => Message::ClearBrowserFilter,
        BrowserMessage::ToggleBrowserFilterCaseSensitive => {
            Message::ToggleBrowserFilterCaseSensitive
        }
        BrowserMessage::ToggleBrowserFilterRegex => Message::ToggleBrowserFilterRegex,
        BrowserMessage::ToggleShowHidden => Message::ToggleBrowserShowHidden,
        BrowserMessage::SetBrowserSortField(f) => Message::SetBrowserSortField(f),
        BrowserMessage::SetBrowserSortOrder(o) => Message::SetBrowserSortOrder(o),
        BrowserMessage::SetBrowserThumbSize(s) => Message::SetBrowserThumbSize(s),
        BrowserMessage::SetBrowserViewMode(m) => Message::SetBrowserViewMode(m),
        BrowserMessage::BrowserEntryActivated {
            path,
            is_dir,
            is_image,
        } => Message::BrowserEntryActivated {
            path,
            is_dir,
            is_image,
        },
        BrowserMessage::ToggleFolderExpand(p) => Message::ToggleFolderExpand(p),
        BrowserMessage::OpenBrowserContextMenu { path, x, y } => {
            Message::OpenBrowserContextMenu { path, x, y }
        }
        BrowserMessage::OpenQuickAccessContextMenu { path, x, y } => {
            Message::OpenQuickAccessContextMenu { path, x, y }
        }
        BrowserMessage::ToggleContextPluginsSubmenu => Message::ToggleContextPluginsSubmenu,
        BrowserMessage::OpenContextPluginsSubmenu => Message::OpenContextPluginsSubmenu,
        BrowserMessage::ToggleContextPluginGroup(id) => Message::ToggleContextPluginGroup(id),
        BrowserMessage::OpenContextPluginGroup(id) => Message::OpenContextPluginGroup(id),
        BrowserMessage::BrowserCopyPath(p) => Message::BrowserCopyPath(p),
        BrowserMessage::BrowserStartRename(p) => Message::BrowserStartRename(p),
        BrowserMessage::BrowserCopyTo(ps) => Message::BrowserCopyTo(ps),
        BrowserMessage::BrowserMoveTo(ps) => Message::BrowserMoveTo(ps),
        BrowserMessage::BrowserConfirmDeletePermanently(ps) => {
            Message::BrowserConfirmDeletePermanently(ps)
        }
        BrowserMessage::BrowserCopyFiles(ps) => Message::BrowserCopyFiles(ps),
        BrowserMessage::BrowserPasteFiles => Message::BrowserPasteFiles,
        BrowserMessage::CopyImageToClipboard(p) => Message::CopyImageToClipboard(p),
        BrowserMessage::RevealInFolder(p) => Message::RevealInFolder(p),
        BrowserMessage::OpenManagePlugins => Message::OpenManagePlugins,
        BrowserMessage::ReloadPlugins => Message::Dispatch(Command::PluginRefresh),
        BrowserMessage::RunPlugin(id) => Message::RunPlugin(id),
        BrowserMessage::BeginDragBrowserPanel(p) => Message::BeginDragBrowserPanel(p),
        BrowserMessage::HoverBrowserPanelDrop(p) => Message::HoverBrowserPanelDrop(p),
        BrowserMessage::DropBrowserPanel(p) => Message::DropBrowserPanel(p),
        BrowserMessage::SwapBrowserLeftPanels => Message::SwapBrowserLeftPanels,
        BrowserMessage::ToggleBrowserLeftPanelCollapse(p) => {
            Message::ToggleBrowserLeftPanelCollapse(p)
        }
        BrowserMessage::CloseBrowserLeftPanel(p) => Message::CloseBrowserLeftPanel(p),
        BrowserMessage::ToggleBrowserPanelVisible(p) => Message::ToggleBrowserPanelVisible(p),
        BrowserMessage::BrowserSplitterDrag(w) => Message::BrowserSplitterDrag(w),
        BrowserMessage::BrowserSplitterRelease(w) => Message::BrowserSplitterRelease(w),
        BrowserMessage::BeginDragBrowserRightPanel(p) => Message::BeginDragBrowserRightPanel(p),
        BrowserMessage::HoverBrowserRightPanelDrop(p) => Message::HoverBrowserRightPanelDrop(p),
        BrowserMessage::DropBrowserRightPanel(p) => Message::DropBrowserRightPanel(p),
        BrowserMessage::SwapBrowserRightPanels => Message::SwapBrowserRightPanels,
        BrowserMessage::ToggleBrowserRightPanelCollapse(p) => {
            Message::ToggleBrowserRightPanelCollapse(p)
        }
        BrowserMessage::CloseBrowserRightPanel(p) => Message::CloseBrowserRightPanel(p),
        BrowserMessage::ToggleBrowserRightPanelVisible(p) => {
            Message::ToggleBrowserRightPanelVisible(p)
        }
        BrowserMessage::BrowserRightSplitterDrag(w) => Message::BrowserRightSplitterDrag(w),
        BrowserMessage::BrowserRightSplitterRelease(w) => Message::BrowserRightSplitterRelease(w),
        BrowserMessage::ToggleQuickAccessEditing => Message::ToggleQuickAccessEditing,
        BrowserMessage::OpenInViewerWindow(p) => Message::OpenInViewerWindow(p),
        BrowserMessage::OpenExternally(p) => Message::OpenExternally(p),
        BrowserMessage::OpenWith(p) => Message::OpenWith(p),
        BrowserMessage::EditPath(p) => Message::BrowserEditPath(p),
        BrowserMessage::VisibleRangeChanged { start, end } => {
            Message::BrowserVisibleRangeChanged { start, end }
        }
        BrowserMessage::PickFolderDialog => Message::PickFolderDialog,
    }
}

pub(super) fn map_editor_dialog_spec<'a>(
    spec: DialogSpec<'a, EditorMessage>,
) -> DialogSpec<'a, Message> {
    DialogSpec {
        open: spec.open,
        title: spec.title,
        width: spec.width,
        body: spec.body.map(Message::Editor),
        footer: spec.footer.map(Message::Editor),
        on_close: map_editor_message(spec.on_close),
        on_noop: map_editor_message(spec.on_noop),
    }
}

pub(super) fn map_editor_message(msg: EditorMessage) -> Message {
    match msg {
        EditorMessage::Noop => Message::Noop,
        EditorMessage::Dispatch(cmd) => Message::Dispatch(cmd),
        EditorMessage::ContextMenuDispatch(cmd) => Message::ContextMenuDispatch(cmd),
        EditorMessage::ToggleMenu(kind) => {
            let m = match kind {
                EditorMenuKind::File => ActiveMenu::EditorFile,
                EditorMenuKind::Edit => ActiveMenu::EditorEdit,
                EditorMenuKind::Image => ActiveMenu::EditorImage,
                EditorMenuKind::Layer => ActiveMenu::EditorLayer,
                EditorMenuKind::Vector => ActiveMenu::EditorVector,
                EditorMenuKind::Filter => ActiveMenu::EditorFilter,
                EditorMenuKind::Plugins => ActiveMenu::EditorPlugins,
                EditorMenuKind::Select => ActiveMenu::EditorSelect,
                EditorMenuKind::View => ActiveMenu::EditorView,
                EditorMenuKind::Window => ActiveMenu::EditorWindow,
                EditorMenuKind::Help => ActiveMenu::EditorHelp,
                EditorMenuKind::AddLayer => ActiveMenu::EditorAddLayer,
                EditorMenuKind::WorkspaceSwitcher => ActiveMenu::WorkspaceSwitcher,
            };
            Message::ToggleMenu(m)
        }
        EditorMessage::HoverMenu(kind) => {
            let m = match kind {
                EditorMenuKind::File => ActiveMenu::EditorFile,
                EditorMenuKind::Edit => ActiveMenu::EditorEdit,
                EditorMenuKind::Image => ActiveMenu::EditorImage,
                EditorMenuKind::Layer => ActiveMenu::EditorLayer,
                EditorMenuKind::Vector => ActiveMenu::EditorVector,
                EditorMenuKind::Filter => ActiveMenu::EditorFilter,
                EditorMenuKind::Plugins => ActiveMenu::EditorPlugins,
                EditorMenuKind::Select => ActiveMenu::EditorSelect,
                EditorMenuKind::View => ActiveMenu::EditorView,
                EditorMenuKind::Window => ActiveMenu::EditorWindow,
                EditorMenuKind::Help => ActiveMenu::EditorHelp,
                EditorMenuKind::AddLayer => ActiveMenu::EditorAddLayer,
                EditorMenuKind::WorkspaceSwitcher => ActiveMenu::WorkspaceSwitcher,
            };
            Message::HoverMenu(m)
        }
        EditorMessage::ToggleWorkspacePanels => Message::ToggleMenu(ActiveMenu::WorkspacePanels),
        EditorMessage::RequestEditorLeaveToBrowser => Message::RequestEditorLeaveToBrowser,
        EditorMessage::RequestEditorLeave { close_editor } => {
            Message::RequestEditorLeave { close_editor }
        }
        EditorMessage::OpenBrowserWindow => Message::OpenBrowserWindow,
        EditorMessage::OpenViewerWindow => Message::OpenEditorViewerWindow,
        EditorMessage::Canvas(ev) => Message::Canvas(ev),
        EditorMessage::CloseDialog => Message::CloseDialog,
        EditorMessage::OpenExportDialog => Message::OpenExportDialog,
        EditorMessage::PickExportDialog => Message::PickExportDialog,
        EditorMessage::ConfirmExport => Message::ConfirmExport,
        EditorMessage::ExportPathChanged(v) => Message::ExportPathChanged(v),
        EditorMessage::ExportQualityChanged(v) => Message::ExportQualityChanged(v),
        EditorMessage::ExportMaxEdgeChanged(v) => Message::ExportMaxEdgeChanged(v),
        EditorMessage::ExportMetadataToggled(v) => Message::ExportMetadataToggled(v),
        EditorMessage::OpenResizeDialog => Message::OpenResizeDialog,
        EditorMessage::OpenCanvasSizeDialog => Message::OpenCanvasSizeDialog,
        EditorMessage::SwitchTab(id) => Message::EditorSwitchTab(id),
        EditorMessage::RequestCloseTab(id) => Message::EditorRequestCloseTab(id),
        EditorMessage::RequestCloseTabs(ids) => Message::EditorRequestCloseTabs(ids),
        EditorMessage::OpenTabContextMenu(id) => Message::EditorOpenTabContextMenu(id),
        EditorMessage::DuplicateDocument(tab_id) => Message::EditorDuplicateDocument(tab_id),
        EditorMessage::OpenNewTabDialog => Message::EditorOpenNewTabDialog,
        EditorMessage::SmartPasteSetMode(m) => Message::SmartPasteSetMode(m),
        EditorMessage::SmartPasteSetWidth(w) => Message::SmartPasteSetWidth(w),
        EditorMessage::SmartPasteSetHeight(h) => Message::SmartPasteSetHeight(h),
        EditorMessage::SmartPasteSetOffsetX(x) => Message::SmartPasteSetOffsetX(x),
        EditorMessage::SmartPasteSetOffsetY(y) => Message::SmartPasteSetOffsetY(y),
        EditorMessage::SmartPasteApply => Message::SmartPasteApply,
        EditorMessage::SmartPasteDismiss => Message::SmartPasteDismiss,
        EditorMessage::OpenColorPicker(t) => Message::OpenColorPicker(t),
        EditorMessage::ColorPickerSetRgb(r, g, b) => Message::ColorPickerSetRgb(r, g, b),
        EditorMessage::ColorPickerPickScreen => Message::ColorPickerPickScreen,
        EditorMessage::ColorPickerHexChanged(v) => Message::ColorPickerHexChanged(v),
        EditorMessage::ColorPickerToggleTransparent(v) => Message::ColorPickerToggleTransparent(v),
        EditorMessage::ConfirmColorPicker => Message::ConfirmColorPicker,
        EditorMessage::ResetViewportBackground => Message::ResetViewportBackground,
        EditorMessage::CopyImageToClipboard(p) => Message::CopyImageToClipboard(p),
        EditorMessage::EditorSelectTool(t) => Message::EditorSelectTool(t),
        EditorMessage::ToolGroupPressed(g) => Message::ToolGroupPressed(g),
        EditorMessage::ToolGroupReleased(g) => Message::ToolGroupReleased(g),
        EditorMessage::OpenToolGroupFlyout(g) => Message::OpenToolGroupFlyout(g),
        EditorMessage::CloseToolGroupFlyout => Message::CloseToolGroupFlyout,
        EditorMessage::EditorSetSelectOp(op) => Message::EditorSetSelectOp(op),
        EditorMessage::EditorSetSelectFeather(v) => Message::EditorSetSelectFeather(v),
        EditorMessage::EditorToggleSelectAntiAlias => Message::EditorToggleSelectAntiAlias,
        EditorMessage::EditorSetWandTolerance(v) => Message::EditorSetWandTolerance(v),
        EditorMessage::EditorToggleWandContiguous => Message::EditorToggleWandContiguous,
        EditorMessage::EditorToggleSelectSampleAll => Message::EditorToggleSelectSampleAll,
        EditorMessage::EditorSetQuickSelectRadius(v) => Message::EditorSetQuickSelectRadius(v),
        EditorMessage::EditorSetBrushSmoothingMode(m) => Message::EditorSetBrushSmoothingMode(m),
        EditorMessage::EditorSetStabilizerDistance(d) => Message::EditorSetStabilizerDistance(d),
        EditorMessage::EditorSetShapeFillMode(m) => Message::EditorSetShapeFillMode(m),
        EditorMessage::EditorSetShapeStrokeWidth(w) => Message::EditorSetShapeStrokeWidth(w),
        EditorMessage::EditorSetFloodFillTolerance(t) => Message::EditorSetFloodFillTolerance(t),
        EditorMessage::EditorToggleFloodFillContiguous => Message::EditorToggleFloodFillContiguous,
        EditorMessage::EditorSetFloodFillOpacity(v) => Message::EditorSetFloodFillOpacity(v),
        EditorMessage::EditorToggleFloodFillSampleAll => Message::EditorToggleFloodFillSampleAll,
        EditorMessage::EditorSetGradientKind(k) => Message::EditorSetGradientKind(k),
        EditorMessage::EditorSetGradientColorMode(c) => Message::EditorSetGradientColorMode(c),
        EditorMessage::EditorSetGradientOpacity(v) => Message::EditorSetGradientOpacity(v),
        EditorMessage::EditorToggleGradientReverse => Message::EditorToggleGradientReverse,
        EditorMessage::EditorToggleGradientDither => Message::EditorToggleGradientDither,
        EditorMessage::EditorSetGradientRepeat(r) => Message::EditorSetGradientRepeat(r),
        EditorMessage::EditorSetGradientBlendMode(b) => Message::EditorSetGradientBlendMode(b),
        EditorMessage::EditorToggleGradientInteractive => Message::EditorToggleGradientInteractive,
        EditorMessage::EditorApplyGradientPreset(id) => Message::EditorApplyGradientPreset(id),
        EditorMessage::EditorSelectGradientStop(i) => Message::EditorSelectGradientStop(i),
        EditorMessage::EditorAddGradientStop(p) => Message::EditorAddGradientStop(p),
        EditorMessage::EditorRemoveSelectedGradientStop => {
            Message::EditorRemoveSelectedGradientStop
        }
        EditorMessage::EditorSetGradientStopPos(i, p) => Message::EditorSetGradientStopPos(i, p),
        EditorMessage::EditorToggleGradientStopAlpha(i) => {
            Message::EditorToggleGradientStopAlpha(i)
        }
        EditorMessage::EditorSetGradientStopFg(i) => Message::EditorSetGradientStopFg(i),
        EditorMessage::EditorApplyGradientDraft => Message::EditorApplyGradientDraft,
        EditorMessage::EditorCancelGradientDraft => Message::EditorCancelGradientDraft,
        EditorMessage::EditorSetHealingMode(m) => Message::EditorSetHealingMode(m),
        EditorMessage::EditorSetHealingRadius(r) => Message::EditorSetHealingRadius(r),
        EditorMessage::EditorSetHealingHardness(h) => Message::EditorSetHealingHardness(h),
        EditorMessage::EditorSetHealingStrength(s) => Message::EditorSetHealingStrength(s),
        EditorMessage::EditorToggleHealingSampleAll => Message::EditorToggleHealingSampleAll,
        EditorMessage::EditorSetHealingSource(src) => Message::EditorSetHealingSource(src),
        EditorMessage::EditorSetDenoiseStrength(v) => Message::EditorSetDenoiseStrength(v),
        EditorMessage::EditorSetDenoiseDetail(v) => Message::EditorSetDenoiseDetail(v),
        EditorMessage::EditorSetTransformSpec(spec) => Message::EditorSetTransformSpec(spec),
        EditorMessage::EditorTransformCommit => Message::EditorTransformCommit,
        EditorMessage::EditorTransformCancel => Message::EditorTransformCancel,
        EditorMessage::OpenDenoiseDialog => Message::OpenDenoiseDialog,
        EditorMessage::OpenManagePlugins => Message::OpenManagePlugins,
        EditorMessage::ReloadPlugins => Message::Dispatch(Command::PluginRefresh),
        EditorMessage::RunPlugin(id) => Message::RunPlugin(id),
        EditorMessage::EditorCopyPixels { merged } => Message::EditorCopyPixels { merged },
        EditorMessage::EditorCutPixels => Message::EditorCutPixels,
        EditorMessage::EditorPastePixels { into_selection } => {
            Message::EditorPastePixels { into_selection }
        }
        EditorMessage::ZoomStep(f) => Message::ZoomStep(f),
        EditorMessage::ZoomFit => Message::Canvas(CanvasEvent::ResetFit),
        EditorMessage::ZoomActual => Message::EditorZoomActual,
        EditorMessage::CanvasPanTo { pan_x, pan_y } => Message::CanvasPanTo { pan_x, pan_y },
        EditorMessage::SetAntiAliasingMode(m) => Message::SetAntiAliasingMode(m),
        EditorMessage::SetViewportDenoiseMode(m) => Message::SetViewportDenoiseMode(m),
        EditorMessage::SelectEditorToolTab(t) => Message::SelectEditorToolTab(t),
        EditorMessage::EditorSwapColors => Message::EditorSwapColors,
        EditorMessage::EditorResetColors => Message::EditorResetColors,
        EditorMessage::EditorCycleCompareMode => Message::EditorCycleCompareMode,
        EditorMessage::EditorSetCompareMode(m) => Message::EditorSetCompareMode(m),
        EditorMessage::EditorOpenAdjustPopup(tab) => Message::EditorOpenAdjustPopup(tab),
        EditorMessage::EditorSetBrushRadius(v) => Message::EditorSetBrushRadius(v),
        EditorMessage::EditorSetBrushOpacity(v) => Message::EditorSetBrushOpacity(v),
        EditorMessage::EditorSetBrushHardness(v) => Message::EditorSetBrushHardness(v),
        EditorMessage::EditorSetBrushFlow(v) => Message::EditorSetBrushFlow(v),
        EditorMessage::EditorSetBrushSpacing(v) => Message::EditorSetBrushSpacing(v),
        EditorMessage::EditorSetBrushTip(t) => Message::EditorSetBrushTip(t),
        EditorMessage::EditorSetLiquifyMode(m) => Message::EditorSetLiquifyMode(m),
        EditorMessage::EditorSetLiquifyRadius(v) => Message::EditorSetLiquifyRadius(v),
        EditorMessage::EditorSetLiquifyStrength(v) => Message::EditorSetLiquifyStrength(v),
        EditorMessage::EditorSetRotateAngle(v) => Message::EditorSetRotateAngle(v),
        EditorMessage::EditorApplyRotateAngle => Message::EditorApplyRotateAngle,
        EditorMessage::ToggleCropMode => Message::ToggleCropMode,
        EditorMessage::SetCropPreset(p) => Message::SetCropPreset(p),
        EditorMessage::SetCropField(f, v) => Message::SetCropField(f, v),
        EditorMessage::ApplyCrop => Message::ApplyCrop,
        EditorMessage::CancelCrop => Message::CancelCrop,
        EditorMessage::BeginDragEditorPanel(p) => Message::BeginDragEditorPanel(p),
        EditorMessage::HoverEditorPanelDrop(p) => Message::HoverEditorPanelDrop(p),
        EditorMessage::DropEditorPanel(p) => Message::DropEditorPanel(p),
        EditorMessage::SwapEditorPanelOrder => Message::SwapEditorPanelOrder,
        EditorMessage::ToggleEditorPanelCollapse(p) => Message::ToggleEditorPanelCollapse(p),
        EditorMessage::CloseEditorPanel(p) => Message::CloseEditorPanel(p),
        EditorMessage::CloseEditorToolsPanel => Message::CloseEditorToolsPanel,
        EditorMessage::DockSplitterDrag(k, v) => Message::DockSplitterDrag(k, v),
        EditorMessage::DockSplitterRelease(k, v) => Message::DockSplitterRelease(k, v),
        EditorMessage::OpenLayerContextMenu { layer_id } => {
            Message::OpenLayerContextMenu { layer_id }
        }
        EditorMessage::EditorStartLayerRename(id, name) => {
            Message::EditorStartLayerRename(id, name)
        }
        EditorMessage::EditorLayerRenameChanged(v) => Message::EditorLayerRenameChanged(v),
        EditorMessage::EditorCommitLayerRename => Message::EditorCommitLayerRename,
        EditorMessage::EditorCancelLayerRename => Message::EditorCancelLayerRename,
        EditorMessage::EditorSetApplyTarget(t) => Message::EditorSetApplyTarget(t),
        EditorMessage::CopyText(s) => Message::CopyText(s),
        EditorMessage::OpenPluginModels(id) => Message::OpenPluginModels(id),
        EditorMessage::OpenNewDocumentDialog => Message::OpenNewDocumentDialog,
        EditorMessage::OpenExternally(p) => Message::OpenExternally(p),
        EditorMessage::OpenFileDialog => Message::EditorOpenNewTabDialog,
        other @ (EditorMessage::AiSetRembgModel(_)
        | EditorMessage::AiSetRembgFeather(_)
        | EditorMessage::AiSetRembgRefine(_)
        | EditorMessage::AiSetRembgRefineRadius(_)
        | EditorMessage::AiSetRembgRefineSoftness(_)
        | EditorMessage::AiRunRemoveBackground
        | EditorMessage::AiSetWatermarkTab(_)
        | EditorMessage::AiSetWmDetector(_)
        | EditorMessage::AiSetWmInpainter(_)
        | EditorMessage::AiSetWmConfidence(_)
        | EditorMessage::AiSetWmMaskPadding(_)
        | EditorMessage::AiSetWmMaskBrushRadius(_)
        | EditorMessage::AiClearWmMaskStrokes
        | EditorMessage::AiSetWmRegion(_, _, _, _)
        | EditorMessage::AiSetWmGeminiSize(_)
        | EditorMessage::AiSetWmGeminiBrightness(_)
        | EditorMessage::AiSetWmGeminiOffsetX(_)
        | EditorMessage::AiSetWmGeminiOffsetY(_)
        | EditorMessage::AiToggleWmGeminiForce
        | EditorMessage::AiSetWmRegionFeather(_)
        | EditorMessage::AiRunWatermark
        | EditorMessage::AiSetCaptionModel(_)
        | EditorMessage::AiSetCaptionGeneralThreshold(_)
        | EditorMessage::AiSetCaptionCharThreshold(_)
        | EditorMessage::AiToggleCaptionUnderscores
        | EditorMessage::AiSetCaptionPrefix(_)
        | EditorMessage::AiSetCaptionSuffix(_)
        | EditorMessage::AiSetCaptionExclude(_)
        | EditorMessage::AiSetCaptionDraft(_)
        | EditorMessage::AiRunCaption
        | EditorMessage::AiSaveCaption(_)) => Message::EditorAi(other),
        other @ (EditorMessage::EditorSetVectorMode(_)
        | EditorMessage::EditorSetShapeCornerRadius(_)
        | EditorMessage::EditorSelectVectorNode(_)
        | EditorMessage::EditorSetVectorSimplifyTolerance(_)
        | EditorMessage::EditorSetVectorSmoothTension(_)
        | EditorMessage::EditorUpdateActiveVectorShape(_)
        | EditorMessage::OpenTransformEachDialog
        | EditorMessage::TransformEachSetSpec(_)
        | EditorMessage::ConfirmTransformEach
        | EditorMessage::OpenVectorBlendDialog
        | EditorMessage::VectorBlendSetSpec(_)
        | EditorMessage::ConfirmVectorBlend
        | EditorMessage::OpenExportVectorSvgDialog
        | EditorMessage::PickExportVectorSvgDialog
        | EditorMessage::ExportVectorSvgPathChanged(_)
        | EditorMessage::ConfirmExportVectorSvg
        | EditorMessage::EditorSetTextDraft(_)
        | EditorMessage::EditorAddOrUpdateTextLayer(_)
        | EditorMessage::EditorSetAlignTarget(_)
        | EditorMessage::EditorAlignObjects(_)
        | EditorMessage::EditorDistributeObjects(_)
        | EditorMessage::EditorToggleLayerMultiSelect(_)
        | EditorMessage::EditorLayerRowClicked(_)
        | EditorMessage::EditorBeginDragLayer(_)
        | EditorMessage::EditorDropLayer(_)
        | EditorMessage::ToggleRulers
        | EditorMessage::ToggleGrid
        | EditorMessage::SetGridSpacing(_)
        | EditorMessage::SetGridSettings { .. }
        | EditorMessage::ToggleGuides
        | EditorMessage::ToggleLockGuides
        | EditorMessage::ClearGuides
        | EditorMessage::AddGuide(_, _)
        | EditorMessage::SetPendingGuidePreview(_)
        | EditorMessage::ToggleSnap(_)) => Message::EditorVectorText(other),
    }
}

pub(super) fn map_viewer_message(msg: ViewerMessage) -> Message {
    match msg {
        ViewerMessage::Noop => Message::Noop,
        ViewerMessage::Dispatch(cmd) => Message::Dispatch(cmd),
        ViewerMessage::ContextMenuDispatch(cmd) => Message::ContextMenuDispatch(cmd),
        ViewerMessage::PickImagesDialog => Message::PickImagesDialog,
        ViewerMessage::DualStepSpread(d) => Message::DualStepSpread(d),
        ViewerMessage::ZoomStep(f) => Message::ZoomStep(f),
        ViewerMessage::Canvas(ev) => Message::Canvas(ev),
        ViewerMessage::ViewerCanvas(p, ev) => Message::ViewerCanvas(p, ev),
        ViewerMessage::ViewerZoomActual => Message::ViewerZoomActual,
        ViewerMessage::ViewerRotatePreview(d) => Message::ViewerRotatePreview(d),
        ViewerMessage::ToggleFullscreen => Message::ToggleFullscreen,
        ViewerMessage::SetViewerViewMode(m) => Message::SetViewerViewMode(m),
        ViewerMessage::SetAntiAliasingMode(m) => Message::SetAntiAliasingMode(m),
        ViewerMessage::SetViewportDenoiseMode(m) => Message::SetViewportDenoiseMode(m),
        ViewerMessage::ToggleReaderOptions => Message::ToggleMenu(ActiveMenu::ReaderOptions),
        ViewerMessage::ToggleWorkspaceSwitcher => {
            Message::ToggleMenu(ActiveMenu::WorkspaceSwitcher)
        }
        ViewerMessage::ToggleViewerUtilities => Message::ToggleMenu(ActiveMenu::ViewerUtilities),
        ViewerMessage::ToggleWorkspacePanels => Message::ToggleMenu(ActiveMenu::WorkspacePanels),
        ViewerMessage::OpenViewerContextMenu { path, pos } => {
            Message::OpenViewerContextMenu { path, pos }
        }
        ViewerMessage::CopyImageToClipboard(p) => Message::CopyImageToClipboard(p),
        ViewerMessage::CopyViewerPath => Message::CopyViewerPath,
        ViewerMessage::EditPath(p) => Message::BrowserEditPath(p),
        ViewerMessage::ShowInBrowser(p) => Message::ShowInBrowser(p),
        ViewerMessage::OpenWith(p) => Message::OpenWith(p),
        ViewerMessage::RevealInFolder(p) => Message::RevealInFolder(p),
        ViewerMessage::ToggleFilmstrip => Message::ToggleFilmstrip,
        ViewerMessage::CloseFilmstrip => Message::CloseFilmstrip,
        ViewerMessage::FilmstripScrolled { offset } => Message::FilmstripScrolled { offset },
        ViewerMessage::FilmstripWheelScroll(delta) => Message::FilmstripWheelScroll(delta),
        ViewerMessage::ToggleContextPluginsSubmenu => Message::ToggleContextPluginsSubmenu,
        ViewerMessage::OpenContextPluginsSubmenu => Message::OpenContextPluginsSubmenu,
        ViewerMessage::ToggleContextZoomSubmenu => Message::ToggleContextZoomSubmenu,
        ViewerMessage::OpenContextZoomSubmenu => Message::OpenContextZoomSubmenu,
        ViewerMessage::ToggleContextPluginGroup(id) => Message::ToggleContextPluginGroup(id),
        ViewerMessage::OpenContextPluginGroup(id) => Message::OpenContextPluginGroup(id),
        ViewerMessage::ConfirmDeletePermanently(ps) => Message::BrowserConfirmDeletePermanently(ps),
        ViewerMessage::ToggleViewerInspector => Message::ToggleViewerInspector,
        ViewerMessage::ToggleViewerColorPopover => Message::ToggleViewerColorPopover,
        ViewerMessage::CloseViewerPanel(p) => Message::CloseViewerPanel(p),
        ViewerMessage::ToggleViewerPanelCollapse(p) => Message::ToggleViewerPanelCollapse(p),
        ViewerMessage::BeginDragViewerPanel(p) => Message::BeginDragViewerPanel(p),
        ViewerMessage::HoverViewerPanelDrop(p) => Message::HoverViewerPanelDrop(p),
        ViewerMessage::DropViewerPanel(p) => Message::DropViewerPanel(p),
        ViewerMessage::SwapViewerRightPanels => Message::SwapViewerRightPanels,
        ViewerMessage::ViewerSplitterDrag(w) => Message::ViewerSplitterDrag(w),
        ViewerMessage::ViewerSplitterRelease(w) => Message::ViewerSplitterRelease(w),
        ViewerMessage::ViewerRightSplitDrag(p) => Message::ViewerRightSplitDrag(p),
        ViewerMessage::ViewerRightSplitRelease(p) => Message::ViewerRightSplitRelease(p),
        ViewerMessage::SetViewerAdjExposure(v) => Message::SetViewerAdjExposure(v),
        ViewerMessage::SetViewerAdjGamma(v) => Message::SetViewerAdjGamma(v),
        ViewerMessage::SetViewerAdjBrightness(v) => Message::SetViewerAdjBrightness(v),
        ViewerMessage::SetViewerAdjContrast(v) => Message::SetViewerAdjContrast(v),
        ViewerMessage::SetViewerAdjSaturation(v) => Message::SetViewerAdjSaturation(v),
        ViewerMessage::SetViewerAdjHighlights(v) => Message::SetViewerAdjHighlights(v),
        ViewerMessage::SetViewerAdjShadows(v) => Message::SetViewerAdjShadows(v),
        ViewerMessage::ResetViewerAdj => Message::ResetViewerAdj,
        ViewerMessage::ViewerAdjOpenInEditor => Message::ViewerAdjOpenInEditor,
        ViewerMessage::OpenBrowserWindow => Message::OpenBrowserWindow,
        ViewerMessage::WebtoonScrolled(vp) => Message::WebtoonScrolled(vp),
        ViewerMessage::OpenTranslateSettings => Message::OpenTranslateSettings,
        ViewerMessage::SetTranslateProvider(p) => Message::SetTranslateProvider(p),
        ViewerMessage::OpenPluginSettings => Message::OpenManagePlugins,
        ViewerMessage::RunPlugin(id) => Message::RunPlugin(id),
        ViewerMessage::OpenInNewViewerWindow(p) => Message::OpenInViewerWindow(p),
        other @ (ViewerMessage::ToggleTranslateOverlay
        | ViewerMessage::CloseTranslateOverlay
        | ViewerMessage::SetTranslateMode(_)
        | ViewerMessage::SetTranslateOcrEngine(_)
        | ViewerMessage::SetTranslateSourceLang(_)
        | ViewerMessage::SetTranslateTargetLang(_)
        | ViewerMessage::SwapTranslateLangs
        | ViewerMessage::StartTranslateRegionPick
        | ViewerMessage::ScanTranslatePage
        | ViewerMessage::ToggleTranslateBubbles
        | ViewerMessage::ToggleTranslateBubbleStyle
        | ViewerMessage::ToggleTranslateBlockOriginal(_)
        | ViewerMessage::SetTranslateFontScale(_)
        | ViewerMessage::SelectTranslateBlock(_)
        | ViewerMessage::UpdateTranslateBlockOriginal(_, _)
        | ViewerMessage::UpdateTranslateBlockText(_, _)
        | ViewerMessage::MoveTranslateBlock(_, _, _)
        | ViewerMessage::RetryTranslateBlock(_)
        | ViewerMessage::RemoveTranslateBlock(_)
        | ViewerMessage::ClearTranslateBlocks
        | ViewerMessage::SaveTranslationSidecar
        | ViewerMessage::DeleteTranslationSidecar
        | ViewerMessage::CopyTranslatedText
        | ViewerMessage::BeginDragTranslateBubble(_)
        | ViewerMessage::BeginDragTranslateTopBar
        | ViewerMessage::BeginDragTranslateResultCard
        | ViewerMessage::ResetTranslateTopBarPos
        | ViewerMessage::ResetTranslateResultCardPos
        | ViewerMessage::DismissTranslateResultCard
        | ViewerMessage::NextTranslateBubble
        | ViewerMessage::ToggleTranslateResultCardOriginal(_)
        | ViewerMessage::BeginDragTranslatePageBubble(_, _)
        | ViewerMessage::SelectTranslatePageBlock(_, _)
        | ViewerMessage::ToggleTranslatePageBlockOriginal(_, _)
        | ViewerMessage::RetryTranslatePageBlock(_, _)
        | ViewerMessage::RemoveTranslatePageBlock(_, _)
        | ViewerMessage::BeginWebtoonTranslateRegion { .. }
        | ViewerMessage::WebtoonHoverPage { .. }) => Message::ViewerTranslate(other),
    }
}
