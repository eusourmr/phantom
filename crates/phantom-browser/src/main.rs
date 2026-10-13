//! Native desktop shell for the Phantom browser.
//!
//! Phantom owns its browser chrome, navigation state, tabs, and native page
//! painting. Web content is parsed and transformed by Phantom's own engine.
//!
//! No Chromium, WebKit, Gecko, WebView, or embedded browser renderer is used.

#![forbid(unsafe_code)]
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use lucide_icons::{Icon, LUCIDE_FONT_BYTES};
use phantom_engine::{
    Engine, FetchCredentialsMode, FetchHostRequest, ExternalStylesheetRequest, FormControlId, FormControlKind, FormSubmissionError,
    ImageLoading, ObjectFit, ObjectPosition, PaintAttachment, PaintColor, PaintCommand,
    PaintFontFamily, PaintFontStyle, PaintFontWeight, PaintList, PaintRect, PaintTextRange,
    PageScriptTask, ResourcePriority,
};
use phantom_image::{
    AnimatedImageDecoder, AnimationDecodeLimits, AnimationLoopCount, DecodedAnimation,
    DecodedImage, ImageDecodeLimits, ImageDecoder, ImageMetadata, ImageResourceId,
    RasterImageDecoder, image_is_animated,
};
use phantom_net::{
    BinaryResponse, CookieSiteContext, CorsBodyMethod, CorsRequestClassification, DocumentLoadError, DocumentResponse, HttpUrl,
    NetworkClient, NetworkError, NetworkIsolationKey, TextResponse, classify_cors_request,
};
const APP_NAME: &str = "Phantom";
const APP_VERSION: &str = "0.0.1";
const LUCIDE_FONT_NAME: &str = "phantom-lucide";
const MAX_IMAGES_PER_DOCUMENT: usize = 64;
const MAX_IMAGE_PRELOADS_PER_DOCUMENT: usize = 16;
const MAX_IMAGE_RESOURCE_REQUESTS_PER_DOCUMENT: usize = 64;
const MAX_SITE_ICON_CANDIDATES: usize = 8;
const MAX_SITE_ICON_BODY_BYTES: u64 = 1024 * 1024;
const MAX_IMAGE_BODY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_AUTO_SUBRESOURCE_FETCHES_PER_DOCUMENT: usize = 72;
const MAX_AUTO_SUBRESOURCE_BODY_BYTES_PER_DOCUMENT: u64 = 96 * 1024 * 1024;
const MAX_TAB_RASTER_BYTES: u64 = 256 * 1024 * 1024;
const LAZY_LOAD_MARGIN: f32 = 768.0;
const MAX_RECENTLY_CLOSED_TABS: usize = 10;
const MAX_SCRIPT_FETCHES_PER_DOCUMENT: usize = 16;
const MAX_CONCURRENT_SCRIPT_FETCHES: usize = 8;
const MAX_SCRIPT_FETCH_BODY_BYTES: u64 = 1024 * 1024;
const MAX_EXTERNAL_SCRIPTS_PER_DOCUMENT: usize = 16;
// PHANTOM_2N2_EXTERNAL_STYLESHEETS_I
const MAX_EXTERNAL_STYLESHEETS_PER_DOCUMENT: usize = 32;
// PHANTOM_2N4_CSS_IMPORT_GRAPH_I
const MAX_STYLESHEET_IMPORT_DEPTH: usize = 4;
const MAX_STYLESHEET_IMPORTS_PER_GRAPH: usize = 16;
// Keep the fully expanded author source inside the existing one-MiB CSS admission budget.
const MAX_STYLESHEET_GRAPH_BODY_BYTES: usize = 1024 * 1024;

// PHANTOM_2G9_UI_ALPHA_10_TAB_STRIP_PRESSURE
// PHANTOM_2G10_UI_ALPHA_11_TAB_GEOMETRY
// PHANTOM_2G11_UI_ALPHA_12_TAB_IDENTITY
const PINNED_TAB_WIDTH: f32 = 40.0;
const REGULAR_TAB_MAX_WIDTH: f32 = 184.0;
const REGULAR_TAB_MIN_WIDTH: f32 = 56.0;
const REGULAR_TAB_CLOSE_MIN_WIDTH: f32 = 96.0;
const TAB_OUTER_WIDTH_ESTIMATE: f32 = 12.0;
const TAB_ITEM_GAP: f32 = 4.0;
const NEW_TAB_BUTTON_WIDTH: f32 = 31.0;
const HISTORY_BUTTON_WIDTH: f32 = 31.0;
const WINDOW_CONTROL_WIDTH: f32 = 38.0;
const WINDOW_CONTROLS_GAP: f32 = 2.0;
const WINDOW_CONTROLS_LEFT_GAP: f32 = 8.0;
const WINDOW_CONTROLS_RIGHT_PADDING: f32 = 20.0;
const MIN_DRAG_REGION_WIDTH: f32 = 16.0;
const TOP_CHROME_FIXED_GAP_BUDGET: f32 = 24.0;

const PAGE_LOGO_BYTES: &[u8] = include_bytes!("../assets/branding/phantom-logo.png");

const APP_ICON_BYTES: &[u8] = include_bytes!("../assets/branding/phantom-app-icon-1024.png");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NavigationAction {
    New,
    History(usize),
    Reload,
}

struct PendingNavigation {
    receiver: Receiver<Result<DocumentResponse, DocumentLoadError>>,
    action: NavigationAction,
    generation: u64,
}

// PHANTOM_2D3_NAVIGATION_STATE_MACHINE
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NavigationPhase {
    Empty,
    Fetching,
    Parsing,
    Ready,
    Failed,
}

enum NavigationState {
    Empty,
    Fetching(PendingNavigation),
    Parsing(NavigationAction),
    Ready,
    Failed(DocumentPageError),
}

impl NavigationState {
    fn phase(&self) -> NavigationPhase {
        match self {
            Self::Empty => NavigationPhase::Empty,
            Self::Fetching(_) => NavigationPhase::Fetching,
            Self::Parsing(_) => NavigationPhase::Parsing,
            Self::Ready => NavigationPhase::Ready,
            Self::Failed(_) => NavigationPhase::Failed,
        }
    }

    fn is_loading(&self) -> bool {
        matches!(self, Self::Fetching(_) | Self::Parsing(_))
    }

    fn has_committed_document(&self) -> bool {
        matches!(self, Self::Ready)
    }

    fn loading_action(&self) -> Option<NavigationAction> {
        match self {
            Self::Fetching(pending) => Some(pending.action),
            Self::Parsing(action) => Some(*action),
            Self::Empty | Self::Ready | Self::Failed(_) => None,
        }
    }

    fn error(&self) -> Option<&DocumentPageError> {
        match self {
            Self::Failed(error) => Some(error),
            Self::Empty | Self::Fetching(_) | Self::Parsing(_) | Self::Ready => None,
        }
    }
}

// PHANTOM_2D5_SUBRESOURCE_BUDGET
#[derive(Debug)]
struct SubresourceBudget {
    remaining_fetches: usize,
    remaining_body_bytes: u64,
}

impl SubresourceBudget {
    fn new() -> Self {
        Self {
            remaining_fetches: MAX_AUTO_SUBRESOURCE_FETCHES_PER_DOCUMENT,
            remaining_body_bytes: MAX_AUTO_SUBRESOURCE_BODY_BYTES_PER_DOCUMENT,
        }
    }

    fn reserve(&mut self, max_body_bytes: u64) -> Option<u64> {
        if self.remaining_fetches == 0 || self.remaining_body_bytes == 0 {
            return None;
        }

        self.remaining_fetches = self.remaining_fetches.saturating_sub(1);
        let reserved = self.remaining_body_bytes.min(max_body_bytes.max(1));
        self.remaining_body_bytes = self.remaining_body_bytes.saturating_sub(reserved);
        Some(reserved)
    }

    fn refund_unused(&mut self, reserved: u64, used: u64) {
        let refund = reserved.saturating_sub(used.min(reserved));
        self.remaining_body_bytes = self
            .remaining_body_bytes
            .saturating_add(refund)
            .min(MAX_AUTO_SUBRESOURCE_BODY_BYTES_PER_DOCUMENT);
    }
}

fn reserve_subresource_body(
    budget: &Arc<Mutex<SubresourceBudget>>,
    max_body_bytes: u64,
) -> Option<u64> {
    budget
        .lock()
        .ok()
        .and_then(|mut state| state.reserve(max_body_bytes))
}

fn refund_unused_subresource_body(
    budget: &Arc<Mutex<SubresourceBudget>>,
    reserved: u64,
    used: u64,
) {
    if let Ok(mut state) = budget.lock() {
        state.refund_unused(reserved, used);
    }
}

fn fetch_budgeted_binary(
    network: &NetworkClient,
    budget: &Arc<Mutex<SubresourceBudget>>,
    isolation_key: &NetworkIsolationKey,
    url: &HttpUrl,
    max_body_bytes: u64,
) -> Result<BinaryResponse, String> {
    let Some(reserved) = reserve_subresource_body(budget, max_body_bytes) else {
        return Err("document automatic-resource budget exhausted".to_owned());
    };

    let response = network
        .fetch_bytes_partitioned_with_limit(isolation_key, url, reserved)
        .map_err(|error| error.to_string())?;

    let used = u64::try_from(response.body().len()).unwrap_or(u64::MAX);
    refund_unused_subresource_body(budget, reserved, used);

    Ok(response)
}

struct ImageLoadRequest {
    resources: Vec<ImageResourceId>,
    url: HttpUrl,
    isolation_key: NetworkIsolationKey,
    loading: ImageLoading,
    priority: ResourcePriority,
    top: f32,
    preload_only: bool,
}

enum LoadedRaster {
    Static(DecodedImage),
    Animated(DecodedAnimation),
}

struct LoadedImage {
    metadata: ImageMetadata,
    raster: LoadedRaster,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResourceLoadKind {
    Preload,
    Image,
}

enum LoadedResource {
    Preloaded,
    Image(LoadedImage),
}

#[derive(Clone)]
enum ImageTextureBinding {
    Static(egui::TextureHandle),
    Animated(Arc<AnimatedTexture>),
}

impl ImageTextureBinding {
    fn texture_at(&self, now: Instant) -> Option<&egui::TextureHandle> {
        match self {
            Self::Static(texture) => Some(texture),
            Self::Animated(animation) => animation.texture_at(now),
        }
    }

    fn next_repaint_after(&self, now: Instant) -> Option<Duration> {
        match self {
            Self::Static(_) => None,
            Self::Animated(animation) => animation.next_repaint_after(now),
        }
    }

    fn set_animation_active(&self, active: bool, now: Instant) {
        if let Self::Animated(animation) = self {
            animation.set_active(active, now);
        }
    }

    const fn is_animated(&self) -> bool {
        matches!(self, Self::Animated(_))
    }

    fn same_asset(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Static(left), Self::Static(right)) => left.id() == right.id(),
            (Self::Animated(left), Self::Animated(right)) => Arc::ptr_eq(left, right),
            (Self::Static(_), Self::Animated(_)) | (Self::Animated(_), Self::Static(_)) => false,
        }
    }
}

struct AnimationTextureFrame {
    texture: egui::TextureHandle,
    delay: Duration,
}

struct AnimatedTexture {
    frames: Vec<AnimationTextureFrame>,
    loop_count: AnimationLoopCount,
    cycle_duration: Duration,
    clock: Mutex<AnimationClock>,
}

struct AnimationClock {
    elapsed: Duration,
    last_updated: Instant,
    active: bool,
}

impl AnimatedTexture {
    fn set_active(&self, active: bool, now: Instant) {
        let Ok(mut clock) = self.clock.lock() else {
            return;
        };

        if clock.active {
            clock.elapsed = clock
                .elapsed
                .saturating_add(now.saturating_duration_since(clock.last_updated));
        }
        clock.last_updated = now;
        clock.active = active;
    }

    fn elapsed_at(&self, now: Instant) -> Duration {
        let Ok(clock) = self.clock.lock() else {
            return Duration::ZERO;
        };

        if clock.active {
            clock
                .elapsed
                .saturating_add(now.saturating_duration_since(clock.last_updated))
        } else {
            clock.elapsed
        }
    }

    fn texture_at(&self, now: Instant) -> Option<&egui::TextureHandle> {
        let frame_index = self.frame_index(self.elapsed_at(now))?;
        self.frames.get(frame_index).map(|frame| &frame.texture)
    }

    fn next_repaint_after(&self, now: Instant) -> Option<Duration> {
        let elapsed = self.elapsed_at(now);
        let cycle_millis = duration_millis(self.cycle_duration).max(1);
        let elapsed_millis = duration_millis(elapsed);

        if self.animation_finished(elapsed_millis, cycle_millis) {
            return None;
        }

        let cycle_position = elapsed_millis % cycle_millis;
        let mut boundary = 0_u64;

        for frame in &self.frames {
            boundary = boundary.saturating_add(duration_millis(frame.delay).max(1));
            if cycle_position < boundary {
                return Some(Duration::from_millis(
                    boundary.saturating_sub(cycle_position).max(1),
                ));
            }
        }

        Some(Duration::from_millis(1))
    }

    fn frame_index(&self, elapsed: Duration) -> Option<usize> {
        if self.frames.is_empty() {
            return None;
        }

        let cycle_millis = duration_millis(self.cycle_duration).max(1);
        let elapsed_millis = duration_millis(elapsed);

        if self.animation_finished(elapsed_millis, cycle_millis) {
            return self.frames.len().checked_sub(1);
        }

        let cycle_position = elapsed_millis % cycle_millis;
        let mut boundary = 0_u64;

        for (index, frame) in self.frames.iter().enumerate() {
            boundary = boundary.saturating_add(duration_millis(frame.delay).max(1));
            if cycle_position < boundary {
                return Some(index);
            }
        }

        self.frames.len().checked_sub(1)
    }

    fn animation_finished(&self, elapsed_millis: u64, cycle_millis: u64) -> bool {
        match self.loop_count {
            AnimationLoopCount::Infinite => false,
            AnimationLoopCount::Finite(count) => {
                let cycles = u64::from(count).max(1);
                elapsed_millis >= cycle_millis.saturating_mul(cycles)
            }
        }
    }
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

struct ImageLoadEvent {
    generation: u64,
    resources: Vec<ImageResourceId>,
    cache_key: String,
    kind: ResourceLoadKind,
    result: Result<LoadedResource, String>,
}

struct CachedImage {
    metadata: ImageMetadata,
    binding: ImageTextureBinding,
    raster_bytes: u64,
    last_used: u64,
}

struct PendingImageBatch {
    receiver: Receiver<ImageLoadEvent>,
    remaining: usize,
    total: usize,
    generation: u64,
    cancelled: Arc<AtomicBool>,
}

impl Drop for PendingImageBatch {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

// PHANTOM_2C11_SITE_ICON_LIFECYCLE
// PHANTOM_2C15_SITE_ICON_FALLBACK
struct LoadedSiteIcon {
    source: String,
    decoded: DecodedImage,
}

struct PendingSiteIcon {
    receiver: Receiver<Result<LoadedSiteIcon, String>>,
    generation: u64,
    cancelled: Arc<AtomicBool>,
}
// PHANTOM_2D1_DOCUMENT_ERROR_SURFACE
#[derive(Debug)]
struct DocumentPageError {
    title: String,
    message: String,
}

impl DocumentPageError {
    fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
        }
    }
}
// PHANTOM_2L4_BROWSER_FETCH_BRIDGE_I
struct PendingScriptFetch {
    request_id: u64,
    generation: u64,
    receiver: Receiver<Result<TextResponse, NetworkError>>,
}

// PHANTOM_2L5_EXTERNAL_PAGE_SCRIPTS_I
struct PendingPageScripts {
    generation: u64,
    remaining: VecDeque<PageScriptTask>,
    external_receiver: Option<Receiver<Result<TextResponse, NetworkError>>>,
    external_count: usize,
}

// PHANTOM_2N2_EXTERNAL_STYLESHEETS_I
struct PendingExternalStylesheet {
    node_id: u64,
    receiver: Receiver<StylesheetGraphOutcome>,
}

// PHANTOM_2N4_CSS_IMPORT_GRAPH_I
struct StylesheetGraphOutcome {
    css: Option<String>,
    loaded: usize,
    failed: usize,
}

#[derive(Default)]
struct StylesheetImportBudget {
    imports: usize,
    body_bytes: usize,
}

struct CssImportStatement {
    start: usize,
    end: usize,
    href: Option<String>,
}

struct PendingExternalStylesheets {
    generation: u64,
    remaining: VecDeque<ExternalStylesheetRequest>,
    active: Option<PendingExternalStylesheet>,
    loaded: usize,
    failed: usize,
    status_code: u16,
    body_bytes: usize,
}

struct BrowserTab {
    engine: Engine,
    form_values: BTreeMap<FormControlId, String>,
    form_values_generation: u64,
    address: String,
    title: String,
    pinned: bool,
    status: String,
    navigation: NavigationState,
    pending_images: Option<PendingImageBatch>,
    pending_site_icon: Option<PendingSiteIcon>,
    subresource_budget: Arc<Mutex<SubresourceBudget>>,
    deferred_images: Vec<ImageLoadRequest>,
    image_textures: BTreeMap<ImageResourceId, ImageTextureBinding>,
    site_icon: Option<egui::TextureHandle>,
    visible_images: BTreeSet<ImageResourceId>,
    image_cache: BTreeMap<String, CachedImage>,
    cache_clock: u64,
    loaded_images: usize,
    failed_images: usize,
    preloaded_resources: usize,
    failed_preloads: usize,
    raster_bytes: u64,
    history: Vec<String>,
    history_scroll_offsets: Vec<f32>,
    history_index: Option<usize>,
    current_scroll_y: f32,
    pending_scroll_y: Option<f32>,
    document_generation: u64,
    javascript_last_tick: Option<Instant>,
    javascript_timer_failed: bool,
    pending_script_fetches: Vec<PendingScriptFetch>,
    script_fetch_count: usize,
    pending_page_scripts: Option<PendingPageScripts>,
    pending_external_stylesheets: Option<PendingExternalStylesheets>,
    // PHANTOM_2N5_TEXT_SUBRESOURCE_POLICY_II
    // One bounded per-tab client carries the text cache across document generations.
    // Browser policy still selects the stricter transport contract for each destination.
    text_subresource_network: NetworkClient,
}

impl BrowserTab {
    fn new() -> Self {
        Self {
            engine: Engine::new(),
            form_values: BTreeMap::new(),
            form_values_generation: 0,
            address: String::new(),
            title: "Nova aba".to_owned(),
            pinned: false,
            status: "Nova aba · JavaScript OFF · Telemetria OFF".to_owned(),
            navigation: NavigationState::Empty,
            pending_images: None,
            pending_site_icon: None,
            subresource_budget: Arc::new(Mutex::new(SubresourceBudget::new())),
            deferred_images: Vec::new(),
            image_textures: BTreeMap::new(),
            site_icon: None,
            visible_images: BTreeSet::new(),
            image_cache: BTreeMap::new(),
            cache_clock: 0,
            loaded_images: 0,
            failed_images: 0,
            preloaded_resources: 0,
            failed_preloads: 0,
            raster_bytes: 0,
            history: Vec::new(),
            history_scroll_offsets: Vec::new(),
            history_index: None,
            current_scroll_y: 0.0,
            pending_scroll_y: None,
            document_generation: 0,
            javascript_last_tick: None,
            javascript_timer_failed: false,
            pending_script_fetches: Vec::new(),
            script_fetch_count: 0,
            pending_page_scripts: None,
            pending_external_stylesheets: None,
            text_subresource_network: NetworkClient::with_max_body_bytes(
                MAX_SCRIPT_FETCH_BODY_BYTES,
            ),
        }
    }

    fn can_go_back(&self) -> bool {
        self.history_index.is_some_and(|index| index > 0)
    }

    fn can_go_forward(&self) -> bool {
        self.history_index
            .is_some_and(|index| index.saturating_add(1) < self.history.len())
    }

    fn navigation_phase(&self) -> NavigationPhase {
        self.navigation.phase()
    }

    fn is_loading(&self) -> bool {
        self.navigation.is_loading()
    }

    fn has_committed_document(&self) -> bool {
        self.navigation.has_committed_document()
    }

    fn navigation_error(&self) -> Option<&DocumentPageError> {
        self.navigation.error()
    }

    fn loading_action(&self) -> Option<NavigationAction> {
        self.navigation.loading_action()
    }

    fn begin_fetching(&mut self, pending: PendingNavigation) {
        self.navigation = NavigationState::Fetching(pending);
    }

    fn begin_parsing(&mut self, action: NavigationAction) {
        self.navigation = NavigationState::Parsing(action);
    }

    fn mark_navigation_ready(&mut self) {
        self.navigation = NavigationState::Ready;
    }

    fn fail_navigation(&mut self, error: DocumentPageError) {
        self.navigation = NavigationState::Failed(error);
    }

    fn clear_navigation_state(&mut self) {
        self.navigation = NavigationState::Empty;
        self.javascript_last_tick = None;
        self.javascript_timer_failed = false;
        self.pending_script_fetches.clear();
        self.script_fetch_count = 0;
        self.pending_page_scripts = None;
        self.pending_external_stylesheets = None;
    }

    fn is_loading_images(&self) -> bool {
        self.pending_images.is_some() || self.pending_site_icon.is_some()
    }

    fn animation_repaint_after(&self, now: Instant) -> Option<Duration> {
        self.image_textures
            .iter()
            .filter(|(resource, _)| self.visible_images.contains(resource))
            .filter_map(|(_, binding)| binding.next_repaint_after(now))
            .min()
    }

    fn reset_subresource_budget(&mut self) {
        self.subresource_budget = Arc::new(Mutex::new(SubresourceBudget::new()));
    }

    fn cancel_image_work(&mut self) {
        if let Some(pending) = self.pending_images.take() {
            pending.cancelled.store(true, Ordering::Release);
        }

        if let Some(pending) = self.pending_site_icon.take() {
            pending.cancelled.store(true, Ordering::Release);
        }

        self.deferred_images.clear();
    }
}

#[derive(Clone, Copy)]
enum NavigationUiCommand {
    Go,
    Back,
    Forward,
    Reload,
}

// 2C-10 — recently closed tabs
#[derive(Clone, Debug)]
struct ClosedTabSnapshot {
    address: String,
    title: String,
    pinned: bool,
    history: Vec<String>,
    history_index: Option<usize>,
}

impl ClosedTabSnapshot {
    fn capture(tab: &BrowserTab) -> Option<Self> {
        let address = tab
            .history_index
            .and_then(|index| tab.history.get(index))
            .cloned()
            .unwrap_or_else(|| tab.address.trim().to_owned());

        if address.is_empty() {
            return None;
        }

        Some(Self {
            address,
            title: tab.title.clone(),
            pinned: tab.pinned,
            history: tab.history.clone(),
            history_index: tab.history_index,
        })
    }
}
struct PhantomApp {
    network: NetworkClient,
    tabs: Vec<BrowserTab>,
    active_tab: usize,
    page_logo: Option<egui::TextureHandle>,
    floating_bar_forced: bool,
    focus_address_next_frame: bool,
    window_maximized: bool,
    recently_closed_tabs: Vec<ClosedTabSnapshot>,
}

impl PhantomApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        configure_fonts(&context.egui_ctx);
        context.egui_ctx.set_theme(egui::ThemePreference::System);

        Self {
            network: NetworkClient::new(),
            tabs: vec![BrowserTab::new()],
            active_tab: 0,
            page_logo: load_texture(&context.egui_ctx, "phantom-page-logo", PAGE_LOGO_BYTES),
            floating_bar_forced: true,
            focus_address_next_frame: true,
            window_maximized: false,
            recently_closed_tabs: Vec::new(),
        }
    }

    fn active_tab(&self) -> &BrowserTab {
        &self.tabs[self.active_tab]
    }

    fn active_tab_mut(&mut self) -> &mut BrowserTab {
        &mut self.tabs[self.active_tab]
    }

    fn open_new_tab(&mut self) {
        self.tabs.push(BrowserTab::new());
        self.active_tab = self.tabs.len().saturating_sub(1);
        self.floating_bar_forced = true;
        self.focus_address_next_frame = true;
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }

        self.remember_closed_tab(index);

        if self.tabs.len() == 1 {
            self.tabs[0] = BrowserTab::new();
            self.active_tab = 0;
            self.floating_bar_forced = true;
            self.focus_address_next_frame = true;
            return;
        }

        self.tabs.remove(index);

        if index < self.active_tab {
            self.active_tab = self.active_tab.saturating_sub(1);
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
    }

    fn remember_closed_tab(&mut self, index: usize) {
        let Some(snapshot) = self.tabs.get(index).and_then(ClosedTabSnapshot::capture) else {
            return;
        };

        self.recently_closed_tabs.push(snapshot);
        if self.recently_closed_tabs.len() > MAX_RECENTLY_CLOSED_TABS {
            self.recently_closed_tabs.remove(0);
        }
    }

    fn reopen_last_closed_tab(&mut self) {
        let Some(snapshot) = self.recently_closed_tabs.pop() else {
            return;
        };

        let target = snapshot.address.clone();
        let mut tab = BrowserTab::new();
        tab.address = snapshot.address;
        tab.title = snapshot.title;
        tab.pinned = snapshot.pinned;
        tab.history = snapshot.history;
        tab.history_index = snapshot.history_index;
        if tab.history.is_empty() {
            tab.history.push(target.clone());
            tab.history_index = Some(0);
        }

        let replace_blank = self.tabs.len() == 1
            && !self.tabs[0].has_committed_document()
            && self.tabs[0].address.trim().is_empty()
            && self.tabs[0].history.is_empty();

        let insert_index = if replace_blank {
            self.tabs[0] = tab;
            0
        } else {
            let insert_index = if tab.pinned {
                self.tabs
                    .iter()
                    .position(|candidate| !candidate.pinned)
                    .unwrap_or(self.tabs.len())
            } else {
                self.tabs.len()
            };
            self.tabs.insert(insert_index, tab);
            insert_index
        };

        self.active_tab = insert_index;
        self.floating_bar_forced = false;
        self.focus_address_next_frame = false;

        let network = self.network.clone();
        start_navigation(
            &network,
            &mut self.tabs[insert_index],
            target,
            NavigationAction::Reload,
        );
    }
    fn toggle_tab_pinned(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }

        let was_active = index == self.active_tab;
        let new_pinned = !self.tabs[index].pinned;
        let mut tab = self.tabs.remove(index);

        if index < self.active_tab {
            self.active_tab = self.active_tab.saturating_sub(1);
        }

        tab.pinned = new_pinned;

        let insert_index = if new_pinned {
            self.tabs
                .iter()
                .position(|candidate| !candidate.pinned)
                .unwrap_or(self.tabs.len())
        } else {
            self.tabs
                .iter()
                .rposition(|candidate| candidate.pinned)
                .map_or(0, |position| position.saturating_add(1))
        };

        self.tabs.insert(insert_index, tab);

        if was_active {
            self.active_tab = insert_index;
        } else if insert_index <= self.active_tab {
            self.active_tab = self.active_tab.saturating_add(1);
        }
    }

    fn handle_shortcuts(&mut self, context: &egui::Context) {
        let (focus_location, new_tab, reopen_closed, close_tab, reload) = context.input(|input| {
            let command = input.modifiers.command;
            let shift = input.modifiers.shift;
            (
                command && input.key_pressed(egui::Key::L),
                command && !shift && input.key_pressed(egui::Key::T),
                command && shift && input.key_pressed(egui::Key::T),
                command && input.key_pressed(egui::Key::W),
                command && input.key_pressed(egui::Key::R),
            )
        });

        if new_tab {
            self.open_new_tab();
        }

        if reopen_closed {
            self.reopen_last_closed_tab();
        }

        if close_tab {
            self.close_tab(self.active_tab);
        }

        if reload {
            self.execute_navigation_command(NavigationUiCommand::Reload);
        }

        if focus_location {
            self.floating_bar_forced = true;
            self.focus_address_next_frame = true;
        }

        if context.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.floating_bar_forced = false;
            self.focus_address_next_frame = false;
        }
    }
    fn poll_navigation(&mut self, context: &egui::Context) {
        let network = self.network.clone();

        for tab in &mut self.tabs {
            poll_tab_navigation(tab, &network, context.pixels_per_point());
            poll_pending_external_stylesheets(tab, &network, context.pixels_per_point());
            if tab.pending_external_stylesheets.is_some() {
                context.request_repaint_after(Duration::from_millis(50));
            }
            poll_pending_page_scripts(tab, context.pixels_per_point());
            if tab.pending_page_scripts.is_some() {
                context.request_repaint_after(Duration::from_millis(50));
            }
            pump_page_timers(tab, Instant::now(), context.pixels_per_point());
            poll_page_fetches(tab, context.pixels_per_point());
            if !tab.pending_script_fetches.is_empty() {
                context.request_repaint_after(Duration::from_millis(50));
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && !tab.javascript_timer_failed
                && tab.engine.pending_javascript_timer_count() > 0
            {
                context.request_repaint_after(Duration::from_millis(25));
            }
            poll_tab_images(tab, context);
            poll_tab_site_icon(tab, context);
        }
    }

    fn execute_navigation_command(&mut self, command: NavigationUiCommand) {
        let network = self.network.clone();
        let tab = self.active_tab_mut();

        match command {
            NavigationUiCommand::Go => {
                let target = tab.address.trim().to_owned();

                if !target.is_empty() {
                    start_navigation(&network, tab, target, NavigationAction::New);
                }
            }

            NavigationUiCommand::Back => {
                let Some(current_index) = tab.history_index else {
                    return;
                };

                let Some(target_index) = current_index.checked_sub(1) else {
                    return;
                };

                let Some(target) = tab.history.get(target_index).cloned() else {
                    return;
                };

                start_navigation(
                    &network,
                    tab,
                    target,
                    NavigationAction::History(target_index),
                );
            }

            NavigationUiCommand::Forward => {
                let Some(current_index) = tab.history_index else {
                    return;
                };

                let target_index = current_index.saturating_add(1);

                let Some(target) = tab.history.get(target_index).cloned() else {
                    return;
                };

                start_navigation(
                    &network,
                    tab,
                    target,
                    NavigationAction::History(target_index),
                );
            }

            NavigationUiCommand::Reload => {
                let target = tab
                    .history_index
                    .and_then(|index| tab.history.get(index))
                    .cloned()
                    .unwrap_or_else(|| tab.address.trim().to_owned());

                if !target.is_empty() {
                    start_navigation(&network, tab, target, NavigationAction::Reload);
                }
            }
        }
    }

    // PHANTOM_2F7_UI_ALPHA_1
    // PHANTOM_2F_FINAL_UI_CONSOLIDATED
    // PHANTOM_2G1_UI_ALPHA_2
    // PHANTOM_2G2_UI_ALPHA_3
    // PHANTOM_2G9_UI_ALPHA_10
    // PHANTOM_2G10_UI_ALPHA_11
    // PHANTOM_2G13_UI_ALPHA_14_TAB_SURFACE
    // PHANTOM_2G14_UI_ALPHA_15_TAB_INTERACTION
    // PHANTOM_2G15_UI_ALPHA_16_TAB_SURFACE
    fn top_chrome(&mut self, ui: &mut egui::Ui) {
        let context = ui.ctx().clone();
        let mut activate_tab = None;
        let mut close_tab = None;
        let mut toggle_pin_tab = None;
        let mut reload_tab = None;
        let mut create_tab = false;
        let mut reopen_closed_tab = false;
        let mut minimize = false;
        let mut toggle_maximize = false;
        let mut close_window = false;
        let mut start_drag = false;
        let panel_fill = ui.visuals().panel_fill;

        egui::Panel::top("phantom-top-chrome")
            .exact_size(48.0)
            .frame(
                egui::Frame::new()
                    .fill(panel_fill)
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| {
                let pinned_tab_count = self.tabs.iter().filter(|tab| tab.pinned).count();
                let regular_tab_count = self.tabs.len().saturating_sub(pinned_tab_count);
                let regular_tab_width = regular_tab_width_for(
                    ui.available_width(),
                    pinned_tab_count,
                    regular_tab_count,
                    !self.recently_closed_tabs.is_empty(),
                );

                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = TAB_ITEM_GAP;

                    for (index, tab) in self.tabs.iter().enumerate() {
                        let is_active = index == self.active_tab;
                        let title = tab_title(tab);
                        // PHANTOM_2G7_PINNED_TABS_I
                        let width = if tab.pinned {
                            PINNED_TAB_WIDTH
                        } else {
                            regular_tab_width
                        };
                        // Alpha 16 removes the remaining filled pill treatment.
                        // Active state is carried by typography + the bottom indicator.
                        let fill = tab_surface_fill(panel_fill);

                        let tab_frame = egui::Frame::new()
                            .fill(fill)
                            .stroke(egui::Stroke::new(0.0, egui::Color32::TRANSPARENT))
                            .corner_radius(7)
                            .inner_margin(egui::Margin::symmetric(6, 3))
                            .show(ui, |ui| {
                                ui.set_width(width);

                                ui.horizontal_centered(|ui| {
                                    let show_close =
                                        !tab.pinned && width >= REGULAR_TAB_CLOSE_MIN_WIDTH;
                                    let tab_response = if tab.pinned {
                                        // PHANTOM_2G12_UI_ALPHA_13_TAB_IDENTITY_GEOMETRY
                                        // Pinned and regular tabs now paint the same 16x16
                                        // identity glyph on the same 24px vertical track.
                                        tab_identity_slot(
                                            ui,
                                            tab.site_icon.as_ref(),
                                            egui::vec2(28.0, 24.0),
                                            egui::Sense::click(),
                                        )
                                    } else {
                                        // PHANTOM_2G11_UI_ALPHA_12_TAB_IDENTITY
                                        // Keep a permanent icon slot so the title never jumps
                                        // horizontally when the asynchronous favicon arrives.
                                        let _ = tab_identity_slot(
                                            ui,
                                            tab.site_icon.as_ref(),
                                            egui::vec2(TAB_IDENTITY_ICON_SIZE, 24.0),
                                            egui::Sense::hover(),
                                        );

                                        let title_reserve = regular_tab_title_reserve(show_close);
                                        let title_width = (width - title_reserve).max(18.0);
                                        let visible_title = compact_tab_title(&title, title_width);

                                        let title_text = if is_active {
                                            egui::RichText::new(visible_title).size(13.0).strong()
                                        } else {
                                            egui::RichText::new(visible_title).size(13.0)
                                        };

                                        ui.add_sized(
                                            [title_width, 24.0],
                                            egui::Label::new(title_text)
                                                .sense(egui::Sense::click()),
                                        )
                                    };
                                    // Pinned tabs never expose X. Under tab-strip pressure,
                                    // regular tabs keep closing available via Ctrl+W/context menu.
                                    let close_clicked = show_close
                                        && icon_button(ui, Icon::X, true, [25.0, 24.0], 13.0)
                                            .on_hover_text("Fechar aba · Ctrl+W")
                                            .clicked();

                                    (tab_response, close_clicked)
                                })
                                .inner
                            });

                        if is_active {
                            let indicator = active_tab_indicator_rect(tab_frame.response.rect);
                            ui.painter().rect_filled(
                                indicator,
                                1.0,
                                ui.visuals().selection.bg_fill,
                            );
                        }

                        let (tab_response, close_clicked) = tab_frame.inner;
                        let tab_response = tab_response.on_hover_text(if tab.pinned {
                            format!(
                                "{} · aba fixada · fechar com Ctrl+W ou menu de contexto",
                                tab.title
                            )
                        } else {
                            tab.title.clone()
                        });

                        if tab_response.clicked() {
                            activate_tab = Some(index);
                        }

                        tab_response.context_menu(|ui| {
                            let pin_label = if tab.pinned {
                                "Desafixar aba"
                            } else {
                                "Fixar aba"
                            };

                            if ui.button(pin_label).clicked() {
                                toggle_pin_tab = Some(index);
                                ui.close();
                            }

                            if ui.button("Recarregar · Ctrl+R").clicked() {
                                reload_tab = Some(index);
                                ui.close();
                            }

                            ui.separator();

                            if ui.button("Fechar aba · Ctrl+W").clicked() {
                                close_tab = Some(index);
                                ui.close();
                            }
                        });

                        if close_clicked {
                            close_tab = Some(index);
                        }

                        // PHANTOM_2G8_PINNED_TABS_II_GROUP_BOUNDARY
                        let ends_pinned_group = tab.pinned
                            && self
                                .tabs
                                .get(index.saturating_add(1))
                                .is_some_and(|candidate| !candidate.pinned);

                        if ends_pinned_group {
                            ui.separator();
                            ui.add_space(2.0);
                        }
                    }

                    if icon_button(ui, Icon::Plus, true, [31.0, 30.0], 16.0)
                        .on_hover_text("Nova aba · Ctrl+T")
                        .clicked()
                    {
                        create_tab = true;
                    }

                    let controls_width = WINDOW_CONTROLS_LEFT_GAP
                        + WINDOW_CONTROL_WIDTH * 3.0
                        + WINDOW_CONTROLS_GAP * 2.0
                        + WINDOW_CONTROLS_RIGHT_PADDING;
                    if !self.recently_closed_tabs.is_empty()
                        && icon_button(ui, Icon::History, true, [31.0, 30.0], 15.0)
                            .on_hover_text("Reabrir última aba fechada · Ctrl+Shift+T")
                            .clicked()
                    {
                        reopen_closed_tab = true;
                    }
                    let drag_width =
                        (ui.available_width() - controls_width).max(MIN_DRAG_REGION_WIDTH);
                    let (_, drag_response) = ui.allocate_exact_size(
                        egui::vec2(drag_width, 30.0),
                        egui::Sense::click_and_drag(),
                    );

                    if drag_response.drag_started() {
                        start_drag = true;
                    }

                    if drag_response.double_clicked() {
                        toggle_maximize = true;
                    }

                    ui.add_space(WINDOW_CONTROLS_LEFT_GAP);

                    if icon_button(ui, Icon::Minus, true, [WINDOW_CONTROL_WIDTH, 30.0], 15.0)
                        .on_hover_text("Minimizar")
                        .clicked()
                    {
                        minimize = true;
                    }

                    ui.add_space(WINDOW_CONTROLS_GAP);

                    if icon_button(
                        ui,
                        Icon::Maximize2,
                        true,
                        [WINDOW_CONTROL_WIDTH, 30.0],
                        13.5,
                    )
                    .on_hover_text("Maximizar / restaurar")
                    .clicked()
                    {
                        toggle_maximize = true;
                    }

                    ui.add_space(WINDOW_CONTROLS_GAP);

                    if icon_button(ui, Icon::X, true, [WINDOW_CONTROL_WIDTH, 30.0], 15.0)
                        .on_hover_text("Fechar Phantom")
                        .clicked()
                    {
                        close_window = true;
                    }

                    // 2C-9 FIX 5 — native-window inset
                    ui.add_space(WINDOW_CONTROLS_RIGHT_PADDING);
                });
            });

        if let Some(index) = activate_tab
            && index < self.tabs.len()
        {
            self.active_tab = index;
            self.floating_bar_forced = false;
            self.focus_address_next_frame = false;
        }

        if let Some(index) = toggle_pin_tab {
            self.toggle_tab_pinned(index);
        }

        if let Some(index) = reload_tab
            && index < self.tabs.len()
        {
            self.active_tab = index;
            self.execute_navigation_command(NavigationUiCommand::Reload);
        }

        if let Some(index) = close_tab {
            self.close_tab(index);
        }

        if create_tab {
            self.open_new_tab();
        }

        if reopen_closed_tab {
            self.reopen_last_closed_tab();
        }

        if start_drag {
            context.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        if minimize {
            context.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }

        if toggle_maximize {
            self.window_maximized = !self.window_maximized;
            context.send_viewport_cmd(egui::ViewportCommand::Maximized(self.window_maximized));
        }

        if close_window {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn page_area(&mut self, ui: &mut egui::Ui) {
        match self.active_tab().navigation_phase() {
            NavigationPhase::Empty => self.empty_page(ui, self.active_tab()),
            NavigationPhase::Fetching | NavigationPhase::Parsing => {
                self.loading_page(ui, self.active_tab());
            }
            NavigationPhase::Ready => self.rendered_page(ui),
            NavigationPhase::Failed => self.document_error_page(ui, self.active_tab()),
        }
    }
    // PHANTOM_2G3_UI_ALPHA_4_MASCOT_INTEGRATION
    // PHANTOM_2G4_UI_ALPHA_5_TYPOGRAPHY
    fn empty_page(&self, ui: &mut egui::Ui, tab: &BrowserTab) {
        ui.add_space(34.0);

        ui.vertical_centered(|ui| {
            if let Some(logo) = &self.page_logo {
                ui.image((logo.id(), egui::vec2(196.0, 196.0)));
                ui.add_space(13.0);
            }

            // PHANTOM_2G6_UI_ALPHA_7_HIERARCHY
            ui.label(egui::RichText::new("Nova aba").size(22.0).strong());
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Motor próprio em Rust · interface nativa")
                    .size(12.5)
                    .weak(),
            );
            ui.add_space(12.0);

            // PHANTOM_2G5_UI_ALPHA_6_STATUS_REFINEMENT
            if !tab.status.trim().is_empty() {
                ui.label(
                    egui::RichText::new(&tab.status)
                        .size(10.5)
                        .monospace()
                        .weak(),
                );
            }
        });
    }

    fn document_error_page(&self, ui: &mut egui::Ui, tab: &BrowserTab) {
        let Some(error) = tab.navigation_error() else {
            return;
        };

        ui.add_space(64.0);

        ui.vertical_centered(|ui| {
            if let Some(logo) = &self.page_logo {
                ui.image((logo.id(), egui::vec2(148.0, 148.0)));
                ui.add_space(18.0);
            }

            ui.heading(&error.title);
            ui.add_space(10.0);

            ui.add_sized(
                [560.0, 56.0],
                egui::Label::new(
                    egui::RichText::new(&error.message)
                        .size(15.0)
                        .color(ui.visuals().weak_text_color()),
                )
                .wrap(),
            );

            if !tab.address.trim().is_empty() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(truncate_text(tab.address.trim(), 90))
                        .small()
                        .weak(),
                );
            }
        });
    }
    fn loading_page(&self, ui: &mut egui::Ui, tab: &BrowserTab) {
        let heading = match tab.navigation_phase() {
            NavigationPhase::Parsing => "Construindo página",
            NavigationPhase::Fetching => match tab.loading_action() {
                Some(NavigationAction::New) => "Abrindo página",
                Some(NavigationAction::History(_)) => "Restaurando histórico",
                Some(NavigationAction::Reload) => "Recarregando página",
                None => "Carregando",
            },
            NavigationPhase::Empty | NavigationPhase::Ready | NavigationPhase::Failed => {
                "Carregando"
            }
        };
        let stage = match tab.navigation_phase() {
            NavigationPhase::Fetching => "Rede · recebendo documento",
            NavigationPhase::Parsing => "Estrutura · HTML → DOM → layout",
            NavigationPhase::Empty | NavigationPhase::Ready | NavigationPhase::Failed => {
                "Preparando navegação"
            }
        };

        ui.add_space(64.0);

        ui.vertical_centered(|ui| {
            if let Some(logo) = &self.page_logo {
                ui.image((logo.id(), egui::vec2(132.0, 132.0)));
                ui.add_space(12.0);
            }

            ui.spinner();
            ui.add_space(10.0);
            ui.heading(heading);
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(9, 5))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(stage).size(11.5).strong());
                });
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(truncate_text(tab.address.as_str(), 88))
                    .small()
                    .weak(),
            );
        });
    }

    // PHANTOM_2C12_LINK_INTERACTION_UX
    // PHANTOM_2C13_BROWSER_INPUTS_I
    fn rendered_page(&mut self, ui: &mut egui::Ui) {
        let active_index = self.active_tab;

        let viewport_width = ui.available_width().max(1.0);
        let viewport_height = ui.available_height().max(1.0);
        {
            let tab = &mut self.tabs[active_index];
            let current_viewport = (
                tab.engine.layout().viewport_width(),
                tab.engine.layout().viewport_height(),
            );
            if layout_viewport_changed(
                current_viewport.0,
                current_viewport.1,
                viewport_width,
                viewport_height,
            ) && let Err(error) = tab
                .engine
                .relayout_with_viewport(viewport_width, viewport_height)
            {
                tab.status = format!("Falha ao ajustar viewport: {error}");
            }
        }

        {
            let tab = &mut self.tabs[active_index];

            if tab.form_values_generation != tab.document_generation {
                tab.form_values.clear();
                tab.form_values_generation = tab.document_generation;
            }
        }

        let document_url = self.tabs[active_index]
            .history_index
            .and_then(|index| self.tabs[active_index].history.get(index))
            .cloned()
            .unwrap_or_else(|| self.tabs[active_index].address.clone());

        let command_modifier = ui.ctx().input(|input| input.modifiers.command);
        let controls = self.tabs[active_index].engine.form_control_regions();
        let requested_scroll_y = self.tabs[active_index].pending_scroll_y.take();

        {
            let tab = &mut self.tabs[active_index];

            for control in &controls {
                if matches!(
                    control.kind(),
                    FormControlKind::Text | FormControlKind::Search
                ) {
                    tab.form_values
                        .entry(control.id())
                        .or_insert_with(|| control.initial_value().to_owned());
                }
            }
        }

        let (
            visible_range,
            hovered_link,
            link_activation,
            form_activation,
            control_click_activation,
            element_activation,
        ) = {
            let tab = &mut self.tabs[active_index];
            let engine = &tab.engine;
            let paint = engine.paint_list();
            let image_textures = &tab.image_textures;
            let form_values = &mut tab.form_values;

            let document_width = paint.viewport_width().max(1.0);
            let document_height = paint.content_height().max(1.0);
            let mut visible_range = (0.0_f32, ui.available_height().max(1.0));
            let mut hovered_link = None::<String>;
            let mut link_activation = None::<(u64, bool)>;
            let mut form_activation = None;
            let mut control_click_activation = None::<u64>;
            let mut element_activation = None::<u64>;
            let mut pointer_over_control = false;

            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show_viewport(ui, |ui, viewport| {
                    let canvas_width = document_width.max(ui.available_width());
                    let canvas_height = document_height + 72.0;

                    let (canvas_rect, canvas_response) = ui.allocate_exact_size(
                        egui::vec2(canvas_width, canvas_height),
                        egui::Sense::click(),
                    );

                    let horizontal_offset = ((canvas_width - document_width) * 0.5).max(0.0);
                    let origin = canvas_rect.min + egui::vec2(horizontal_offset, 18.0);

                    if let Some(scroll_y) = requested_scroll_y {
                        let scroll_target = egui::Rect::from_min_size(
                            origin + egui::vec2(0.0, scroll_y.max(0.0)),
                            egui::vec2(1.0, 1.0),
                        );
                        ui.scroll_to_rect(scroll_target, Some(egui::Align::TOP));
                    }

                    visible_range = (
                        (viewport.top() - origin.y).max(0.0),
                        (viewport.bottom() - origin.y).max(0.0),
                    );

                    let viewport_offset = egui::vec2(
                        (viewport.left() - origin.x).max(0.0),
                        (viewport.top() - origin.y).max(0.0),
                    );
                    paint_page(ui, origin, viewport_offset, paint, image_textures);

                    for control in &controls {
                        let rect = control.rect();
                        let widget_rect = egui::Rect::from_min_size(
                            origin + egui::vec2(rect.x(), rect.y()),
                            egui::vec2(rect.width().max(1.0), rect.height().max(1.0)),
                        );

                        let response = match control.kind() {
                            FormControlKind::Text | FormControlKind::Search => {
                                let value = form_values
                                    .entry(control.id())
                                    .or_insert_with(|| control.initial_value().to_owned());

                                let mut edit = egui::TextEdit::singleline(value)
                                    .hint_text(control.placeholder());

                                if !control.enabled() {
                                    edit = edit.interactive(false);
                                }

                                ui.place(widget_rect, edit)
                            }

                            FormControlKind::Submit => {
                                ui.place(widget_rect, egui::Button::new(control.label()))
                            }
                        };

                        pointer_over_control |= response.hovered();

                        match control.kind() {
                            FormControlKind::Text | FormControlKind::Search => {
                                if control.enabled() && response.clicked() {
                                    control_click_activation = Some(control.id().as_u64());
                                }
                                if control.enabled()
                                    && response.lost_focus()
                                    && ui.input(|input| input.key_pressed(egui::Key::Enter))
                                {
                                    form_activation = Some((None, control.form(), None));
                                }
                            }

                            FormControlKind::Submit => {
                                if control.enabled() && response.clicked() {
                                    form_activation = Some((
                                        Some(control.id().as_u64()),
                                        control.form(),
                                        Some(control.id()),
                                    ));
                                }
                            }
                        }
                    }

                    if !pointer_over_control
                        && canvas_response.hovered()
                        && let Some(pointer) = ui.ctx().pointer_hover_pos()
                    {
                        let document_x = pointer.x - origin.x;
                        let document_y = pointer.y - origin.y;

                        if let Some(link) = engine.link_at(document_x, document_y) {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);

                            let resolved = HttpUrl::parse(&document_url)
                                .ok()
                                .and_then(|base| base.resolve(link.href()).ok());

                            hovered_link = Some(
                                resolved
                                    .as_ref()
                                    .map(|url| url.as_str().to_owned())
                                    .unwrap_or_else(|| link.href().to_owned()),
                            );

                            if canvas_response.clicked() {
                                link_activation = Some((link.node_id(), command_modifier));
                            }
                        } else if canvas_response.clicked() {
                            element_activation = engine.dom_element_at(document_x, document_y);
                        }
                    }
                });

            (
                visible_range,
                hovered_link,
                link_activation,
                form_activation,
                control_click_activation,
                element_activation,
            )
        };

        if let Some(target) = hovered_link.as_deref() {
            egui::Area::new(egui::Id::new("phantom-link-target-preview"))
                .anchor(egui::Align2::LEFT_BOTTOM, [12.0, -12.0])
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::new()
                        .fill(ui.visuals().panel_fill)
                        .stroke(egui::Stroke::new(
                            1.0,
                            ui.visuals().widgets.noninteractive.bg_stroke.color,
                        ))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::symmetric(8, 5))
                        .show(ui, |ui| {
                            ui.set_max_width(560.0);
                            ui.label(egui::RichText::new(target).size(11.0).weak());
                        });
                });
        }

        self.tabs[active_index].current_scroll_y = visible_range.0;
        let visible_images = visible_image_resources(
            self.tabs[active_index].engine.paint_list(),
            visible_range.0,
            visible_range.1,
        );
        let now = Instant::now();
        let network = self.network.clone();

        {
            let tab = &mut self.tabs[active_index];
            tab.visible_images = visible_images;

            for (resource, binding) in &tab.image_textures {
                binding.set_animation_active(tab.visible_images.contains(resource), now);
            }

            activate_deferred_images(&network, tab, visible_range.1);

            if tab.pending_images.is_some() {
                ui.ctx().request_repaint_after(Duration::from_millis(50));
            }

            if let Some(delay) = tab.animation_repaint_after(now) {
                ui.ctx().request_repaint_after(delay);
            }
        }

        // PHANTOM_2M24_NATIVE_INTERACTION_JAVASCRIPT_EVENTS_I
        if let Some((native_click_node, form, submitter)) = form_activation {
            let default_allowed = native_click_node.is_none_or(|node_id| {
                match dispatch_native_click_before_default_action(
                    &mut self.tabs[active_index],
                    node_id,
                    &document_url,
                    ui.ctx().pixels_per_point(),
                ) {
                    Ok(allowed) => allowed,
                    Err(error) => {
                        self.tabs[active_index].status =
                            format!("Falha no click JavaScript nativo: {error}");
                        false
                    }
                }
            });

            if default_allowed {
                let submission = {
                    let tab = &self.tabs[active_index];
                    tab.engine
                        .build_get_form_submission(form, submitter, &tab.form_values)
                };

                match submission {
                    Ok(submission) => {
                        let target = HttpUrl::parse(&document_url)
                            .and_then(|base| {
                                if submission.action().is_empty() {
                                    Ok(base)
                                } else {
                                    base.resolve(submission.action())
                                }
                            })
                            .map(|url| url.with_query_pairs(submission.fields()));

                        match target {
                            Ok(target) => {
                                let tab = &mut self.tabs[active_index];
                                start_navigation(
                                    &network,
                                    tab,
                                    target.as_str().to_owned(),
                                    NavigationAction::New,
                                );
                            }

                            Err(error) => {
                                self.tabs[active_index].status =
                                    format!("Form action bloqueada · {error}");
                            }
                        }
                    }

                    Err(FormSubmissionError::UnsupportedMethod(method)) => {
                        self.tabs[active_index].status = format!(
                            "Form method ainda não suportado · {}",
                            method.to_ascii_uppercase()
                        );
                    }

                    Err(FormSubmissionError::FormNotFound) => {
                        self.tabs[active_index].status =
                            "Formulário não encontrado no documento ativo".to_owned();
                    }
                }
            }
        } else if let Some((node_id, command_modifier)) = link_activation {
            let default_allowed = match dispatch_native_click_before_default_action(
                &mut self.tabs[active_index],
                node_id,
                &document_url,
                ui.ctx().pixels_per_point(),
            ) {
                Ok(allowed) => allowed,
                Err(error) => {
                    self.tabs[active_index].status =
                        format!("Falha no click JavaScript nativo: {error}");
                    false
                }
            };

            if default_allowed {
                let link_default = self.tabs[active_index]
                    .engine
                    .link_for_node(node_id)
                    .map(|link| (link.href().to_owned(), link.opens_new_context()));

                if let Some((href, opens_new_context)) = link_default {
                    match HttpUrl::parse(&document_url).and_then(|base| base.resolve(&href)) {
                        Ok(target) => {
                            let target = target.as_str().to_owned();
                            if opens_new_context || command_modifier {
                                self.open_new_tab();
                                let tab = self.active_tab_mut();
                                start_navigation(&network, tab, target, NavigationAction::New);
                            } else if active_index < self.tabs.len() {
                                let tab = &mut self.tabs[active_index];
                                start_navigation(&network, tab, target, NavigationAction::New);
                            }
                        }
                        Err(error) => {
                            self.tabs[active_index].status =
                                format!("Link bloqueado · {error}");
                        }
                    }
                }
            }
        } else if let Some(node_id) = control_click_activation.or(element_activation)
            && let Err(error) = dispatch_native_click_before_default_action(
                &mut self.tabs[active_index],
                node_id,
                &document_url,
                ui.ctx().pixels_per_point(),
            )
        {
            self.tabs[active_index].status =
                format!("Falha no click JavaScript nativo: {error}");
        }
    }
    fn floating_navigation(&mut self, context: &egui::Context) {
        let pointer_in_zone = pointer_in_navigation_zone(context);
        let phase = self.active_tab().navigation_phase();
        let visible =
            self.floating_bar_forced || pointer_in_zone || !matches!(phase, NavigationPhase::Ready);

        if !visible {
            return;
        }

        let bar_width = (context.content_rect().width() * 0.72).clamp(500.0, 980.0);
        let mut command = None;
        let mut keep_forced = self.floating_bar_forced;
        let mut focus_consumed = false;

        let active_index = self.active_tab;
        let can_back = self.tabs[active_index].can_go_back();
        let can_forward = self.tabs[active_index].can_go_forward();
        let can_reload = self.tabs[active_index].has_committed_document()
            || !self.tabs[active_index].address.trim().is_empty();
        let loading = self.tabs[active_index].is_loading();
        let origin_identity = navigation_origin_identity(&self.tabs[active_index]);
        let phase_badge = navigation_phase_badge(&self.tabs[active_index]);

        egui::Area::new(egui::Id::new("phantom-floating-navigation"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -14.0])
            .order(egui::Order::Foreground)
            .show(context, |ui| {
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .corner_radius(20)
                    .inner_margin(egui::Margin::symmetric(14, 11))
                    .show(ui, |ui| {
                        ui.set_width(bar_width);

                        ui.horizontal(|ui| {
                            if icon_button(
                                ui,
                                Icon::ArrowLeft,
                                can_back && !loading,
                                [36.0, 36.0],
                                18.0,
                            )
                            .on_hover_text("Voltar")
                            .clicked()
                            {
                                command = Some(NavigationUiCommand::Back);
                                keep_forced = false;
                            }

                            if icon_button(
                                ui,
                                Icon::ArrowRight,
                                can_forward && !loading,
                                [36.0, 36.0],
                                18.0,
                            )
                            .on_hover_text("Avançar")
                            .clicked()
                            {
                                command = Some(NavigationUiCommand::Forward);
                                keep_forced = false;
                            }

                            if icon_button(
                                ui,
                                Icon::RotateCw,
                                can_reload && !loading,
                                [36.0, 36.0],
                                17.0,
                            )
                            .on_hover_text("Recarregar")
                            .clicked()
                            {
                                command = Some(NavigationUiCommand::Reload);
                                keep_forced = false;
                            }

                            // PHANTOM_2D2_ORIGIN_IDENTITY_UX
                            if let Some((secure_transport, tooltip)) = origin_identity.as_ref() {
                                let icon = if *secure_transport {
                                    Icon::Lock
                                } else {
                                    Icon::ShieldAlert
                                };

                                ui.label(lucide_text(icon, 14.0)).on_hover_text(tooltip);
                            }

                            if let Some((label, tooltip)) = phase_badge.as_ref() {
                                let badge = egui::Frame::new()
                                    .fill(ui.visuals().faint_bg_color)
                                    .corner_radius(9)
                                    .inner_margin(egui::Margin::symmetric(7, 4))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(label.as_str()).size(10.5).strong(),
                                        );
                                    });

                                badge.response.on_hover_text(tooltip.as_str());
                            }

                            let address_width = (ui.available_width() - 58.0).max(180.0);

                            let address_response = ui.add_sized(
                                [address_width, 36.0],
                                egui::TextEdit::singleline(&mut self.tabs[active_index].address)
                                    .hint_text("Digite um endereço")
                                    .margin(egui::Margin::symmetric(12, 8)),
                            );

                            if self.focus_address_next_frame {
                                address_response.request_focus();
                                focus_consumed = true;
                                keep_forced = true;
                            }

                            if address_response.gained_focus() {
                                keep_forced = true;
                            }

                            let enter_pressed = address_response.lost_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter));

                            let go_clicked = ui
                                .add_enabled_ui(!loading, |ui| {
                                    ui.add_sized([48.0, 36.0], egui::Button::new("Ir"))
                                        .clicked()
                                })
                                .inner;

                            if enter_pressed || go_clicked {
                                command = Some(NavigationUiCommand::Go);
                                keep_forced = false;
                            }

                            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                                address_response.surrender_focus();
                                keep_forced = false;
                            }
                        });
                    });
            });

        if focus_consumed {
            self.focus_address_next_frame = false;
        }

        self.floating_bar_forced = keep_forced;

        if let Some(command) = command {
            self.execute_navigation_command(command);
        }
    }

    fn update_window_title(&self, context: &egui::Context) {
        let title = if self.active_tab().title == "Nova aba" {
            format!("{APP_NAME} {APP_VERSION}")
        } else {
            format!("{} — {APP_NAME}", self.active_tab().title)
        };

        context.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }
}

impl eframe::App for PhantomApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ui.ctx());
        self.poll_navigation(ui.ctx());
        self.update_window_title(ui.ctx());

        let now = Instant::now();
        for (index, tab) in self.tabs.iter().enumerate() {
            if index != self.active_tab {
                for binding in tab.image_textures.values() {
                    binding.set_animation_active(false, now);
                }
            }
        }

        if self
            .tabs
            .iter()
            .any(|tab| tab.is_loading() || tab.is_loading_images())
        {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }

        if let Some(delay) = self.active_tab().animation_repaint_after(Instant::now()) {
            ui.ctx().request_repaint_after(delay);
        }

        self.top_chrome(ui);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(10),
            )
            .show(ui, |ui| {
                self.page_area(ui);
            });

        self.floating_navigation(ui.ctx());
    }
}

fn navigation_phase_badge(tab: &BrowserTab) -> Option<(String, String)> {
    match tab.navigation_phase() {
        NavigationPhase::Empty => None,
        NavigationPhase::Fetching => {
            Some(("NET".to_owned(), "Rede · recebendo documento".to_owned()))
        }
        NavigationPhase::Parsing => Some((
            "DOM".to_owned(),
            "Estrutura · HTML → DOM → layout".to_owned(),
        )),
        NavigationPhase::Ready => {
            let tooltip = tab.engine.html_build_report().map_or_else(
                || tab.status.clone(),
                |report| {
                    format!(
                        "Phantom HTML nativo · tokenizer → tree builder → DOM · {} referências resolvidas · {} preservadas · {} recuperações · {} erros recuperáveis",
                        report.character_references_resolved,
                        report.character_references_preserved,
                        report.recoveries_total,
                        report.tokenizer_parse_errors,
                    )
                },
            );

            Some((format!("HTML {}", tab.engine.document().len()), tooltip))
        }
        NavigationPhase::Failed => Some(("ERR".to_owned(), tab.status.clone())),
    }
}

fn navigation_origin_identity(tab: &BrowserTab) -> Option<(bool, String)> {
    let source = match tab.navigation_phase() {
        NavigationPhase::Ready => current_history_url(tab)?,
        NavigationPhase::Failed => tab.address.trim(),
        NavigationPhase::Empty | NavigationPhase::Fetching | NavigationPhase::Parsing => {
            return None;
        }
    };

    let url = HttpUrl::parse(source).ok()?;
    let origin = url.origin();
    let secure_transport = origin.is_secure_transport();
    let tooltip = if secure_transport {
        format!("HTTPS · transporte criptografado · {origin}")
    } else {
        format!("HTTP · transporte sem criptografia · {origin}")
    };

    Some((secure_transport, tooltip))
}

fn document_page_error_from_load(error: &DocumentLoadError) -> DocumentPageError {
    match error {
        DocumentLoadError::UnsupportedMediaType(media_type) => DocumentPageError::new(
            "Formato de documento ainda não suportado",
            format!(
                "O servidor respondeu com {media_type}. Nesta fase o Phantom abre documentos HTML e XHTML."
            ),
        ),

        DocumentLoadError::UnidentifiedMediaType => DocumentPageError::new(
            "Não foi possível identificar esta página",
            "A resposta não informou um tipo HTML válido e não pôde ser reconhecida com segurança.",
        ),

        DocumentLoadError::NoContent { status } => DocumentPageError::new(
            "Esta resposta não contém uma página",
            format!("O servidor respondeu com HTTP {status}, sem conteúdo de documento."),
        ),

        DocumentLoadError::PartialContent => DocumentPageError::new(
            "Resposta parcial não suportada",
            "O Phantom não usa HTTP 206 como representação principal de uma página.",
        ),

        DocumentLoadError::Network(network_error) => DocumentPageError::new(
            "Não foi possível abrir esta página",
            network_error.to_string(),
        ),
    }
}
fn navigation_commit_url(response_final_url: &str, requested_url: &str) -> String {
    let Ok(response_url) = HttpUrl::parse(response_final_url) else {
        return response_final_url.to_owned();
    };

    if response_url.fragment().is_some() {
        return response_url.as_str().to_owned();
    }

    let requested_fragment = HttpUrl::parse(requested_url)
        .ok()
        .and_then(|url| url.fragment().map(str::to_owned));

    requested_fragment.map_or_else(
        || response_url.as_str().to_owned(),
        |fragment| {
            response_url
                .with_fragment(Some(&fragment))
                .as_str()
                .to_owned()
        },
    )
}

fn current_history_url(tab: &BrowserTab) -> Option<&str> {
    tab.history_index
        .and_then(|index| tab.history.get(index))
        .map(String::as_str)
}

fn resolve_navigation_url(tab: &BrowserTab, target: &str) -> Option<HttpUrl> {
    let trimmed = target.trim();

    if trimmed.starts_with('#') {
        let current = HttpUrl::parse(current_history_url(tab)?).ok()?;
        current.resolve(trimmed).ok()
    } else {
        HttpUrl::parse(trimmed).ok()
    }
}

fn fragment_scroll_position(tab: &BrowserTab, target: &HttpUrl) -> f32 {
    match target.fragment() {
        None | Some("") => 0.0,
        Some(fragment) => tab
            .engine
            .fragment_target(fragment)
            .map_or(tab.current_scroll_y, |target| target.top()),
    }
}

fn ensure_history_scroll_offsets(tab: &mut BrowserTab) {
    if tab.history_scroll_offsets.len() < tab.history.len() {
        tab.history_scroll_offsets.resize(tab.history.len(), 0.0);
    } else if tab.history_scroll_offsets.len() > tab.history.len() {
        tab.history_scroll_offsets.truncate(tab.history.len());
    }
}

fn save_current_history_scroll(tab: &mut BrowserTab) {
    ensure_history_scroll_offsets(tab);

    let Some(index) = tab.history_index else {
        return;
    };

    if let Some(offset) = tab.history_scroll_offsets.get_mut(index) {
        *offset = tab.current_scroll_y.max(0.0);
    }
}

fn try_same_document_fragment_navigation(
    tab: &mut BrowserTab,
    target: &str,
    action: NavigationAction,
) -> bool {
    if !tab.has_committed_document() || matches!(action, NavigationAction::Reload) {
        return false;
    }

    let Some(current_source) = current_history_url(tab) else {
        return false;
    };
    let Ok(current_url) = HttpUrl::parse(current_source) else {
        return false;
    };
    let Some(target_url) = resolve_navigation_url(tab, target) else {
        return false;
    };

    if !current_url.same_document_except_fragment(&target_url) {
        return false;
    }

    let fragment_state_changed = current_url.fragment() != target_url.fragment();

    if !fragment_state_changed && target_url.fragment().is_none() {
        return false;
    }

    save_current_history_scroll(tab);

    let scroll_y = match action {
        NavigationAction::History(index) => {
            ensure_history_scroll_offsets(tab);
            tab.history_scroll_offsets
                .get(index)
                .copied()
                .unwrap_or_else(|| fragment_scroll_position(tab, &target_url))
        }

        NavigationAction::New => fragment_scroll_position(tab, &target_url),

        NavigationAction::Reload => return false,
    };

    let serialized = target_url.as_str().to_owned();

    match action {
        NavigationAction::New => {
            if current_url.as_str() != serialized {
                if let Some(index) = tab.history_index {
                    let keep = index.saturating_add(1);
                    tab.history.truncate(keep);
                    tab.history_scroll_offsets.truncate(keep);
                } else {
                    tab.history.clear();
                    tab.history_scroll_offsets.clear();
                }

                tab.history.push(serialized.clone());
                tab.history_scroll_offsets.push(scroll_y);
                tab.history_index = tab.history.len().checked_sub(1);
            }
        }

        NavigationAction::History(index) => {
            if index >= tab.history.len() {
                return false;
            }
            tab.history_index = Some(index);
        }

        NavigationAction::Reload => return false,
    }

    tab.address = serialized;
    tab.current_scroll_y = scroll_y.max(0.0);
    tab.pending_scroll_y = Some(tab.current_scroll_y);

    tab.status = match target_url.fragment() {
        Some(fragment) if !fragment.is_empty() => {
            if tab.engine.fragment_target(fragment).is_some() {
                format!("Seção #{fragment} · navegação interna")
            } else {
                format!("Fragmento #{fragment} · alvo não encontrado")
            }
        }
        _ => "Topo do documento · navegação interna".to_owned(),
    };

    true
}

fn prepare_committed_fragment_scroll(
    tab: &mut BrowserTab,
    action: NavigationAction,
    final_url: &str,
) {
    if !matches!(action, NavigationAction::New) {
        return;
    }

    let Ok(url) = HttpUrl::parse(final_url) else {
        return;
    };

    if url.fragment().is_some() {
        let scroll_y = fragment_scroll_position(tab, &url);
        tab.current_scroll_y = scroll_y.max(0.0);
        tab.pending_scroll_y = Some(tab.current_scroll_y);

        ensure_history_scroll_offsets(tab);
        if let Some(index) = tab.history_index
            && let Some(offset) = tab.history_scroll_offsets.get_mut(index)
        {
            *offset = tab.current_scroll_y;
        }
    }
}
fn start_navigation(
    network: &NetworkClient,
    tab: &mut BrowserTab,
    target: String,
    action: NavigationAction,
) {
    // PHANTOM_2C14_NAVIGATION_LIFECYCLE_II
    if try_same_document_fragment_navigation(tab, &target, action) {
        return;
    }

    save_current_history_scroll(tab);

    tab.pending_scroll_y = Some(match action {
        NavigationAction::History(index) => tab
            .history_scroll_offsets
            .get(index)
            .copied()
            .unwrap_or_default(),
        NavigationAction::Reload => tab.current_scroll_y,
        NavigationAction::New => 0.0,
    });

    tab.clear_navigation_state();
    tab.cancel_image_work();
    tab.document_generation = tab.document_generation.saturating_add(1);
    let generation = tab.document_generation;
    tab.address = target.clone();
    tab.status = format!("Carregando {target} …");

    let client = network.clone();
    let (sender, receiver) = mpsc::channel();

    let thread_result = thread::Builder::new()
        .name("phantom-network".to_owned())
        .spawn(move || {
            let result = if matches!(action, NavigationAction::Reload) {
                client.reload_document(&target)
            } else {
                client.fetch_document(&target)
            };
            let _ = sender.send(result);
        });

    match thread_result {
        Ok(_handle) => {
            tab.begin_fetching(PendingNavigation {
                receiver,
                action,
                generation,
            });
        }

        Err(error) => {
            tab.fail_navigation(DocumentPageError::new(
                "Não foi possível iniciar a navegação",
                error.to_string(),
            ));
            tab.status = format!("Não foi possível iniciar a navegação: {error}");
        }
    }
}

// PHANTOM_2M24_NATIVE_INTERACTION_JAVASCRIPT_EVENTS_I
fn dispatch_native_click_before_default_action(
    tab: &mut BrowserTab,
    node_id: u64,
    document_url: &str,
    device_pixel_ratio: f32,
) -> Result<bool, phantom_engine::EngineError> {
    tab.engine
        .dispatch_native_click(node_id, document_url, f64::from(device_pixel_ratio))
}

// PHANTOM_2N5_TEXT_SUBRESOURCE_POLICY_II
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextSubresourcePolicy {
    ClassicScript,
    Stylesheet,
    SameOriginFetch,
}

// PHANTOM_2N6_CORS_FOUNDATION_I
#[derive(Clone, Debug, Eq, PartialEq)]
enum PageFetchCorsDecision {
    SameOrigin,
    CorsRequired { request_origin: String },
}

fn classify_page_fetch_cors(
    document_origin: &HttpUrl,
    target: &HttpUrl,
) -> PageFetchCorsDecision {
    if target.same_origin(document_origin) {
        PageFetchCorsDecision::SameOrigin
    } else {
        PageFetchCorsDecision::CorsRequired {
            request_origin: document_origin.origin().as_str().to_owned(),
        }
    }
}

// PHANTOM_2N10_SIMPLE_REQUEST_MATRIX_I
fn cors_simple_request_denial(method: &str, author_headers: &[(&str, &str)]) -> Option<String> {
    match classify_cors_request(method, author_headers) {
        CorsRequestClassification::Simple => None,
        CorsRequestClassification::PreflightRequired => Some(
            "CORS preflight required; OPTIONS transport is not enabled in 2N-10".to_owned(),
        ),
        CorsRequestClassification::Forbidden => {
            Some("CORS request rejected by forbidden method/header policy".to_owned())
        }
    }
}

fn cors_response_origin_denial(
    request_origin: &str,
    allow_origin: Option<&str>,
) -> Option<String> {
    let admitted = allow_origin
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|value| {
            value == "*"
                || (value != "null"
                    && !value.contains(',')
                    && value.split_ascii_whitespace().count() == 1
                    && value == request_origin)
        });

    if admitted {
        None
    } else {
        Some(format!(
            "CORS response admission blocked for Origin {request_origin}"
        ))
    }
}

fn fetch_text_subresource(
    client: &NetworkClient,
    url: &HttpUrl,
    document_origin: &HttpUrl,
    policy: TextSubresourcePolicy,
) -> Result<TextResponse, NetworkError> {
    let response = match policy {
        TextSubresourcePolicy::ClassicScript | TextSubresourcePolicy::Stylesheet => {
            client.fetch_text_classic_script(url.as_str(), document_origin)?
        }
        TextSubresourcePolicy::SameOriginFetch => {
            if let Some(reason) = cors_simple_request_denial("GET", &[]) {
                return Err(NetworkError::Request(reason));
            }
            client.fetch_text_cors_simple(url.as_str(), document_origin)?
        }
    };

    if policy == TextSubresourcePolicy::SameOriginFetch {
        let final_url = HttpUrl::parse(response.final_url())?;
        if let PageFetchCorsDecision::CorsRequired { request_origin } =
            classify_page_fetch_cors(document_origin, &final_url)
            && let Some(reason) = cors_response_origin_denial(
                &request_origin,
                response.access_control_allow_origin(),
            )
        {
            return Err(NetworkError::Request(reason));
        }
    }

    Ok(response)
}

// PHANTOM_2N17_FETCH_REQUEST_INIT_TRANSPORT_BRIDGE_I
fn fetch_structured_page_request(
    client: &NetworkClient,
    request: &FetchHostRequest,
    url: &HttpUrl,
    document_url: &HttpUrl,
) -> Result<TextResponse, NetworkError> {
    let headers = request.headers().iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    let credentials = request.credentials();
    // Preserve the 2N-5 conditional-cache/revalidation contract for ordinary
    // fetch(url) requests. Header-aware and explicitly credentialed modes use
    // their own network path; no cache bypass for the legacy default GET.
    if request.method() == "GET"
        && headers.is_empty()
        && credentials == FetchCredentialsMode::SameOrigin
    {
        return fetch_text_subresource(
            client, url, document_url, TextSubresourcePolicy::SameOriginFetch,
        );
    }
    if request.method() == "GET" {
        return match credentials {
            FetchCredentialsMode::Omit => client.fetch_text_cors_get_with_headers(
                url.as_str(), document_url, &headers,
            ),
            FetchCredentialsMode::SameOrigin | FetchCredentialsMode::Include => {
                let same_origin = url.same_origin(document_url);
                if credentials == FetchCredentialsMode::SameOrigin && !same_origin {
                    return client.fetch_text_cors_get_with_headers(
                        url.as_str(), document_url, &headers,
                    );
                }
                // Tuple-origin equivalence is enough to prove same-site,
                // but never assume distinct origins are same-site.
                let site_context = if same_origin {
                    CookieSiteContext::SameSite
                } else {
                    CookieSiteContext::CrossSite
                };
                client.fetch_text_cors_credentialed_get_with_headers(
                    url.as_str(), document_url, &headers, site_context,
                )
            }
        };
    }
    // 2N-15 only supports credential-free request bodies. Fail closed.
    if credentials == FetchCredentialsMode::Include
        || (credentials == FetchCredentialsMode::SameOrigin && url.same_origin(document_url))
    {
        return Err(NetworkError::Request(
            "credentialed request bodies are not supported by the 2N-17 transport".to_owned(),
        ));
    }
    let method = match request.method() {
        "POST" => CorsBodyMethod::Post,
        "PUT" => CorsBodyMethod::Put,
        "PATCH" => CorsBodyMethod::Patch,
        _ => return Err(NetworkError::Request("unsupported fetch method".to_owned())),
    };
    client.fetch_text_cors_with_body(
        url.as_str(), document_url, method, &headers,
        request.body().unwrap_or_default().as_bytes(),
    )
}

// PHANTOM_2N2_EXTERNAL_STYLESHEETS_I
fn valid_stylesheet_mime(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| {
        value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .eq_ignore_ascii_case("text/css")
    })
}

fn css_starts_with_ascii_case(source: &str, offset: usize, token: &str) -> bool {
    source
        .get(offset..offset.saturating_add(token.len()))
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(token))
}

fn skip_css_space_and_comments(source: &str, mut offset: usize) -> usize {
    let bytes = source.as_bytes();
    loop {
        while offset < bytes.len() && bytes[offset].is_ascii_whitespace() {
            offset = offset.saturating_add(1);
        }
        if offset.saturating_add(1) < bytes.len()
            && bytes[offset] == b'/'
            && bytes[offset + 1] == b'*'
        {
            let Some(relative_end) = source
                .get(offset + 2..)
                .and_then(|tail| tail.find("*/"))
            else {
                return bytes.len();
            };
            offset = offset.saturating_add(2 + relative_end + 2);
            continue;
        }
        return offset;
    }
}

fn css_statement_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut offset = start;
    let mut quote = None;
    let mut paren_depth = 0_usize;

    while offset < bytes.len() {
        let byte = bytes[offset];
        if let Some(active_quote) = quote {
            if byte == b'\\' {
                offset = offset.saturating_add(2).min(bytes.len());
                continue;
            }
            if byte == active_quote {
                quote = None;
            }
            offset = offset.saturating_add(1);
            continue;
        }

        if offset.saturating_add(1) < bytes.len()
            && byte == b'/'
            && bytes[offset + 1] == b'*'
        {
            let relative_end = source.get(offset + 2..)?.find("*/")?;
            offset = offset.saturating_add(2 + relative_end + 2);
            continue;
        }

        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'(' => paren_depth = paren_depth.saturating_add(1),
            b')' => paren_depth = paren_depth.saturating_sub(1),
            b';' if paren_depth == 0 => return Some(offset.saturating_add(1)),
            _ => {}
        }
        offset = offset.saturating_add(1);
    }
    None
}

fn parse_css_quoted_prefix(input: &str) -> Option<(String, &str)> {
    let quote = input.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let quote_bytes = quote.len_utf8();
    let remainder = input.get(quote_bytes..)?;
    for (index, character) in remainder.char_indices() {
        if character == '\\' {
            // CSS escapes are intentionally deferred in this bounded first slice.
            return None;
        }
        if character == quote {
            let value = remainder.get(..index)?.to_owned();
            let tail = remainder.get(index + character.len_utf8()..)?;
            return Some((value, tail));
        }
    }
    None
}

fn parse_unconditional_css_import_href(statement: &str) -> Option<String> {
    let mut input = statement.trim();
    if !input.get(..7).is_some_and(|value| value.eq_ignore_ascii_case("@import")) {
        return None;
    }
    input = input.get(7..)?.trim_start();
    input = input.strip_suffix(';')?.trim_end();

    let (href, tail) = if matches!(input.chars().next(), Some('\'' | '"')) {
        parse_css_quoted_prefix(input)?
    } else if input
        .get(..4)
        .is_some_and(|value| value.eq_ignore_ascii_case("url("))
    {
        let inner = input.get(4..)?;
        let close = inner.find(')')?;
        let raw_value = inner.get(..close)?.trim();
        let tail = inner.get(close + 1..)?;
        let href = if matches!(raw_value.chars().next(), Some('\'' | '"')) {
            let (value, quoted_tail) = parse_css_quoted_prefix(raw_value)?;
            if !quoted_tail.trim().is_empty() {
                return None;
            }
            value
        } else {
            if raw_value.is_empty()
                || raw_value.chars().any(char::is_whitespace)
                || raw_value.chars().any(|character| matches!(character, '\'' | '"' | '(' | ')' | '\\'))
            {
                return None;
            }
            raw_value.to_owned()
        };
        (href, tail)
    } else {
        return None;
    };

    if !tail.trim().is_empty() || href.trim().is_empty() {
        return None;
    }
    Some(href)
}

fn leading_css_import_statements(source: &str) -> Vec<CssImportStatement> {
    let mut statements = Vec::new();
    let mut offset = 0_usize;

    loop {
        offset = skip_css_space_and_comments(source, offset);
        if offset >= source.len() {
            break;
        }

        if css_starts_with_ascii_case(source, offset, "@charset") {
            let Some(end) = css_statement_end(source, offset) else {
                break;
            };
            offset = end;
            continue;
        }

        if !css_starts_with_ascii_case(source, offset, "@import") {
            break;
        }
        if source
            .as_bytes()
            .get(offset.saturating_add(7))
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-'))
        {
            break;
        }
        let Some(end) = css_statement_end(source, offset) else {
            break;
        };
        let href = source
            .get(offset..end)
            .and_then(parse_unconditional_css_import_href);
        statements.push(CssImportStatement {
            start: offset,
            end,
            href,
        });
        offset = end;
    }

    statements
}

fn fetch_stylesheet_graph(
    client: &NetworkClient,
    url: &HttpUrl,
    document_origin: &HttpUrl,
) -> StylesheetGraphOutcome {
    let mut budget = StylesheetImportBudget::default();
    fetch_stylesheet_graph_node(client, url, document_origin, 0, &mut budget, &[])
}

fn fetch_stylesheet_graph_node(
    client: &NetworkClient,
    url: &HttpUrl,
    document_origin: &HttpUrl,
    depth: usize,
    budget: &mut StylesheetImportBudget,
    path: &[String],
) -> StylesheetGraphOutcome {
    let requested_key = url.as_str().to_owned();
    if path.iter().any(|entry| entry == &requested_key) {
        return StylesheetGraphOutcome {
            css: None,
            loaded: 0,
            failed: 1,
        };
    }

    let response = match fetch_text_subresource(client, url, document_origin, TextSubresourcePolicy::Stylesheet) {
        Ok(response)
            if (200..300).contains(&response.status())
                && valid_stylesheet_mime(response.content_type()) => response,
        Ok(_) | Err(_) => {
            return StylesheetGraphOutcome {
                css: None,
                loaded: 0,
                failed: 1,
            };
        }
    };

    let next_body_bytes = budget.body_bytes.saturating_add(response.body_bytes());
    if next_body_bytes > MAX_STYLESHEET_GRAPH_BODY_BYTES {
        return StylesheetGraphOutcome {
            css: None,
            loaded: 0,
            failed: 1,
        };
    }
    budget.body_bytes = next_body_bytes;

    let Ok(final_url) = HttpUrl::parse(response.final_url()) else {
        return StylesheetGraphOutcome {
            css: None,
            loaded: 0,
            failed: 1,
        };
    };
    let final_key = final_url.as_str().to_owned();
    if path.iter().any(|entry| entry == &final_key) {
        return StylesheetGraphOutcome {
            css: None,
            loaded: 0,
            failed: 1,
        };
    }

    let mut next_path = path.to_vec();
    next_path.push(requested_key.clone());
    if final_key != requested_key {
        next_path.push(final_key);
    }

    let mut css = String::with_capacity(response.body().len());
    let mut cursor = 0_usize;
    let mut loaded = 1_usize;
    let mut failed = 0_usize;

    for statement in leading_css_import_statements(response.body()) {
        if let Some(prefix) = response.body().get(cursor..statement.start) {
            css.push_str(prefix);
        }

        let Some(href) = statement.href else {
            if let Some(original) = response.body().get(statement.start..statement.end) {
                css.push_str(original);
            }
            failed = failed.saturating_add(1);
            cursor = statement.end;
            continue;
        };

        if depth >= MAX_STYLESHEET_IMPORT_DEPTH
            || budget.imports >= MAX_STYLESHEET_IMPORTS_PER_GRAPH
        {
            failed = failed.saturating_add(1);
            css.push('\n');
            cursor = statement.end;
            continue;
        }
        budget.imports = budget.imports.saturating_add(1);

        let Ok(import_url) = final_url.resolve(href.trim()) else {
            failed = failed.saturating_add(1);
            css.push('\n');
            cursor = statement.end;
            continue;
        };
        let imported = fetch_stylesheet_graph_node(
            client,
            &import_url,
            document_origin,
            depth.saturating_add(1),
            budget,
            &next_path,
        );
        loaded = loaded.saturating_add(imported.loaded);
        failed = failed.saturating_add(imported.failed);
        if let Some(imported_css) = imported.css {
            css.push_str(&imported_css);
        }
        css.push('\n');
        cursor = statement.end;
    }

    if let Some(remainder) = response.body().get(cursor..) {
        css.push_str(remainder);
    }

    if css.len() > MAX_STYLESHEET_GRAPH_BODY_BYTES {
        return StylesheetGraphOutcome {
            css: None,
            loaded: 0,
            failed: failed.saturating_add(1),
        };
    }

    StylesheetGraphOutcome {
        css: Some(css),
        loaded,
        failed,
    }
}

fn reset_committed_document_resources(tab: &mut BrowserTab) {
    tab.cancel_image_work();
    tab.image_textures.clear();
    tab.site_icon = None;
    tab.visible_images.clear();
    tab.image_cache.clear();
    tab.cache_clock = 0;
    tab.loaded_images = 0;
    tab.failed_images = 0;
    tab.preloaded_resources = 0;
    tab.failed_preloads = 0;
    tab.raster_bytes = 0;
    tab.reset_subresource_budget();
}

fn finish_committed_document(
    network: &NetworkClient,
    tab: &mut BrowserTab,
    status_code: u16,
    body_bytes: usize,
    stylesheet_loaded: usize,
    stylesheet_failed: usize,
    device_pixel_ratio: f32,
) {
    let final_url = tab.address.clone();
    let script_result = start_page_scripts(tab, &final_url, device_pixel_ratio);
    tab.title = tab
        .engine
        .document_title()
        .unwrap_or_else(|| title_from_url(&final_url));
    tab.mark_navigation_ready();
    tab.javascript_last_tick = Some(Instant::now());

    tab.status = if stylesheet_loaded == 0 && stylesheet_failed == 0 {
        format!(
            "HTTP {status_code} · {body_bytes} bytes · {} nós · {} caixas · {} comandos",
            tab.engine.document().len(),
            tab.engine.layout().len(),
            tab.engine.paint_list().len()
        )
    } else {
        format!(
            "HTTP {status_code} · {body_bytes} bytes · CSS externo {stylesheet_loaded} ok / {stylesheet_failed} falha · {} nós · {} caixas · {} comandos",
            tab.engine.document().len(),
            tab.engine.layout().len(),
            tab.engine.paint_list().len()
        )
    };

    if let Err(error) = script_result {
        // A script failure does not replace a successfully loaded page.
        tab.status = format!("Página carregada · falha de script: {error}");
    }

    start_site_icon_loading(network, tab, &final_url);
    start_image_loading(network, tab, &final_url, device_pixel_ratio);
}

fn start_external_stylesheets(
    tab: &mut BrowserTab,
    status_code: u16,
    body_bytes: usize,
) -> bool {
    let requests = tab.engine.external_stylesheet_requests();
    if requests.is_empty() {
        return false;
    }

    let mut sequence = PendingExternalStylesheets {
        generation: tab.document_generation,
        remaining: requests
            .into_iter()
            .take(MAX_EXTERNAL_STYLESHEETS_PER_DOCUMENT)
            .collect(),
        active: None,
        loaded: 0,
        failed: 0,
        status_code,
        body_bytes,
    };
    advance_external_stylesheet_sequence(tab, &mut sequence);
    tab.pending_external_stylesheets = Some(sequence);
    true
}

fn advance_external_stylesheet_sequence(
    tab: &mut BrowserTab,
    sequence: &mut PendingExternalStylesheets,
) {
    loop {
        if let Some(active) = sequence.active.as_ref() {
            let result = match active.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => StylesheetGraphOutcome {
                    css: None,
                    loaded: 0,
                    failed: 1,
                },
            };
            let node_id = active.node_id;
            sequence.active = None;
            sequence.failed = sequence.failed.saturating_add(result.failed);
            if let Some(css) = result.css {
                sequence.loaded = sequence.loaded.saturating_add(result.loaded.saturating_sub(1));
                match tab.engine.install_external_stylesheet(node_id, &css) {
                    Ok(()) => sequence.loaded = sequence.loaded.saturating_add(1),
                    Err(error) => {
                        sequence.failed = sequence.failed.saturating_add(1);
                        tab.status = format!("Stylesheet externo rejeitado pelo engine: {error}");
                    }
                }
            } else {
                tab.status = "Stylesheet externo ou @import rejeitado por HTTP, MIME ou limite".to_owned();
            }
        }

        let Some(request) = sequence.remaining.pop_front() else {
            return;
        };
        let Ok(base) = HttpUrl::parse(&tab.address) else {
            sequence.failed = sequence.failed.saturating_add(1);
            tab.status = "URL da página inválida para stylesheet externo".to_owned();
            continue;
        };
        let Ok(url) = base.resolve(request.source()) else {
            sequence.failed = sequence.failed.saturating_add(1);
            tab.status = "URL de stylesheet externo inválida".to_owned();
            continue;
        };

        // Reuse Phantom's hardened text-subresource transport. The browser
        // performs stylesheet MIME admission here; no CSS parsing occurs in net.
        // PHANTOM_2N3_STYLESHEET_LIFECYCLE_CACHE_III
        // Reuse one bounded per-tab client so fresh CSS and conditional
        // revalidation survive navigation without widening the 1 MiB budget.
        let client = tab.text_subresource_network.clone();
        let origin = base;
        let node_id = request.node_id();
        let (sender, receiver) = mpsc::channel();
        let spawned = thread::Builder::new()
            .name("phantom-page-stylesheet".to_owned())
            .spawn(move || {
                let outcome = fetch_stylesheet_graph(&client, &url, &origin);
                let _ = sender.send(outcome);
            });
        if spawned.is_ok() {
            sequence.active = Some(PendingExternalStylesheet { node_id, receiver });
            return;
        }
        sequence.failed = sequence.failed.saturating_add(1);
        tab.status = "Não foi possível iniciar o stylesheet externo".to_owned();
    }
}

fn poll_pending_external_stylesheets(
    tab: &mut BrowserTab,
    network: &NetworkClient,
    device_pixel_ratio: f32,
) {
    let Some(mut sequence) = tab.pending_external_stylesheets.take() else {
        return;
    };
    if sequence.generation != tab.document_generation
        || tab.navigation_phase() != NavigationPhase::Parsing
    {
        return;
    }

    advance_external_stylesheet_sequence(tab, &mut sequence);
    if sequence.active.is_some() || !sequence.remaining.is_empty() {
        tab.pending_external_stylesheets = Some(sequence);
        return;
    }

    finish_committed_document(
        network,
        tab,
        sequence.status_code,
        sequence.body_bytes,
        sequence.loaded,
        sequence.failed,
        device_pixel_ratio,
    );
}

// PHANTOM_2L2_BROWSER_PAGE_SCRIPT_EXECUTION_I
fn execute_committed_page_scripts(
    tab: &mut BrowserTab,
    final_url: &str,
    device_pixel_ratio: f32,
) -> Result<usize, phantom_engine::EngineError> {
    tab.engine
        .execute_inline_page_scripts(final_url, f64::from(device_pixel_ratio))
}

// PHANTOM_2L5_EXTERNAL_PAGE_SCRIPTS_I
fn start_page_scripts(
    tab: &mut BrowserTab,
    document_url: &str,
    device_pixel_ratio: f32,
) -> Result<usize, phantom_engine::EngineError> {
    let tasks = tab.engine.page_script_tasks();
    if !tasks.iter().any(|task| matches!(task, PageScriptTask::External(_))) {
        return execute_committed_page_scripts(tab, document_url, device_pixel_ratio);
    }
    let mut sequence = PendingPageScripts {
        generation: tab.document_generation,
        remaining: tasks.into(),
        external_receiver: None,
        external_count: 0,
    };
    let executed = advance_page_script_sequence(tab, &mut sequence, document_url, device_pixel_ratio)?;
    if !sequence.remaining.is_empty() || sequence.external_receiver.is_some() {
        tab.pending_page_scripts = Some(sequence);
    }
    Ok(executed)
}

fn advance_page_script_sequence(
    tab: &mut BrowserTab,
    sequence: &mut PendingPageScripts,
    document_url: &str,
    device_pixel_ratio: f32,
) -> Result<usize, phantom_engine::EngineError> {
    let mut executed = 0;
    loop {
        if let Some(receiver) = sequence.external_receiver.as_ref() {
            let result = match receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return Ok(executed),
                Err(TryRecvError::Disconnected) => {
                    Err(NetworkError::Request("script worker stopped".to_owned()))
                }
            };
            sequence.external_receiver = None;
            match result {
                Ok(response) => {
                    let mime = response.content_type().map(|value| {
                        value.split(';').next().unwrap_or_default().trim()
                    });
                    let valid_mime = mime.is_none_or(|value| {
                        value.eq_ignore_ascii_case("text/javascript")
                            || value.eq_ignore_ascii_case("application/javascript")
                            || value.eq_ignore_ascii_case("application/x-javascript")
                    });
                    if (200..300).contains(&response.status()) && valid_mime {
                        tab.engine.execute_javascript(
                            response.body(), document_url, f64::from(device_pixel_ratio),
                        )?;
                        executed += 1;
                    } else {
                        tab.status = "Script externo rejeitado por HTTP ou MIME".to_owned();
                    }
                }
                Err(error) => tab.status = format!("Script externo indisponível: {error}"),
            }
        }
        let Some(task) = sequence.remaining.pop_front() else {
            return Ok(executed);
        };
        match task {
            PageScriptTask::Inline(source) => {
                tab.engine.execute_javascript(
                    &source, document_url, f64::from(device_pixel_ratio),
                )?;
                executed += 1;
            }
            PageScriptTask::External(source) => {
                if sequence.external_count >= MAX_EXTERNAL_SCRIPTS_PER_DOCUMENT {
                    tab.status = "Limite de scripts externos atingido".to_owned();
                    continue;
                }
                let Ok(base) = HttpUrl::parse(document_url) else {
                    tab.status = "URL da página inválido para script externo".to_owned();
                    continue;
                };
                let Ok(url) = base.resolve(&source) else {
                    tab.status = "URL de script externo inválido".to_owned();
                    continue;
                };
                let origin = base;
                // PHANTOM_2L6_CLASSIC_SCRIPT_CDN_POLICY_I
                let client = tab.text_subresource_network.clone();
                let (sender, receiver) = mpsc::channel();
                let spawned = thread::Builder::new()
                    .name("phantom-page-script".to_owned())
                    .spawn(move || {
                        let result = fetch_text_subresource(
                            &client,
                            &url,
                            &origin,
                            TextSubresourcePolicy::ClassicScript,
                        );
                        let _ = sender.send(result);
                    });
                if spawned.is_ok() {
                    sequence.external_count += 1;
                    sequence.external_receiver = Some(receiver);
                    return Ok(executed);
                }
                tab.status = "Não foi possível iniciar o script externo".to_owned();
            }
        }
    }
}

fn poll_pending_page_scripts(tab: &mut BrowserTab, device_pixel_ratio: f32) {
    let Some(mut sequence) = tab.pending_page_scripts.take() else {
        return;
    };
    if sequence.generation != tab.document_generation
        || tab.navigation_phase() != NavigationPhase::Ready
    {
        return;
    }
    let document_url = tab.address.clone();
    match advance_page_script_sequence(tab, &mut sequence, &document_url, device_pixel_ratio) {
        Ok(executed) => {
            if executed > 0 {
                tab.title = tab.engine.document_title()
                    .unwrap_or_else(|| title_from_url(&document_url));
            }
            if !sequence.remaining.is_empty() || sequence.external_receiver.is_some() {
                tab.pending_page_scripts = Some(sequence);
            }
        }
        Err(error) => tab.status = format!("Falha de script da página: {error}"),
    }
}

fn poll_tab_navigation(tab: &mut BrowserTab, network: &NetworkClient, device_pixel_ratio: f32) {
    let receive_result = match &tab.navigation {
        NavigationState::Fetching(pending) => Some((
            pending.action,
            pending.generation,
            pending.receiver.try_recv(),
        )),
        NavigationState::Empty
        | NavigationState::Parsing(_)
        | NavigationState::Ready
        | NavigationState::Failed(_) => None,
    };

    match receive_result {
        Some((_action, generation, _result)) if generation != tab.document_generation => {
            tab.clear_navigation_state();
        }

        Some((action, _generation, Ok(Ok(response)))) => {
            let status_code = response.status();
            let body_bytes = response.body_bytes();
            let final_url = navigation_commit_url(response.final_http_url().as_str(), &tab.address);

            tab.begin_parsing(action);

            match tab.engine.load_html(response.body()) {
                Ok(()) => {
                    commit_history(tab, action, &final_url);
                    prepare_committed_fragment_scroll(tab, action, &final_url);
                    tab.address = final_url.clone();
                    tab.title = tab
                        .engine
                        .document_title()
                        .unwrap_or_else(|| title_from_url(&final_url));
                    reset_committed_document_resources(tab);

                    // External author stylesheets are admitted before page scripts
                    // so script-observable layout/paint starts from the committed CSS.
                    if start_external_stylesheets(tab, status_code, body_bytes) {
                        tab.status = "Carregando stylesheets externos".to_owned();
                    } else {
                        finish_committed_document(
                            network,
                            tab,
                            status_code,
                            body_bytes,
                            0,
                            0,
                            device_pixel_ratio,
                        );
                    }
                }

                Err(error) => {
                    tab.fail_navigation(DocumentPageError::new(
                        "Não foi possível renderizar esta página",
                        error.to_string(),
                    ));
                    tab.status = format!("Falha de renderização: {error}");
                }
            }
        }

        Some((_action, _generation, Ok(Err(error)))) => {
            tab.fail_navigation(document_page_error_from_load(&error));
            tab.status = format!("Falha de navegação: {error}");
        }

        Some((_action, _generation, Err(TryRecvError::Disconnected))) => {
            tab.fail_navigation(DocumentPageError::new(
                "Não foi possível abrir esta página",
                "O processo de carregamento foi encerrado inesperadamente.",
            ));
            tab.status = "O worker de rede foi encerrado inesperadamente.".to_owned();
        }

        Some((_, _, Err(TryRecvError::Empty))) | None => {}
    }
}

// PHANTOM_2L3_BROWSER_TIMER_PUMP_I
fn pump_page_timers(tab: &mut BrowserTab, now: Instant, device_pixel_ratio: f32) {
    if tab.navigation_phase() != NavigationPhase::Ready || tab.javascript_timer_failed {
        return;
    }
    let Some(previous_tick) = tab.javascript_last_tick.replace(now) else {
        return;
    };
    if tab.engine.pending_javascript_timer_count() == 0 {
        return;
    }
    let elapsed_ms = now.duration_since(previous_tick).as_millis().min(u128::from(u64::MAX)) as u64;
    if let Err(error) = tab.engine.advance_javascript_time(
        elapsed_ms,
        &tab.address,
        f64::from(device_pixel_ratio),
    ) {
        tab.status = format!("Falha no timer da página: {error}");
        tab.javascript_timer_failed = true;
    }
}

// PHANTOM_2L4_BROWSER_FETCH_BRIDGE_I
fn poll_page_fetches(tab: &mut BrowserTab, device_pixel_ratio: f32) {
    if tab.navigation_phase() != NavigationPhase::Ready {
        return;
    }
    let document_url = tab.address.clone();
    let mut waiting = Vec::new();
    for pending in std::mem::take(&mut tab.pending_script_fetches) {
        if pending.generation != tab.document_generation {
            continue;
        }
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                waiting.push(pending);
                continue;
            }
            Err(TryRecvError::Disconnected) => Err(NetworkError::Request("fetch worker stopped".to_owned())),
        };
        let completion = match result {
            Ok(response) => tab.engine.complete_javascript_fetch_success_with_headers(
                pending.request_id,
                response.status(),
                response.final_url(),
                (response.body(), &response.cors_safelisted_response_headers().into_pairs()),
                &document_url,
                f64::from(device_pixel_ratio),
            ),
            Err(error) => tab.engine.complete_javascript_fetch_failure(
                pending.request_id,
                &error.to_string(),
                &document_url,
                f64::from(device_pixel_ratio),
            ),
        };
        if let Err(error) = completion {
            tab.status = format!("Falha no fetch da página: {error}");
        }
    }
    tab.pending_script_fetches = waiting;
    let Ok(base_url) = HttpUrl::parse(&document_url) else {
        return;
    };
    let requests = tab.engine.drain_javascript_fetch_host_requests();
    for request in requests {
        let request_id = request.id();
        let resolved = base_url.resolve(request.url());
        let denied = if tab.script_fetch_count >= MAX_SCRIPT_FETCHES_PER_DOCUMENT
            || tab.pending_script_fetches.len() >= MAX_CONCURRENT_SCRIPT_FETCHES
        {
            Some("fetch budget exceeded".to_owned())
        } else {
            resolved.as_ref().err().map(ToString::to_string)
        };
        if let Some(reason) = denied {
            if let Err(error) = tab.engine.complete_javascript_fetch_failure(
                request_id, &reason, &document_url, f64::from(device_pixel_ratio),
            ) {
                tab.status = format!("Falha no fetch da página: {error}");
            }
            continue;
        }
        let Ok(url) = resolved else { continue };
        let client = tab.text_subresource_network.clone();
        let (sender, receiver) = mpsc::channel();
        let origin = base_url.clone();
        let spawned = thread::Builder::new()
            .name("phantom-script-fetch".to_owned())
            .spawn(move || {
                let result = fetch_structured_page_request(
                    &client, &request, &url, &origin,
                );
                let _ = sender.send(result);
            });
        if spawned.is_ok() {
            tab.script_fetch_count += 1;
            tab.pending_script_fetches.push(PendingScriptFetch {
                request_id,
                generation: tab.document_generation,
                receiver,
            });
        } else if let Err(error) = tab.engine.complete_javascript_fetch_failure(
            request_id, "fetch worker unavailable", &document_url, f64::from(device_pixel_ratio),
        ) {
            tab.status = format!("Falha no fetch da página: {error}");
        }
    }
}

fn start_site_icon_loading(network: &NetworkClient, tab: &mut BrowserTab, document_url: &str) {
    if let Some(pending) = tab.pending_site_icon.take() {
        pending.cancelled.store(true, Ordering::Release);
    }

    let Ok(base_url) = HttpUrl::parse(document_url) else {
        return;
    };

    let candidates = collect_site_icon_candidates(tab, &base_url);

    if candidates.is_empty() {
        return;
    }

    let isolation_key = NetworkIsolationKey::from_top_level(&base_url);
    let generation = tab.document_generation;
    let client = network.clone();
    let budget = Arc::clone(&tab.subresource_budget);
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let (sender, receiver) = mpsc::channel();

    let thread_result = thread::Builder::new()
        .name("phantom-site-icon".to_owned())
        .spawn(move || {
            let result = fetch_site_icon_candidates(
                &client,
                &budget,
                &isolation_key,
                candidates,
                &worker_cancelled,
            );

            if !worker_cancelled.load(Ordering::Acquire) {
                let _ = sender.send(result);
            }
        });

    if thread_result.is_ok() {
        tab.pending_site_icon = Some(PendingSiteIcon {
            receiver,
            generation,
            cancelled,
        });
    }
}

fn collect_site_icon_candidates(tab: &BrowserTab, base_url: &HttpUrl) -> Vec<HttpUrl> {
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();

    let declared_limit = MAX_SITE_ICON_CANDIDATES.saturating_sub(1);

    for request in tab.engine.site_icon_requests() {
        if candidates.len() >= declared_limit {
            break;
        }

        let Ok(url) = base_url.resolve(request.source()) else {
            continue;
        };

        if seen.insert(url.as_str().to_owned()) {
            candidates.push(url);
        }
    }

    if candidates.len() < MAX_SITE_ICON_CANDIDATES
        && let Ok(fallback) = base_url.resolve("/favicon.ico")
        && seen.insert(fallback.as_str().to_owned())
    {
        candidates.push(fallback);
    }

    candidates
}

fn fetch_site_icon_candidates(
    network: &NetworkClient,
    budget: &Arc<Mutex<SubresourceBudget>>,
    isolation_key: &NetworkIsolationKey,
    candidates: Vec<HttpUrl>,
    cancelled: &AtomicBool,
) -> Result<LoadedSiteIcon, String> {
    let mut last_error = "no site icon candidate could be decoded".to_owned();

    for url in candidates {
        if cancelled.load(Ordering::Acquire) {
            return Err("site icon loading cancelled".to_owned());
        }

        match fetch_and_decode_site_icon(network, budget, isolation_key, &url) {
            Ok(decoded) => {
                return Ok(LoadedSiteIcon {
                    source: url.as_str().to_owned(),
                    decoded,
                });
            }

            Err(error) => {
                last_error = format!("{}: {error}", url.as_str());
            }
        }
    }

    Err(last_error)
}

fn fetch_and_decode_site_icon(
    network: &NetworkClient,
    budget: &Arc<Mutex<SubresourceBudget>>,
    isolation_key: &NetworkIsolationKey,
    url: &HttpUrl,
) -> Result<DecodedImage, String> {
    let response = fetch_budgeted_binary(
        network,
        budget,
        isolation_key,
        url,
        MAX_SITE_ICON_BODY_BYTES,
    )?;

    if !(200..=299).contains(&response.status()) {
        return Err(format!(
            "HTTP {} while loading site icon",
            response.status()
        ));
    }

    let decoder = RasterImageDecoder;
    let limits = ImageDecodeLimits::new(512, 512, 262_144, 1_048_576);
    let metadata = decoder
        .probe(response.body(), limits)
        .map_err(|error| error.to_string())?;

    if image_is_animated(response.body(), metadata) {
        let animation = decoder
            .decode_animation(
                response.body(),
                limits,
                AnimationDecodeLimits::new(8, 8 * 1024 * 1024),
            )
            .map_err(|error| error.to_string())?;

        let Some(frame) = animation.frames().first() else {
            return Err("animated site icon has no decodable frame".to_owned());
        };

        return Ok(frame.image().clone());
    }

    decoder
        .decode(response.body(), limits)
        .map_err(|error| error.to_string())
}

fn poll_tab_site_icon(tab: &mut BrowserTab, context: &egui::Context) {
    let receive_result = tab
        .pending_site_icon
        .as_ref()
        .map(|pending| (pending.generation, pending.receiver.try_recv()));

    match receive_result {
        Some((generation, _)) if generation != tab.document_generation => {
            if let Some(pending) = tab.pending_site_icon.take() {
                pending.cancelled.store(true, Ordering::Release);
            }
        }

        Some((generation, Ok(Ok(loaded)))) => {
            tab.pending_site_icon = None;

            if let Some(texture) = decoded_image_texture_named(
                context,
                format!("phantom-site-icon-{generation}-{}", loaded.source),
                &loaded.decoded,
            ) {
                tab.site_icon = Some(texture);
            }
        }

        Some((_, Ok(Err(_)))) => {
            tab.pending_site_icon = None;
        }

        Some((_, Err(TryRecvError::Disconnected))) => {
            tab.pending_site_icon = None;
        }

        Some((_, Err(TryRecvError::Empty))) | None => {}
    }
}
fn start_image_loading(
    network: &NetworkClient,
    tab: &mut BrowserTab,
    document_url: &str,
    device_pixel_ratio: f32,
) {
    tab.cancel_image_work();

    let Ok(base_url) = HttpUrl::parse(document_url) else {
        return;
    };

    let requests = collect_document_image_requests(tab, &base_url, device_pixel_ratio);

    if requests.is_empty() {
        return;
    }

    let (immediate, deferred): (Vec<_>, Vec<_>) = requests.into_iter().partition(|request| {
        request.preload_only
            || request.loading == ImageLoading::Eager
            || request.top <= LAZY_LOAD_MARGIN * 2.0
    });
    tab.deferred_images = deferred;

    if immediate.is_empty() {
        update_image_status(tab);
        return;
    }

    start_image_batch(network, tab, immediate);
}

fn start_image_batch(
    network: &NetworkClient,
    tab: &mut BrowserTab,
    requests: Vec<ImageLoadRequest>,
) {
    if requests.is_empty() || tab.pending_images.is_some() {
        return;
    }

    let total = requests.len();
    let generation = tab.document_generation;
    let client = network.clone();
    let budget = Arc::clone(&tab.subresource_budget);
    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);

    let thread_result = thread::Builder::new()
        .name("phantom-images".to_owned())
        .spawn(move || {
            let decoder = RasterImageDecoder;
            let limits = ImageDecodeLimits::new(8_192, 8_192, 16_777_216, 67_108_864);
            let animation_limits = AnimationDecodeLimits::new(256, 128 * 1024 * 1024);

            for request in requests {
                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }
                let cache_key = request.url.as_str().to_owned();
                let (kind, result) = if request.preload_only {
                    (
                        ResourceLoadKind::Preload,
                        preload_image(&client, &budget, &request.isolation_key, &request.url)
                            .map(|()| LoadedResource::Preloaded),
                    )
                } else {
                    (
                        ResourceLoadKind::Image,
                        fetch_and_decode_image(
                            &client,
                            &budget,
                            &decoder,
                            limits,
                            animation_limits,
                            &request.isolation_key,
                            &request.url,
                        )
                        .map(LoadedResource::Image),
                    )
                };

                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }

                if sender
                    .send(ImageLoadEvent {
                        generation,
                        resources: request.resources,
                        cache_key,
                        kind,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

    match thread_result {
        Ok(_handle) => {
            tab.pending_images = Some(PendingImageBatch {
                receiver,
                remaining: total,
                total,
                generation,
                cancelled,
            });

            tab.status = format!("Página pronta · carregando {total} imagens…");
        }

        Err(error) => {
            tab.failed_images = total;
            tab.status = format!("Página pronta · worker de imagens indisponível: {error}");
        }
    }
}

fn collect_document_image_requests(
    tab: &mut BrowserTab,
    base_url: &HttpUrl,
    device_pixel_ratio: f32,
) -> Vec<ImageLoadRequest> {
    let mut discovered = collect_preload_requests(tab, base_url, device_pixel_ratio);
    discovered.extend(collect_image_requests(tab, base_url, device_pixel_ratio));

    let mut grouped = BTreeMap::<String, ImageLoadRequest>::new();

    for request in discovered {
        let key = request.url.as_str().to_owned();

        if let Some(existing) = grouped.get_mut(&key) {
            existing.priority = existing.priority.min(request.priority);
            existing.loading = existing.loading.min(request.loading);
            existing.top = existing.top.min(request.top);
            existing.preload_only &= request.preload_only;

            for resource in request.resources {
                if !existing.resources.contains(&resource) {
                    existing.resources.push(resource);
                }
            }
        } else {
            grouped.insert(key, request);
        }
    }

    let mut requests: Vec<_> = grouped.into_values().collect();
    requests.sort_by(resource_request_order);
    requests.truncate(MAX_IMAGE_RESOURCE_REQUESTS_PER_DOCUMENT);
    requests
}

fn collect_preload_requests(
    tab: &BrowserTab,
    base_url: &HttpUrl,
    device_pixel_ratio: f32,
) -> Vec<ImageLoadRequest> {
    let isolation_key = NetworkIsolationKey::from_top_level(base_url);
    let mut grouped = BTreeMap::<String, (HttpUrl, ResourcePriority)>::new();

    for preload in tab
        .engine
        .image_preload_requests_for_device(device_pixel_ratio)
        .into_iter()
        .take(MAX_IMAGE_PRELOADS_PER_DOCUMENT)
    {
        let Ok(url) = base_url.resolve(preload.source()) else {
            continue;
        };
        let key = url.as_str().to_owned();

        grouped
            .entry(key)
            .and_modify(|(_, priority)| *priority = (*priority).min(preload.priority()))
            .or_insert((url, preload.priority()));
    }

    grouped
        .into_values()
        .map(|(url, priority)| ImageLoadRequest {
            resources: Vec::new(),
            url,
            isolation_key: isolation_key.clone(),
            loading: ImageLoading::Eager,
            priority,
            top: 0.0,
            preload_only: true,
        })
        .collect()
}

fn collect_image_requests(
    tab: &mut BrowserTab,
    base_url: &HttpUrl,
    device_pixel_ratio: f32,
) -> Vec<ImageLoadRequest> {
    let discovered = tab.engine.image_requests_for_device(device_pixel_ratio);
    let isolation_key = NetworkIsolationKey::from_top_level(base_url);

    let mut grouped = BTreeMap::<
        String,
        (
            HttpUrl,
            Vec<ImageResourceId>,
            ImageLoading,
            ResourcePriority,
            f32,
        ),
    >::new();
    let mut element_count = 0_usize;

    for image_request in discovered {
        if element_count >= MAX_IMAGES_PER_DOCUMENT {
            break;
        }

        let Ok(url) = base_url.resolve(image_request.source()) else {
            continue;
        };

        element_count = element_count.saturating_add(1);
        let key = url.as_str().to_owned();

        grouped
            .entry(key)
            .and_modify(|(_, resources, loading, priority, top)| {
                resources.push(image_request.resource());
                *loading = (*loading).min(image_request.loading());
                *priority = (*priority).min(image_request.priority());
                *top = top.min(image_request.top());
            })
            .or_insert_with(|| {
                (
                    url,
                    vec![image_request.resource()],
                    image_request.loading(),
                    image_request.priority(),
                    image_request.top(),
                )
            });
    }

    let mut requests = Vec::new();

    for (cache_key, (url, resources, loading, priority, top)) in grouped {
        if bind_cached_image(tab, &cache_key, &resources) {
            continue;
        }

        requests.push(ImageLoadRequest {
            resources,
            url,
            isolation_key: isolation_key.clone(),
            loading,
            priority,
            top,
            preload_only: false,
        });
    }

    requests.sort_by(resource_request_order);

    requests
}

fn resource_request_order(left: &ImageLoadRequest, right: &ImageLoadRequest) -> std::cmp::Ordering {
    left.priority
        .cmp(&right.priority)
        .then_with(|| right.preload_only.cmp(&left.preload_only))
        .then_with(|| left.loading.cmp(&right.loading))
        .then_with(|| left.top.total_cmp(&right.top))
}

fn bind_cached_image(tab: &mut BrowserTab, cache_key: &str, resources: &[ImageResourceId]) -> bool {
    let Some((metadata, binding)) = tab
        .image_cache
        .get(cache_key)
        .map(|cached| (cached.metadata, cached.binding.clone()))
    else {
        return false;
    };

    tab.cache_clock = tab.cache_clock.saturating_add(1);
    if let Some(cached) = tab.image_cache.get_mut(cache_key) {
        cached.last_used = tab.cache_clock;
    }

    let viewport_width = tab.engine.layout().viewport_width();
    let mut installed = 0_usize;

    for resource in resources {
        if tab
            .engine
            .install_image_metadata(*resource, metadata, viewport_width)
            .is_ok()
        {
            tab.image_textures.insert(*resource, binding.clone());
            installed = installed.saturating_add(1);
        } else {
            tab.failed_images = tab.failed_images.saturating_add(1);
        }
    }

    tab.loaded_images = tab.loaded_images.saturating_add(installed);
    true
}

fn preload_image(
    network: &NetworkClient,
    budget: &Arc<Mutex<SubresourceBudget>>,
    isolation_key: &NetworkIsolationKey,
    url: &HttpUrl,
) -> Result<(), String> {
    let response =
        fetch_budgeted_binary(network, budget, isolation_key, url, MAX_IMAGE_BODY_BYTES)?;

    if !(200..=299).contains(&response.status()) {
        return Err(format!("HTTP {} ao pré-carregar imagem", response.status()));
    }

    Ok(())
}

fn fetch_and_decode_image(
    network: &NetworkClient,
    budget: &Arc<Mutex<SubresourceBudget>>,
    decoder: &RasterImageDecoder,
    limits: ImageDecodeLimits,
    animation_limits: AnimationDecodeLimits,
    isolation_key: &NetworkIsolationKey,
    url: &HttpUrl,
) -> Result<LoadedImage, String> {
    let response =
        fetch_budgeted_binary(network, budget, isolation_key, url, MAX_IMAGE_BODY_BYTES)?;

    if !(200..=299).contains(&response.status()) {
        return Err(format!("HTTP {} ao carregar imagem", response.status(),));
    }

    let metadata = decoder
        .probe(response.body(), limits)
        .map_err(|error| error.to_string())?;

    let raster = if image_is_animated(response.body(), metadata) {
        LoadedRaster::Animated(
            decoder
                .decode_animation(response.body(), limits, animation_limits)
                .map_err(|error| error.to_string())?,
        )
    } else {
        LoadedRaster::Static(
            decoder
                .decode(response.body(), limits)
                .map_err(|error| error.to_string())?,
        )
    };

    Ok(LoadedImage { metadata, raster })
}

fn poll_tab_images(tab: &mut BrowserTab, context: &egui::Context) {
    const MAX_IMAGES_PER_FRAME: usize = 8;

    if tab
        .pending_images
        .as_ref()
        .is_some_and(|pending| pending.generation != tab.document_generation)
    {
        tab.cancel_image_work();
        return;
    }

    for _ in 0..MAX_IMAGES_PER_FRAME {
        let receive_result = tab
            .pending_images
            .as_ref()
            .map(|pending| pending.receiver.try_recv());

        match receive_result {
            Some(Ok(event)) => {
                if event.generation != tab.document_generation {
                    continue;
                }
                if let Some(pending) = tab.pending_images.as_mut() {
                    pending.remaining = pending.remaining.saturating_sub(1);
                }

                install_loaded_image(tab, context, event);
            }

            Some(Err(TryRecvError::Disconnected)) => {
                let was_cancelled = tab
                    .pending_images
                    .as_ref()
                    .is_some_and(|pending| pending.cancelled.load(Ordering::Acquire));
                let unresolved = tab
                    .pending_images
                    .as_ref()
                    .map_or(0, |pending| pending.remaining);

                if !was_cancelled {
                    tab.failed_images = tab.failed_images.saturating_add(unresolved);
                }
                tab.pending_images = None;
                update_image_status(tab);
                break;
            }

            Some(Err(TryRecvError::Empty)) | None => {
                break;
            }
        }
    }

    let completed = tab
        .pending_images
        .as_ref()
        .is_some_and(|pending| pending.remaining == 0);

    if completed {
        tab.pending_images = None;
        update_image_status(tab);
    }
}

fn activate_deferred_images(network: &NetworkClient, tab: &mut BrowserTab, visible_bottom: f32) {
    if tab.pending_images.is_some() || tab.deferred_images.is_empty() {
        return;
    }

    let threshold = visible_bottom + LAZY_LOAD_MARGIN;
    let mut ready = Vec::new();
    let mut deferred = Vec::new();

    for request in std::mem::take(&mut tab.deferred_images) {
        if request.top <= threshold {
            ready.push(request);
        } else {
            deferred.push(request);
        }
    }

    tab.deferred_images = deferred;
    start_image_batch(network, tab, ready);
}

fn visible_image_resources(
    paint: &PaintList,
    visible_top: f32,
    visible_bottom: f32,
) -> BTreeSet<ImageResourceId> {
    paint
        .commands()
        .iter()
        .enumerate()
        .filter_map(|(index, command)| {
            let PaintCommand::Image { rect, resource, .. } = command else {
                return None;
            };

            if paint.attachment(index) == Some(PaintAttachment::Viewport) {
                return Some(*resource);
            }

            let bottom = rect.y() + rect.height();
            (bottom >= visible_top && rect.y() <= visible_bottom).then_some(*resource)
        })
        .collect()
}

fn install_loaded_image(tab: &mut BrowserTab, context: &egui::Context, event: ImageLoadEvent) {
    let resource_count = event.resources.len();
    let loaded = match event.result {
        Ok(loaded) => loaded,
        Err(_error) => {
            match event.kind {
                ResourceLoadKind::Preload => {
                    tab.failed_preloads = tab.failed_preloads.saturating_add(1);
                }
                ResourceLoadKind::Image => {
                    tab.failed_images = tab.failed_images.saturating_add(resource_count);
                }
            }
            update_image_status(tab);
            return;
        }
    };

    let LoadedResource::Image(LoadedImage { metadata, raster }) = loaded else {
        tab.preloaded_resources = tab.preloaded_resources.saturating_add(1);
        update_image_status(tab);
        return;
    };

    let Some(resource_name) = event.resources.first().copied() else {
        update_image_status(tab);
        return;
    };

    let Some((binding, decoded_bytes)) = image_texture_binding(context, resource_name, raster)
    else {
        tab.failed_images = tab.failed_images.saturating_add(resource_count);
        update_image_status(tab);
        return;
    };

    if decoded_bytes > MAX_TAB_RASTER_BYTES {
        tab.failed_images = tab.failed_images.saturating_add(resource_count);
        update_image_status(tab);
        return;
    }

    evict_image_cache_for(tab, decoded_bytes);

    tab.cache_clock = tab.cache_clock.saturating_add(1);
    tab.image_cache.insert(
        event.cache_key,
        CachedImage {
            metadata,
            binding: binding.clone(),
            raster_bytes: decoded_bytes,
            last_used: tab.cache_clock,
        },
    );
    tab.raster_bytes = tab.raster_bytes.saturating_add(decoded_bytes);

    let viewport_width = tab.engine.layout().viewport_width();
    let mut installed = 0_usize;

    for resource in event.resources {
        if tab
            .engine
            .install_image_metadata(resource, metadata, viewport_width)
            .is_ok()
        {
            tab.image_textures.insert(resource, binding.clone());
            installed = installed.saturating_add(1);
        } else {
            tab.failed_images = tab.failed_images.saturating_add(1);
        }
    }

    tab.loaded_images = tab.loaded_images.saturating_add(installed);
    update_image_status(tab);
}

fn image_texture_binding(
    context: &egui::Context,
    resource: ImageResourceId,
    raster: LoadedRaster,
) -> Option<(ImageTextureBinding, u64)> {
    match raster {
        LoadedRaster::Static(decoded) => {
            let bytes = u64::try_from(decoded.rgba8().len()).ok()?;
            let texture = decoded_image_texture(context, resource, &decoded)?;
            Some((ImageTextureBinding::Static(texture), bytes))
        }
        LoadedRaster::Animated(animation) => {
            let bytes = animation.total_raster_bytes();
            let animation = decoded_animation_texture(context, resource, &animation)?;
            Some((ImageTextureBinding::Animated(Arc::new(animation)), bytes))
        }
    }
}

fn decoded_animation_texture(
    context: &egui::Context,
    resource: ImageResourceId,
    animation: &DecodedAnimation,
) -> Option<AnimatedTexture> {
    let mut frames = Vec::with_capacity(animation.frames().len());
    let mut cycle_duration = Duration::ZERO;

    for (index, frame) in animation.frames().iter().enumerate() {
        let texture = decoded_image_texture_named(
            context,
            format!("phantom-web-image-{}-frame-{index}", resource.as_u64()),
            frame.image(),
        )?;
        let delay = Duration::from_millis(frame.delay_millis().max(10));
        cycle_duration = cycle_duration.saturating_add(delay);
        frames.push(AnimationTextureFrame { texture, delay });
    }

    if frames.is_empty() {
        return None;
    }

    Some(AnimatedTexture {
        frames,
        loop_count: animation.loop_count(),
        cycle_duration,
        clock: Mutex::new(AnimationClock {
            elapsed: Duration::ZERO,
            last_updated: Instant::now(),
            active: false,
        }),
    })
}

fn evict_image_cache_for(tab: &mut BrowserTab, incoming_bytes: u64) {
    while tab
        .raster_bytes
        .checked_add(incoming_bytes)
        .is_none_or(|total| total > MAX_TAB_RASTER_BYTES)
    {
        let Some((oldest_key, _)) = tab
            .image_cache
            .iter()
            .min_by_key(|(_, cached)| cached.last_used)
            .map(|(key, cached)| (key.clone(), cached.last_used))
        else {
            break;
        };

        if let Some(evicted) = tab.image_cache.remove(&oldest_key) {
            tab.raster_bytes = tab.raster_bytes.saturating_sub(evicted.raster_bytes);
            tab.image_textures
                .retain(|_, binding| !binding.same_asset(&evicted.binding));
        }
    }
}

fn decoded_image_texture(
    context: &egui::Context,
    resource: ImageResourceId,
    decoded: &DecodedImage,
) -> Option<egui::TextureHandle> {
    decoded_image_texture_named(
        context,
        format!("phantom-web-image-{}", resource.as_u64()),
        decoded,
    )
}

fn decoded_image_texture_named(
    context: &egui::Context,
    name: String,
    decoded: &DecodedImage,
) -> Option<egui::TextureHandle> {
    let width = usize::try_from(decoded.size().width()).ok()?;
    let height = usize::try_from(decoded.size().height()).ok()?;

    let color_image = egui::ColorImage::from_rgba_unmultiplied([width, height], decoded.rgba8());

    Some(context.load_texture(name, color_image, egui::TextureOptions::LINEAR))
}

fn animated_image_count(tab: &BrowserTab) -> usize {
    tab.image_textures
        .values()
        .filter(|binding| binding.is_animated())
        .count()
}

fn update_image_status(tab: &mut BrowserTab) {
    if let Some(pending) = tab.pending_images.as_ref() {
        let completed = pending.total.saturating_sub(pending.remaining);

        tab.status = format!(
            "Página pronta · recursos {completed}/{} · {} imagens · {} preloads · {} animadas · {} falhas img · {} falhas preload",
            pending.total,
            tab.loaded_images,
            tab.preloaded_resources,
            animated_image_count(tab),
            tab.failed_images,
            tab.failed_preloads,
        );
    } else if !tab.deferred_images.is_empty() {
        tab.status = format!(
            "Página pronta · {} imagens · {} adiadas · {} preloads · {} animadas · {} falhas img",
            tab.loaded_images,
            tab.deferred_images.len(),
            tab.preloaded_resources,
            animated_image_count(tab),
            tab.failed_images,
        );
    } else if tab.loaded_images > 0
        || tab.failed_images > 0
        || tab.preloaded_resources > 0
        || tab.failed_preloads > 0
    {
        tab.status = format!(
            "Página pronta · {} imagens · {} preloads · {} animadas · {} falhas img · {} falhas preload",
            tab.loaded_images,
            tab.preloaded_resources,
            animated_image_count(tab),
            tab.failed_images,
            tab.failed_preloads,
        );
    }
}
fn commit_history(tab: &mut BrowserTab, action: NavigationAction, final_url: &str) {
    ensure_history_scroll_offsets(tab);

    match action {
        NavigationAction::New => {
            if let Some(index) = tab.history_index {
                let keep = index.saturating_add(1);
                tab.history.truncate(keep);
                tab.history_scroll_offsets.truncate(keep);
            } else {
                tab.history.clear();
                tab.history_scroll_offsets.clear();
            }

            tab.history.push(final_url.to_owned());
            tab.history_scroll_offsets.push(0.0);
            tab.history_index = tab.history.len().checked_sub(1);
        }

        NavigationAction::History(index) => {
            if let Some(entry) = tab.history.get_mut(index) {
                *entry = final_url.to_owned();
                tab.history_index = Some(index);
            }

            ensure_history_scroll_offsets(tab);
        }

        NavigationAction::Reload => {
            if let Some(index) = tab.history_index
                && let Some(entry) = tab.history.get_mut(index)
            {
                *entry = final_url.to_owned();
            }

            ensure_history_scroll_offsets(tab);
        }
    }
}
fn pointer_in_navigation_zone(context: &egui::Context) -> bool {
    let rect = context.content_rect();
    let zone_width = (rect.width() * 0.72).clamp(480.0, 980.0);
    let zone = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.bottom() - 34.0),
        egui::vec2(zone_width, 96.0),
    );

    context
        .input(|input| input.pointer.hover_pos())
        .is_some_and(|position| zone.contains(position))
}

// PHANTOM_2G15_UI_ALPHA_16_TAB_SURFACE
const fn tab_surface_fill(panel_fill: egui::Color32) -> egui::Color32 {
    panel_fill
}

// PHANTOM_2G13_VIEWPORT_GEOMETRY_FOUNDATION
fn layout_viewport_changed(
    current_width: f32,
    current_height: f32,
    requested_width: f32,
    requested_height: f32,
) -> bool {
    (current_width - requested_width).abs() > 0.5 || (current_height - requested_height).abs() > 0.5
}

// PHANTOM_2G13_UI_ALPHA_14_TAB_SURFACE
const TAB_ACTIVE_INDICATOR_HEIGHT: f32 = 2.0;
const TAB_ACTIVE_INDICATOR_INSET: f32 = 6.0;

fn active_tab_indicator_rect(tab_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(
            tab_rect.left() + TAB_ACTIVE_INDICATOR_INSET,
            tab_rect.bottom() - TAB_ACTIVE_INDICATOR_HEIGHT,
        ),
        egui::pos2(
            (tab_rect.right() - TAB_ACTIVE_INDICATOR_INSET)
                .max(tab_rect.left() + TAB_ACTIVE_INDICATOR_INSET),
            tab_rect.bottom(),
        ),
    )
}

// PHANTOM_2G12_UI_ALPHA_13_TAB_IDENTITY_GEOMETRY
const TAB_IDENTITY_ICON_SIZE: f32 = 16.0;

fn tab_identity_slot(
    ui: &mut egui::Ui,
    site_icon: Option<&egui::TextureHandle>,
    slot_size: egui::Vec2,
    sense: egui::Sense,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(slot_size, sense);
    let icon_rect = egui::Rect::from_center_size(
        rect.center(),
        egui::vec2(TAB_IDENTITY_ICON_SIZE, TAB_IDENTITY_ICON_SIZE),
    );

    if let Some(site_icon) = site_icon {
        ui.painter()
            .image(site_icon.id(), icon_rect, unit_uv(), egui::Color32::WHITE);
    } else {
        ui.painter().circle_stroke(
            icon_rect.center(),
            3.0,
            egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
        );
    }

    response
}

fn regular_tab_width_for(
    available_width: f32,
    pinned_count: usize,
    regular_count: usize,
    has_recently_closed: bool,
) -> f32 {
    if regular_count == 0 {
        return REGULAR_TAB_MAX_WIDTH;
    }

    let available_width = if available_width.is_finite() {
        available_width.max(0.0)
    } else {
        1280.0
    };
    let controls_width = WINDOW_CONTROLS_LEFT_GAP
        + WINDOW_CONTROL_WIDTH * 3.0
        + WINDOW_CONTROLS_GAP * 2.0
        + WINDOW_CONTROLS_RIGHT_PADDING;
    let history_width = if has_recently_closed {
        HISTORY_BUTTON_WIDTH + TAB_OUTER_WIDTH_ESTIMATE
    } else {
        0.0
    };
    let pinned_width = pinned_count as f32 * (PINNED_TAB_WIDTH + TAB_OUTER_WIDTH_ESTIMATE);
    let pinned_boundary = if pinned_count > 0 { 10.0 } else { 0.0 };
    let reserved_width = controls_width
        + MIN_DRAG_REGION_WIDTH
        + NEW_TAB_BUTTON_WIDTH
        + history_width
        + pinned_width
        + pinned_boundary
        + TOP_CHROME_FIXED_GAP_BUDGET;
    let per_tab_width = ((available_width - reserved_width).max(0.0) / regular_count as f32
        - TAB_OUTER_WIDTH_ESTIMATE)
        .clamp(REGULAR_TAB_MIN_WIDTH, REGULAR_TAB_MAX_WIDTH);
    let count_based_width = (REGULAR_TAB_MAX_WIDTH - regular_count.saturating_sub(3) as f32 * 12.0)
        .clamp(112.0, REGULAR_TAB_MAX_WIDTH);

    per_tab_width.min(count_based_width)
}

// PHANTOM_2G11_UI_ALPHA_12_TAB_IDENTITY
const fn regular_tab_title_reserve(show_close: bool) -> f32 {
    if show_close { 56.0 } else { 28.0 }
}

fn compact_tab_title(title: &str, available_width: f32) -> String {
    let estimated_characters = (available_width.max(0.0) / 7.2).floor() as usize;
    let maximum_characters = estimated_characters.clamp(1, 19);

    truncate_text(title, maximum_characters)
}

fn tab_title(tab: &BrowserTab) -> String {
    let title = truncate_text(&tab.title, 19);

    if tab.is_loading() {
        format!("… {title}")
    } else {
        title
    }
}

fn title_from_url(url: &str) -> String {
    let without_scheme = url
        .split_once("://")
        .map_or(url, |(_scheme, remainder)| remainder);

    let host = without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .trim_start_matches("www.");

    if host.is_empty() {
        "Página".to_owned()
    } else {
        truncate_text(host, 28)
    }
}

fn truncate_text(text: &str, maximum_characters: usize) -> String {
    let mut characters = text.chars();
    let mut truncated: String = characters.by_ref().take(maximum_characters).collect();

    if characters.next().is_some() {
        truncated.push('…');
    }

    truncated
}

// PHANTOM_2G15_FIXED_COMPOSITION_I
fn paint_command_origin(
    document_origin: egui::Pos2,
    viewport_offset: egui::Vec2,
    attachment: PaintAttachment,
) -> egui::Pos2 {
    match attachment {
        PaintAttachment::Document => document_origin,
        PaintAttachment::Viewport => document_origin + viewport_offset,
    }
}

// PHANTOM_2H1_UI_ALPHA_17_LAYOUT_OWNED_TEXT
fn web_text_wrap_mode() -> egui::TextWrapMode {
    // Inline layout already owns line breaking. Re-wrapping the same fragment
    // inside egui can split words vertically when fallback font metrics differ
    // from the native renderer. Extend keeps the renderer faithful to the
    // geometry produced by Phantom's layout engine.
    egui::TextWrapMode::Extend
}

// PHANTOM_2H2_BOX_VISUAL_FIDELITY_I
fn paint_page(
    ui: &mut egui::Ui,
    origin: egui::Pos2,
    viewport_offset: egui::Vec2,
    paint: &PaintList,
    image_textures: &BTreeMap<ImageResourceId, ImageTextureBinding>,
) {
    let now = Instant::now();
    let pixels_per_point = sanitized_pixels_per_point(ui.ctx().pixels_per_point());
    let original_clip = ui.clip_rect();
    let mut clip_stack = vec![original_clip];

    for (index, command) in paint.commands().iter().enumerate() {
        let command_origin = paint_command_origin(
            origin,
            viewport_offset,
            paint.attachment(index).unwrap_or(PaintAttachment::Document),
        );

        match command {
            PaintCommand::FillRect {
                rect,
                radius,
                color,
            } => {
                let target = egui_rect(command_origin, *rect);
                ui.painter()
                    .rect_filled(target, *radius, egui_color(*color));
            }

            PaintCommand::StrokeRect {
                rect,
                radius,
                width,
                color,
            } => {
                if *width <= 0.0 {
                    continue;
                }

                let target = egui_rect(command_origin, *rect);
                ui.painter().rect_stroke(
                    target,
                    *radius,
                    egui::Stroke::new(*width, egui_color(*color)),
                    egui::StrokeKind::Inside,
                );
            }

            PaintCommand::PushClipRect { rect } => {
                let target = egui_rect(command_origin, *rect);
                let parent_clip = clip_stack.last().copied().unwrap_or(original_clip);
                let clip = parent_clip.intersect(target);
                clip_stack.push(clip);
                ui.set_clip_rect(clip);
            }

            PaintCommand::PopClipRect => {
                if clip_stack.len() > 1 {
                    clip_stack.pop();
                }
                ui.set_clip_rect(clip_stack.last().copied().unwrap_or(original_clip));
            }

            PaintCommand::Image {
                rect,
                resource,
                alt,
                fit,
                position,
                opacity,
            } => {
                let target = egui_rect(command_origin, *rect);

                if let Some(texture) = image_textures
                    .get(resource)
                    .and_then(|binding| binding.texture_at(now))
                {
                    paint_fitted_image(
                        ui,
                        texture,
                        target,
                        *fit,
                        *position,
                        *opacity,
                        pixels_per_point,
                    );
                } else {
                    paint_image_placeholder(ui, target, paint, *alt, *opacity, pixels_per_point);
                }
            }

            PaintCommand::Text {
                rect,
                text,
                color,
                font_size,
                weight,
                style,
                family,
                underline,
            } => {
                let Some(content) = paint.text(*text) else {
                    continue;
                };

                let mut rich_text = egui::RichText::new(content)
                    .size(*font_size)
                    .color(egui_color(*color));

                if *weight == PaintFontWeight::Bold {
                    rich_text = rich_text.strong();
                }

                if *style == PaintFontStyle::Italic {
                    rich_text = rich_text.italics();
                }

                if *family == PaintFontFamily::Monospace {
                    rich_text = rich_text.monospace();
                }

                if *underline {
                    rich_text = rich_text.underline();
                }

                ui.put(
                    egui_rect(command_origin, *rect),
                    egui::Label::new(rich_text).wrap_mode(web_text_wrap_mode()),
                );
            }
        }
    }

    ui.set_clip_rect(original_clip);
}

// PHANTOM_2H5_IMAGE_TEXT_COMPOSITION_I
fn paint_fitted_image(
    ui: &egui::Ui,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    fit: ObjectFit,
    position: ObjectPosition,
    opacity: f32,
    pixels_per_point: f32,
) {
    let [width, height] = texture.size();
    let source = egui::vec2(width as f32, height as f32);

    if source.x <= 0.0 || source.y <= 0.0 || target.width() <= 0.0 || target.height() <= 0.0 {
        return;
    }

    let target = snap_rect_to_device_pixels(target, pixels_per_point);
    let (draw_rect, uv) = object_geometry(target, source, fit, position);
    let draw_rect = snap_rect_to_device_pixels(draw_rect, pixels_per_point);
    let alpha = opacity_alpha(opacity);
    let tint = egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha);
    ui.painter().image(texture.id(), draw_rect, uv, tint);
}

fn object_geometry(
    target: egui::Rect,
    source: egui::Vec2,
    fit: ObjectFit,
    position: ObjectPosition,
) -> (egui::Rect, egui::Rect) {
    match fit {
        ObjectFit::Fill => (target, unit_uv()),
        ObjectFit::Contain => contained_geometry(target, source, position),
        ObjectFit::Cover => covered_geometry(target, source, position),
        ObjectFit::None => natural_geometry(target, source, position),
        ObjectFit::ScaleDown => {
            if source.x <= target.width() && source.y <= target.height() {
                natural_geometry(target, source, position)
            } else {
                contained_geometry(target, source, position)
            }
        }
    }
}

fn contained_geometry(
    target: egui::Rect,
    source: egui::Vec2,
    position: ObjectPosition,
) -> (egui::Rect, egui::Rect) {
    let scale = (target.width() / source.x).min(target.height() / source.y);
    let size = source * scale;
    let offset = egui::vec2(
        (target.width() - size.x) * position.x(),
        (target.height() - size.y) * position.y(),
    );
    let min = target.min + offset;
    (egui::Rect::from_min_size(min, size), unit_uv())
}

fn covered_geometry(
    target: egui::Rect,
    source: egui::Vec2,
    position: ObjectPosition,
) -> (egui::Rect, egui::Rect) {
    let scale = (target.width() / source.x).max(target.height() / source.y);
    let displayed = source * scale;
    let visible_x = (target.width() / displayed.x).clamp(0.0, 1.0);
    let visible_y = (target.height() / displayed.y).clamp(0.0, 1.0);
    let u0 = (1.0 - visible_x) * position.x();
    let v0 = (1.0 - visible_y) * position.y();
    let uv = egui::Rect::from_min_max(
        egui::pos2(u0, v0),
        egui::pos2(u0 + visible_x, v0 + visible_y),
    );
    (target, uv)
}

fn natural_geometry(
    target: egui::Rect,
    source: egui::Vec2,
    position: ObjectPosition,
) -> (egui::Rect, egui::Rect) {
    let draw_width = source.x.min(target.width());
    let draw_height = source.y.min(target.height());
    let x = target.left() + (target.width() - draw_width) * position.x();
    let y = target.top() + (target.height() - draw_height) * position.y();

    let visible_x = (draw_width / source.x).clamp(0.0, 1.0);
    let visible_y = (draw_height / source.y).clamp(0.0, 1.0);
    let u0 = (1.0 - visible_x) * position.x();
    let v0 = (1.0 - visible_y) * position.y();

    (
        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(draw_width, draw_height)),
        egui::Rect::from_min_max(
            egui::pos2(u0, v0),
            egui::pos2(u0 + visible_x, v0 + visible_y),
        ),
    )
}

fn unit_uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0))
}

fn paint_image_placeholder(
    ui: &mut egui::Ui,
    target: egui::Rect,
    paint: &PaintList,
    alt: Option<PaintTextRange>,
    opacity: f32,
    pixels_per_point: f32,
) {
    let target = snap_rect_to_device_pixels(target, pixels_per_point);
    let alpha = opacity_alpha(opacity);
    ui.painter().rect_filled(
        target,
        0.0,
        egui::Color32::from_rgba_unmultiplied(36, 36, 36, alpha),
    );

    if let Some(alt_range) = alt
        && let Some(content) = paint.text(alt_range)
        && !content.is_empty()
    {
        ui.put(
            target,
            egui::Label::new(
                egui::RichText::new(content)
                    .size(13.0)
                    .color(egui::Color32::from_rgba_unmultiplied(190, 190, 190, alpha)),
            )
            .wrap(),
        );
    }
}

fn sanitized_pixels_per_point(pixels_per_point: f32) -> f32 {
    if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        pixels_per_point
    } else {
        1.0
    }
}

fn snap_rect_to_device_pixels(rect: egui::Rect, pixels_per_point: f32) -> egui::Rect {
    let scale = sanitized_pixels_per_point(pixels_per_point);
    let snap = |value: f32| (value * scale).round() / scale;

    let min = egui::pos2(snap(rect.min.x), snap(rect.min.y));
    let mut max = egui::pos2(snap(rect.max.x), snap(rect.max.y));
    let minimum_step = 1.0 / scale;

    if max.x <= min.x {
        max.x = min.x + minimum_step;
    }
    if max.y <= min.y {
        max.y = min.y + minimum_step;
    }

    egui::Rect::from_min_max(min, max)
}

fn opacity_alpha(opacity: f32) -> u8 {
    let opacity = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        1.0
    };

    (255.0 * opacity).round().clamp(0.0, 255.0) as u8
}

fn egui_rect(origin: egui::Pos2, rect: PaintRect) -> egui::Rect {
    egui::Rect::from_min_size(
        origin + egui::vec2(rect.x(), rect.y()),
        egui::vec2(rect.width().max(1.0), rect.height().max(1.0)),
    )
}

fn egui_color(color: PaintColor) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.red(), color.green(), color.blue(), color.alpha())
}

fn configure_fonts(context: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        LUCIDE_FONT_NAME.to_owned(),
        Arc::new(egui::FontData::from_static(LUCIDE_FONT_BYTES)),
    );

    fonts
        .families
        .insert(lucide_font_family(), vec![LUCIDE_FONT_NAME.to_owned()]);

    context.set_fonts(fonts);
}

fn lucide_font_family() -> egui::FontFamily {
    egui::FontFamily::Name(LUCIDE_FONT_NAME.into())
}

fn lucide_text(icon: Icon, size: f32) -> egui::RichText {
    egui::RichText::new(char::from(icon).to_string())
        .font(egui::FontId::new(size, lucide_font_family()))
}

fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    enabled: bool,
    size: [f32; 2],
    icon_size: f32,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        ui.add_sized(
            size,
            egui::Button::new(lucide_text(icon, icon_size)).frame(false),
        )
    })
    .inner
}
fn decode_image(bytes: &[u8]) -> Option<image::RgbaImage> {
    image::load_from_memory(bytes)
        .ok()
        .map(|image| image.to_rgba8())
}

fn load_texture(context: &egui::Context, name: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let image = decode_image(bytes)?;
    let width = usize::try_from(image.width()).ok()?;
    let height = usize::try_from(image.height()).ok()?;

    let color_image = egui::ColorImage::from_rgba_unmultiplied([width, height], image.as_raw());

    Some(context.load_texture(name, color_image, egui::TextureOptions::LINEAR))
}

fn load_window_icon() -> Option<egui::IconData> {
    let image = decode_image(APP_ICON_BYTES)?;
    let width = image.width();
    let height = image.height();
    let rgba = image.into_raw();

    Some(egui::IconData {
        rgba,
        width,
        height,
    })
}

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(format!("{APP_NAME} {APP_VERSION}"))
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([800.0, 560.0])
        .with_resizable(true)
        .with_decorations(false)
        .with_taskbar(true);

    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        native_options,
        Box::new(|context| Ok(Box::new(PhantomApp::new(context)))),
    )
}

#[cfg(test)]
mod resource_security;

#[cfg(test)]
mod navigation_compatibility;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    // PHANTOM_2L7_RUNTIME_INTEGRATION_CORPUS_I
    struct CorpusServer {
        address: String,
        stop: Arc<AtomicBool>,
        events: Arc<Mutex<Vec<String>>>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl CorpusServer {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let address = format!("http://{}", listener.local_addr()?);
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let events = Arc::new(Mutex::new(Vec::new()));
            let worker_events = Arc::clone(&events);
            let worker = thread::spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if let Ok(mut events) = worker_events.lock() { events.push("accepted".to_owned()); }
                            if let Err(error) = stream.set_nonblocking(false) {
                                if let Ok(mut events) = worker_events.lock() { events.push(format!("blocking mode: {error}")); }
                                continue;
                            }
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                            let mut request_bytes = Vec::new();
                            let mut buffer = [0_u8; 1024];
                            while request_bytes.len() < 8192 {
                                let read = match stream.read(&mut buffer) {
                                    Ok(read) => read,
                                    Err(error) => {
                                        if let Ok(mut events) = worker_events.lock() { events.push(format!("read: {error}")); }
                                        break;
                                    }
                                };
                                if read == 0 { break; }
                                request_bytes.extend_from_slice(&buffer[..read]);
                                if request_bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                                    break;
                                }
                            }
                            let request = String::from_utf8_lossy(&request_bytes);
                            let path = request.split_whitespace().nth(1).unwrap_or_default();
                            if let Ok(mut events) = worker_events.lock() { events.push(format!("request: {path}; bytes={}", request_bytes.len())); }
                            if path == "/slow.txt" { thread::sleep(Duration::from_millis(150)); }
                            if path == "/lifecycle-old-data.txt" { thread::sleep(Duration::from_millis(600)); }
                            if path.starts_with("/stress-data-") { thread::sleep(Duration::from_millis(12)); }
                            let (status, mime, body) = match path {
                                "/static.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l7/static.html")),
                                "/external.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l7/external.html")),
                                "/async.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l7/async.html")),
                                "/app.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2l7/app.js")),
                                "/data.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l7/data.txt")),
                                "/slow.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l8/slow.html")),
                                "/slow.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l8/slow.txt")),
                                "/dynamic.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l9/dynamic.html")),
                                "/integrated.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l10/integrated.html")),
                                "/integrated-app.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2l10/integrated-app.js")),
                                "/integrated-data.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l10/integrated-data.txt")),
                                "/recovery.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l11/recovery.html")),
                                "/recovery-app.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2l11/recovery-app.js")),
                                "/recovery-data.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l11/recovery-data.txt")),
                                "/lifecycle-old.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l12/lifecycle-old.html")),
                                "/lifecycle-old-data.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l12/lifecycle-old-data.txt")),
                                "/lifecycle-new.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l12/lifecycle-new.html")),
                                "/lifecycle-new.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2l12/lifecycle-new.js")),
                                "/lifecycle-new-data.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l12/lifecycle-new-data.txt")),
                                "/stress-a.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l13/stress-a.html")),
                                "/stress-b.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l13/stress-b.html")),
                                "/stress-c.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2l13/stress-c.html")),
                                "/stress-runtime.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2l13/stress-runtime.js")),
                                "/stress-data-1.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l13/stress-data-1.txt")),
                                "/stress-data-2.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l13/stress-data-2.txt")),
                                "/stress-data-3.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l13/stress-data-3.txt")),
                                "/stress-data-4.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l13/stress-data-4.txt")),
                                "/stress-data-5.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l13/stress-data-5.txt")),
                                "/stress-data-6.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l13/stress-data-6.txt")),
                                "/stress-data-7.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2l13/stress-data-7.txt")),
                                "/promise-assimilation.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m1/promise-assimilation.html")),
                                "/promise-assimilation.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m1/promise-assimilation.js")),
                                "/promise-data.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2m1/promise-data.txt")),
                                "/promise-resolution.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m2/promise-resolution.html")),
                                "/promise-resolution.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m2/promise-resolution.js")),
                                "/promise-first.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2m2/promise-first.txt")),
                                "/promise-second.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2m2/promise-second.txt")),
                                "/promise-cycle.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m3/promise-cycle.html")),
                                "/promise-cycle.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m3/promise-cycle.js")),
                                "/thenable-assimilation.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m4/thenable-assimilation.html")),
                                "/thenable-assimilation.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m4/thenable-assimilation.js")),
                                "/thenable-resolution.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m5/thenable-resolution.html")),
                                "/thenable-resolution.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m5/thenable-resolution.js")),
                                "/promise-final.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m6/promise-final.html")),
                                "/promise-final.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m6/promise-final.js")),
                                "/promise-final-data.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2m6/promise-final-data.txt")),
                                "/function-context.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m7/function-context.html")),
                                "/function-context.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m7/function-context.js")),
                                "/event-target.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m8/event-target.html")),
                                "/event-target.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m8/event-target.js")),
                                "/event-propagation.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m9/event-propagation.html")),
                                "/event-propagation.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m9/event-propagation.js")),
                                "/event-control.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m10/event-control.html")),
                                "/event-control.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m10/event-control.js")),
                                "/event-listeners.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m11/event-listeners.html")),
                                "/event-listeners.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m11/event-listeners.js")),
                                "/classlist.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m12/classlist.html")),
                                "/classlist.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m12/classlist.js")),
                                "/style.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m13/style.html")),
                                "/style.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m13/style.js")),
                                "/dataset.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m14/dataset.html")),
                                "/dataset.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m14/dataset.js")),
                                "/reflection.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m15/reflection.html")),
                                "/reflection.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m15/reflection.js")),
                                "/builtins.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m16/builtins.html")),
                                "/builtins.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m16/builtins.js")),
                                "/core-library.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m17/core-library.html")),
                                "/core-library.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m17/core-library.js")),
                                "/core-library-iii.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m18/core-library-iii.html")),
                                "/core-library-iii.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m18/core-library-iii.js")),
                                "/core-library-final.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m19/core-library-final.html")),
                                "/core-library-final.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m19/core-library-final.js")),
                                "/dom-tree.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m20/dom-tree.html")),
                                "/dom-tree.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m20/dom-tree.js")),
                                "/dom-selectors.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m21/dom-selectors.html")),
                                "/dom-selectors.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m21/dom-selectors.js")),
                                "/dom-mutations.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m22/dom-mutations.html")),
                                "/dom-mutations.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m22/dom-mutations.js")),
                                "/html-elements.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m23/html-elements.html")),
                                "/html-elements.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m23/html-elements.js")),
                                "/native-interaction.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m24/native-interaction.html")),
                                "/native-interaction.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m24/native-interaction.js")),
                                "/web-compat-final.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m25/web-compat-final.html")),
                                "/web-compat-final.js" => ("200 OK", "text/javascript", include_str!("../tests/fixtures/2m25/web-compat-final.js")),
                                "/web-compat-final-data.txt" => ("200 OK", "text/plain", include_str!("../tests/fixtures/2m25/web-compat-final-data.txt")),
                                "/web-compat-final-target.html" => ("200 OK", "text/html", include_str!("../tests/fixtures/2m25/web-compat-final-target.html")),
                                _ => ("404 Not Found", "text/plain", "missing"),
                            };
                            let headers = format!("HTTP/1.1 {status}\r\nContent-Type: {mime}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                            let outcome = stream.write_all(headers.as_bytes())
                                .and_then(|()| stream.write_all(body.as_bytes()))
                                .and_then(|()| stream.flush());
                            if let Ok(mut events) = worker_events.lock() { events.push(format!("response: {path}; {outcome:?}")); }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) => {
                            if let Ok(mut events) = worker_events.lock() { events.push(format!("accept: {error}")); }
                            break;
                        },
                    }
                }
            });
            Ok(Self { address, stop, events, worker: Some(worker) })
        }
    }

    impl Drop for CorpusServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    // PHANTOM_2N1_REAL_WEB_NETWORK_I
    struct RealWebServer {
        address: String,
        stop: Arc<AtomicBool>,
        events: Arc<Mutex<Vec<String>>>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl RealWebServer {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let address = format!("http://{}", listener.local_addr()?);
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let events = Arc::new(Mutex::new(Vec::new()));
            let worker_events = Arc::clone(&events);

            let worker = thread::spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if let Ok(mut events) = worker_events.lock() {
                                events.push("accepted".to_owned());
                            }
                            if let Err(error) = stream.set_nonblocking(false) {
                                if let Ok(mut events) = worker_events.lock() {
                                    events.push(format!("blocking mode: {error}"));
                                }
                                continue;
                            }
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                            let mut request_bytes = Vec::new();
                            let mut buffer = [0_u8; 1024];
                            while request_bytes.len() < 8192 {
                                let read = match stream.read(&mut buffer) {
                                    Ok(read) => read,
                                    Err(error) => {
                                        if let Ok(mut events) = worker_events.lock() {
                                            events.push(format!("read: {error}"));
                                        }
                                        break;
                                    }
                                };
                                if read == 0 {
                                    break;
                                }
                                request_bytes.extend_from_slice(&buffer[..read]);
                                if request_bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                                    break;
                                }
                            }

                            let request = String::from_utf8_lossy(&request_bytes);
                            let path = request.split_whitespace().nth(1).unwrap_or_default();
                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!("request: {path}"));
                            }

                            let (status, mime, extra_headers, body): (&str, &str, &str, &[u8]) =
                                match path {
                                    "/2n1-entry" => (
                                        "302 Found",
                                        "text/plain",
                                        "Location: /2n1-page.html\r\n",
                                        b"redirect",
                                    ),
                                    "/2n1-page.html" => (
                                        "200 OK",
                                        "text/html",
                                        "Content-Encoding: gzip\r\nCache-Control: no-cache\r\n",
                                        include_bytes!("../tests/fixtures/2n1/real-web.html.gz"),
                                    ),
                                    "/2n1-missing.js" => (
                                        "404 Not Found",
                                        "text/javascript",
                                        "",
                                        b"missing",
                                    ),
                                    "/2n1-deferred.js" => (
                                        "200 OK",
                                        "text/javascript",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n1/deferred.js"),
                                    ),
                                    "/2n1-data.txt" => (
                                        "200 OK",
                                        "text/plain",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n1/data.txt"),
                                    ),
                                    _ => (
                                        "404 Not Found",
                                        "text/plain",
                                        "",
                                        b"missing",
                                    ),
                                };

                            let headers = format!(
                                "HTTP/1.1 {status}\r\nContent-Type: {mime}; charset=utf-8\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let outcome = stream
                                .write_all(headers.as_bytes())
                                .and_then(|()| stream.write_all(body))
                                .and_then(|()| stream.flush());

                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!("response: {path}; {outcome:?}"));
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            });

            Ok(Self {
                address,
                stop,
                events,
                worker: Some(worker),
            })
        }
    }

    impl Drop for RealWebServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    // PHANTOM_2N2_EXTERNAL_STYLESHEETS_I
    struct ExternalStyleServer {
        address: String,
        stop: Arc<AtomicBool>,
        events: Arc<Mutex<Vec<String>>>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl ExternalStyleServer {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let address = format!("http://{}", listener.local_addr()?);
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let events = Arc::new(Mutex::new(Vec::new()));
            let worker_events = Arc::clone(&events);

            let worker = thread::spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if let Err(error) = stream.set_nonblocking(false) {
                                if let Ok(mut events) = worker_events.lock() {
                                    events.push(format!("blocking mode: {error}"));
                                }
                                continue;
                            }
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                            let mut request_bytes = Vec::new();
                            let mut buffer = [0_u8; 1024];
                            while request_bytes.len() < 8192 {
                                let read = match stream.read(&mut buffer) {
                                    Ok(read) => read,
                                    Err(error) => {
                                        if let Ok(mut events) = worker_events.lock() {
                                            events.push(format!("read: {error}"));
                                        }
                                        break;
                                    }
                                };
                                if read == 0 {
                                    break;
                                }
                                request_bytes.extend_from_slice(&buffer[..read]);
                                if request_bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                                    break;
                                }
                            }

                            let request = String::from_utf8_lossy(&request_bytes);
                            let path = request.split_whitespace().nth(1).unwrap_or_default();
                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!("request: {path}"));
                            }

                            let (status, mime, extra_headers, body): (&str, &str, &str, &[u8]) =
                                match path {
                                    "/2n2-page.html" => (
                                        "200 OK",
                                        "text/html",
                                        "Cache-Control: no-cache\r\n",
                                        include_bytes!("../tests/fixtures/2n2/external-styles.html"),
                                    ),
                                    "/2n2-missing.css" => (
                                        "404 Not Found",
                                        "text/css",
                                        "",
                                        b"missing",
                                    ),
                                    "/2n2-redirect.css" => (
                                        "302 Found",
                                        "text/plain",
                                        "Location: /2n2-main.css\r\n",
                                        b"redirect",
                                    ),
                                    "/2n2-main.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Content-Encoding: gzip\r\nCache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n2/main.css.gz"),
                                    ),
                                    "/2n2-late.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n2/late.css"),
                                    ),
                                    "/2n2-wrong.css" => (
                                        "200 OK",
                                        "text/plain",
                                        "",
                                        b"#target { color: black !important; }",
                                    ),
                                    "/2n2-alternate.css" | "/2n2-print.css" => (
                                        "500 Internal Server Error",
                                        "text/css",
                                        "",
                                        b"must not be requested",
                                    ),
                                    _ => (
                                        "404 Not Found",
                                        "text/plain",
                                        "",
                                        b"missing",
                                    ),
                                };

                            let headers = format!(
                                "HTTP/1.1 {status}\r\nContent-Type: {mime}; charset=utf-8\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let outcome = stream
                                .write_all(headers.as_bytes())
                                .and_then(|()| stream.write_all(body))
                                .and_then(|()| stream.flush());
                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!("response: {path}; {outcome:?}"));
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            });

            Ok(Self {
                address,
                stop,
                events,
                worker: Some(worker),
            })
        }
    }

    impl Drop for ExternalStyleServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    // PHANTOM_2N4_CSS_IMPORT_GRAPH_I
    struct StylesheetImportServer {
        address: String,
        stop: Arc<AtomicBool>,
        events: Arc<Mutex<Vec<String>>>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl StylesheetImportServer {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let address = format!("http://{}", listener.local_addr()?);
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let events = Arc::new(Mutex::new(Vec::new()));
            let worker_events = Arc::clone(&events);

            let worker = thread::spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if let Err(error) = stream.set_nonblocking(false) {
                                if let Ok(mut events) = worker_events.lock() {
                                    events.push(format!("blocking mode: {error}"));
                                }
                                continue;
                            }
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                            let mut request_bytes = Vec::new();
                            let mut buffer = [0_u8; 1024];
                            while request_bytes.len() < 8192 {
                                let read = match stream.read(&mut buffer) {
                                    Ok(read) => read,
                                    Err(error) => {
                                        if let Ok(mut events) = worker_events.lock() {
                                            events.push(format!("read: {error}"));
                                        }
                                        break;
                                    }
                                };
                                if read == 0 {
                                    break;
                                }
                                request_bytes.extend_from_slice(&buffer[..read]);
                                if request_bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                                    break;
                                }
                            }

                            let request = String::from_utf8_lossy(&request_bytes);
                            let path = request.split_whitespace().nth(1).unwrap_or_default();
                            let lower_request = request.to_ascii_lowercase();
                            let deep_revalidation = lower_request
                                .lines()
                                .any(|line| line.trim() == "if-none-match: \"2n4-deep-v1\"");
                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!("request: {path}"));
                                if path == "/nested/deep/deep.css" && deep_revalidation {
                                    events.push("revalidate: if-none-match=\"2n4-deep-v1\"".to_owned());
                                }
                            }

                            let (status, mime, extra_headers, body): (&str, &str, &str, &[u8]) =
                                match path {
                                    "/2n4-page.html" => (
                                        "200 OK",
                                        "text/html",
                                        "Cache-Control: no-store\r\n",
                                        include_bytes!("../tests/fixtures/2n4/import-graph.html"),
                                    ),
                                    "/2n4-root.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/root.css"),
                                    ),
                                    "/2n4-budget-root.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/budget-root.css"),
                                    ),
                                    "/2n4-budget-child.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/budget-child.css"),
                                    ),
                                    "/nested/base.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/nested/base.css"),
                                    ),
                                    "/nested/deep/deep.css" if deep_revalidation => (
                                        "304 Not Modified",
                                        "text/css",
                                        "Cache-Control: no-cache\r\nETag: \"2n4-deep-v1\"\r\n",
                                        b"",
                                    ),
                                    "/nested/deep/deep.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Content-Encoding: gzip\r\nCache-Control: no-cache\r\nETag: \"2n4-deep-v1\"\r\n",
                                        include_bytes!("../tests/fixtures/2n4/nested/deep/deep.css.gz"),
                                    ),
                                    "/2n4-redirect.css" => (
                                        "302 Found",
                                        "text/plain",
                                        "Location: /2n4-redirect-final.css\r\n",
                                        b"redirect",
                                    ),
                                    "/2n4-redirect-final.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/redirect-final.css"),
                                    ),
                                    "/2n4-missing.css" => (
                                        "404 Not Found",
                                        "text/css",
                                        "",
                                        b"missing",
                                    ),
                                    "/2n4-wrong.css" => (
                                        "200 OK",
                                        "text/plain",
                                        "Cache-Control: no-store\r\n",
                                        b"#order { color: rgb(250, 0, 0) !important; }",
                                    ),
                                    "/cycle-a.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/cycle-a.css"),
                                    ),
                                    "/cycle-b.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/cycle-b.css"),
                                    ),
                                    "/limit-1.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/limit-1.css"),
                                    ),
                                    "/limit-2.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/limit-2.css"),
                                    ),
                                    "/limit-3.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/limit-3.css"),
                                    ),
                                    "/limit-4.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/limit-4.css"),
                                    ),
                                    "/limit-5.css" => (
                                        "200 OK",
                                        "text/css",
                                        "Cache-Control: max-age=60\r\n",
                                        include_bytes!("../tests/fixtures/2n4/limit-5.css"),
                                    ),
                                    _ => ("404 Not Found", "text/plain", "", b"missing"),
                                };

                            let headers = format!(
                                "HTTP/1.1 {status}\r\nContent-Type: {mime}; charset=utf-8\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let outcome = stream
                                .write_all(headers.as_bytes())
                                .and_then(|()| stream.write_all(body))
                                .and_then(|()| stream.flush());
                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!("response: {path}; {status}; {outcome:?}"));
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            });

            Ok(Self {
                address,
                stop,
                events,
                worker: Some(worker),
            })
        }
    }

    impl Drop for StylesheetImportServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    // PHANTOM_2N3_STYLESHEET_LIFECYCLE_CACHE_III
    struct StylesheetLifecycleServer {
        address: String,
        stop: Arc<AtomicBool>,
        events: Arc<Mutex<Vec<String>>>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl StylesheetLifecycleServer {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let address = format!("http://{}", listener.local_addr()?);
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let events = Arc::new(Mutex::new(Vec::new()));
            let worker_events = Arc::clone(&events);

            let worker = thread::spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if let Err(error) = stream.set_nonblocking(false) {
                                if let Ok(mut events) = worker_events.lock() {
                                    events.push(format!("blocking mode: {error}"));
                                }
                                continue;
                            }
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                            let mut request_bytes = Vec::new();
                            let mut buffer = [0_u8; 1024];
                            while request_bytes.len() < 8192 {
                                let read = match stream.read(&mut buffer) {
                                    Ok(read) => read,
                                    Err(error) => {
                                        if let Ok(mut events) = worker_events.lock() {
                                            events.push(format!("read: {error}"));
                                        }
                                        break;
                                    }
                                };
                                if read == 0 { break; }
                                request_bytes.extend_from_slice(&buffer[..read]);
                                if request_bytes.windows(4).any(|window| window == b"\r\n\r\n") { break; }
                            }

                            let request = String::from_utf8_lossy(&request_bytes);
                            let path = request.split_whitespace().nth(1).unwrap_or_default();
                            let lower_request = request.to_ascii_lowercase();
                            let revalidating = lower_request
                                .lines()
                                .any(|line| line.trim() == "if-none-match: \"2n3-v1\"");
                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!("request: {path}"));
                                if path == "/2n3-revalidate.css" && revalidating {
                                    events.push("revalidate: if-none-match=\"2n3-v1\"".to_owned());
                                }
                            }

                            if path == "/2n3-slow.css" {
                                thread::sleep(Duration::from_millis(250));
                            }

                            let (status, mime, extra_headers, body): (&str, &str, &str, &[u8]) = match path {
                                "/2n3-cache.html" => ("200 OK", "text/html", "Cache-Control: no-store\r\n", include_bytes!("../tests/fixtures/2n3/cache.html")),
                                "/2n3-cached.css" => ("200 OK", "text/css", "Cache-Control: max-age=60\r\n", include_bytes!("../tests/fixtures/2n3/cached.css")),
                                "/2n3-revalidate.css" if revalidating => ("304 Not Modified", "text/css", "Cache-Control: no-cache\r\nETag: \"2n3-v1\"\r\n", b""),
                                "/2n3-revalidate.css" => ("200 OK", "text/css", "Cache-Control: no-cache\r\nETag: \"2n3-v1\"\r\n", include_bytes!("../tests/fixtures/2n3/revalidate.css")),
                                "/2n3-slow.html" => ("200 OK", "text/html", "Cache-Control: no-store\r\n", include_bytes!("../tests/fixtures/2n3/slow.html")),
                                "/2n3-slow.css" => ("200 OK", "text/css", "Cache-Control: no-store\r\n", include_bytes!("../tests/fixtures/2n3/slow.css")),
                                "/2n3-clean.html" => ("200 OK", "text/html", "Cache-Control: no-store\r\n", include_bytes!("../tests/fixtures/2n3/clean.html")),
                                _ => ("404 Not Found", "text/plain", "", b"missing"),
                            };

                            let headers = format!(
                                "HTTP/1.1 {status}\r\nContent-Type: {mime}; charset=utf-8\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let outcome = stream.write_all(headers.as_bytes())
                                .and_then(|()| stream.write_all(body))
                                .and_then(|()| stream.flush());
                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!("response: {path}; {status}; {outcome:?}"));
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            });

            Ok(Self { address, stop, events, worker: Some(worker) })
        }
    }

    impl Drop for StylesheetLifecycleServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() { let _ = worker.join(); }
        }
    }

    // PHANTOM_2N5_TEXT_SUBRESOURCE_POLICY_II
    struct TextSubresourcePolicyServer {
        address: String,
        stop: Arc<AtomicBool>,
        events: Arc<Mutex<Vec<String>>>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl TextSubresourcePolicyServer {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let address = format!("http://{}", listener.local_addr()?);
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let events = Arc::new(Mutex::new(Vec::new()));
            let worker_events = Arc::clone(&events);

            let worker = thread::spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if let Err(error) = stream.set_nonblocking(false) {
                                if let Ok(mut events) = worker_events.lock() {
                                    events.push(format!("blocking mode: {error}"));
                                }
                                continue;
                            }
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                            let mut request_bytes = Vec::new();
                            let mut buffer = [0_u8; 1024];
                            while request_bytes.len() < 8192 {
                                let read = match stream.read(&mut buffer) {
                                    Ok(read) => read,
                                    Err(error) => {
                                        if let Ok(mut events) = worker_events.lock() {
                                            events.push(format!("read: {error}"));
                                        }
                                        break;
                                    }
                                };
                                if read == 0 { break; }
                                request_bytes.extend_from_slice(&buffer[..read]);
                                if request_bytes.windows(4).any(|window| window == b"\r\n\r\n") { break; }
                            }

                            let request = String::from_utf8_lossy(&request_bytes);
                            let path = request.split_whitespace().nth(1).unwrap_or_default();
                            let lower_request = request.to_ascii_lowercase();
                            let revalidating_data = lower_request
                                .lines()
                                .any(|line| line.trim() == "if-none-match: \"2n5-data-v1\"");
                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!("request: {path}"));
                                if path == "/2n5-data.txt" && revalidating_data {
                                    events.push("revalidate: if-none-match=\"2n5-data-v1\"".to_owned());
                                }
                            }

                            let (status, mime, extra_headers, body): (&str, &str, &str, &[u8]) = match path {
                                "/2n5-page.html" => (
                                    "200 OK",
                                    "text/html",
                                    "Cache-Control: no-store\r\n",
                                    include_bytes!("../tests/fixtures/2n5/policy.html"),
                                ),
                                "/2n5-policy.css" => (
                                    "200 OK",
                                    "text/css",
                                    "Cache-Control: max-age=60\r\n",
                                    include_bytes!("../tests/fixtures/2n5/policy.css"),
                                ),
                                "/2n5-policy.js" => (
                                    "200 OK",
                                    "text/javascript",
                                    "Cache-Control: max-age=60\r\n",
                                    include_bytes!("../tests/fixtures/2n5/policy.js"),
                                ),
                                "/2n5-data.txt" if revalidating_data => (
                                    "304 Not Modified",
                                    "text/plain",
                                    "Cache-Control: no-cache\r\nETag: \"2n5-data-v1\"\r\n",
                                    b"",
                                ),
                                "/2n5-data.txt" => (
                                    "200 OK",
                                    "text/plain",
                                    "Cache-Control: no-cache\r\nETag: \"2n5-data-v1\"\r\n",
                                    include_bytes!("../tests/fixtures/2n5/data.txt"),
                                ),
                                "/2n5-probe.txt" => (
                                    "200 OK",
                                    "text/plain",
                                    "Cache-Control: no-store\r\n",
                                    b"probe",
                                ),
                                _ => ("404 Not Found", "text/plain", "", b"missing"),
                            };

                            let headers = format!(
                                "HTTP/1.1 {status}\r\nContent-Type: {mime}; charset=utf-8\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let outcome = stream.write_all(headers.as_bytes())
                                .and_then(|()| stream.write_all(body))
                                .and_then(|()| stream.flush());
                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!("response: {path}; {status}; {outcome:?}"));
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            });

            Ok(Self { address, stop, events, worker: Some(worker) })
        }
    }

    impl Drop for TextSubresourcePolicyServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() { let _ = worker.join(); }
        }
    }

    // PHANTOM_2N7_CORS_TRANSPORT_I
    struct CorsTransportServer {
        address: String,
        stop: Arc<AtomicBool>,
        events: Arc<Mutex<Vec<String>>>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl CorsTransportServer {
        fn new(exact_origin: String) -> Result<Self, Box<dyn std::error::Error>> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let address = format!("http://{}", listener.local_addr()?);
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let events = Arc::new(Mutex::new(Vec::new()));
            let worker_events = Arc::clone(&events);

            let worker = thread::spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if stream.set_nonblocking(false).is_err() {
                                continue;
                            }
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                            let mut request_bytes = Vec::new();
                            let mut buffer = [0_u8; 1024];
                            while request_bytes.len() < 8192 {
                                let read = match stream.read(&mut buffer) {
                                    Ok(read) => read,
                                    Err(_) => break,
                                };
                                if read == 0 {
                                    break;
                                }
                                request_bytes.extend_from_slice(&buffer[..read]);
                                if request_bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                                    break;
                                }
                            }

                            let header_end = request_bytes
                                .windows(4)
                                .position(|window| window == b"\r\n\r\n")
                                .map(|position| position + 4)
                                .unwrap_or(request_bytes.len());
                            let header_text =
                                String::from_utf8_lossy(&request_bytes[..header_end]).into_owned();
                            let content_length = header_text
                                .lines()
                                .find_map(|line| {
                                    let (name, value) = line.split_once(':')?;
                                    name.trim()
                                        .eq_ignore_ascii_case("content-length")
                                        .then(|| value.trim().parse::<usize>().ok())
                                        .flatten()
                                })
                                .unwrap_or(0)
                                .min(1024 * 1024);
                            let target_len = header_end.saturating_add(content_length);
                            while request_bytes.len() < target_len {
                                let remaining = target_len - request_bytes.len();
                                let chunk_len = remaining.min(buffer.len());
                                let read = match stream.read(&mut buffer[..chunk_len]) {
                                    Ok(read) => read,
                                    Err(_) => break,
                                };
                                if read == 0 {
                                    break;
                                }
                                request_bytes.extend_from_slice(&buffer[..read]);
                            }

                            let request = header_text;
                            let request_body = request_bytes
                                .get(header_end..target_len.min(request_bytes.len()))
                                .unwrap_or_default();
                            let mut request_parts = request.split_whitespace();
                            let request_method = request_parts.next().unwrap_or_default();
                            let path = request_parts.next().unwrap_or_default();
                            let header_value = |target: &str| {
                                request.lines().find_map(|line| {
                                    let (name, value) = line.split_once(':')?;
                                    name.trim()
                                        .eq_ignore_ascii_case(target)
                                        .then(|| value.trim().to_owned())
                                })
                            };
                            let request_origin = header_value("origin");
                            let request_method_header =
                                header_value("access-control-request-method");
                            let request_headers_header =
                                header_value("access-control-request-headers");
                            let x_phantom = header_value("x-phantom");
                            let cookie = header_value("cookie");
                            let content_type = header_value("content-type");
                            let body_text = String::from_utf8_lossy(request_body);
                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!(
                                    "request: {path}; method={request_method}; origin={}; acrm={}; acrh={}; x-phantom={}; cookie={}; content-type={}; body={body_text}",
                                    request_origin.as_deref().unwrap_or("<none>"),
                                    request_method_header.as_deref().unwrap_or("<none>"),
                                    request_headers_header.as_deref().unwrap_or("<none>"),
                                    x_phantom.as_deref().unwrap_or("<none>"),
                                    cookie.as_deref().unwrap_or("<none>"),
                                    content_type.as_deref().unwrap_or("<none>")
                                ));
                            }

                            if path == "/2n7-slow-star.txt" {
                                thread::sleep(Duration::from_millis(180));
                            }

                            let (status, extra_headers, body): (&str, String, &[u8]) = match path {
                                "/2n7-allow-star.txt" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n".to_owned(),
                                    b"CORS STAR GREEN",
                                ),
                                "/2n7-allow-exact.txt" => (
                                    "200 OK",
                                    format!(
                                        "Access-Control-Allow-Origin: {exact_origin}\r\nCache-Control: no-store\r\n"
                                    ),
                                    b"CORS EXACT GREEN",
                                ),
                                "/2n7-no-acao.txt" => (
                                    "200 OK",
                                    "Cache-Control: no-store\r\n".to_owned(),
                                    b"CORS MUST NOT LEAK",
                                ),
                                "/2n7-wrong-origin.txt" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: http://wrong.example\r\nCache-Control: no-store\r\n".to_owned(),
                                    b"CORS WRONG MUST NOT LEAK",
                                ),
                                "/2n7-multi-origin.txt" => (
                                    "200 OK",
                                    format!(
                                        "Access-Control-Allow-Origin: {exact_origin}, http://wrong.example\r\nCache-Control: no-store\r\n"
                                    ),
                                    b"CORS MULTI MUST NOT LEAK",
                                ),
                                "/2n7-redirect.txt" => (
                                    "302 Found",
                                    "Location: /2n7-allow-star.txt\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n".to_owned(),
                                    b"redirect",
                                ),
                                "/2n8-redirect-star.txt" => (
                                    "302 Found",
                                    "Location: /2n8-final-star.txt\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n".to_owned(),
                                    b"redirect",
                                ),
                                "/2n8-redirect-exact.txt" => (
                                    "302 Found",
                                    format!(
                                        "Location: /2n8-final-exact.txt\r\nAccess-Control-Allow-Origin: {exact_origin}\r\nCache-Control: no-store\r\n"
                                    ),
                                    b"redirect",
                                ),
                                "/2n8-redirect-no-acao.txt" => (
                                    "302 Found",
                                    "Location: /2n8-denied-final.txt\r\nCache-Control: no-store\r\n".to_owned(),
                                    b"redirect",
                                ),
                                "/2n8-redirect-wrong-origin.txt" => (
                                    "302 Found",
                                    "Location: /2n8-denied-final.txt\r\nAccess-Control-Allow-Origin: http://wrong.example\r\nCache-Control: no-store\r\n".to_owned(),
                                    b"redirect",
                                ),
                                "/2n8-final-star.txt" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n".to_owned(),
                                    b"CORS REDIRECT STAR GREEN",
                                ),
                                "/2n8-final-exact.txt" => (
                                    "200 OK",
                                    format!(
                                        "Access-Control-Allow-Origin: {exact_origin}\r\nCache-Control: no-store\r\n"
                                    ),
                                    b"CORS REDIRECT EXACT GREEN",
                                ),
                                "/2n8-denied-final.txt" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n".to_owned(),
                                    b"CORS REDIRECT DENIED FINAL MUST NOT LOAD",
                                ),
                                "/2n15-post-json.txt"
                                    if request_method == "OPTIONS" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Access-Control-Allow-Methods: POST\r\n",
                                        "Access-Control-Allow-Headers: content-type,x-phantom\r\n",
                                        "Access-Control-Max-Age: 60\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n15-post-json.txt"
                                    if request_method == "POST" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n"
                                        .to_owned(),
                                    b"POST BODY GREEN",
                                ),
                                "/2n15-put.txt"
                                    if request_method == "OPTIONS" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Access-Control-Allow-Methods: PUT\r\n",
                                        "Access-Control-Allow-Headers: content-type\r\n",
                                        "Access-Control-Max-Age: 60\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n15-put.txt"
                                    if request_method == "PUT" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n"
                                        .to_owned(),
                                    b"PUT BODY GREEN",
                                ),
                                "/2n15-patch.txt"
                                    if request_method == "OPTIONS" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Access-Control-Allow-Methods: PATCH\r\n",
                                        "Access-Control-Allow-Headers: content-type,x-phantom\r\n",
                                        "Access-Control-Max-Age: 60\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n15-patch.txt"
                                    if request_method == "PATCH" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n"
                                        .to_owned(),
                                    b"PATCH BODY GREEN",
                                ),
                                "/2n15-deny-method.txt"
                                    if request_method == "OPTIONS" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Access-Control-Allow-Methods: GET\r\n",
                                        "Access-Control-Allow-Headers: content-type\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n15-deny-method.txt" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n"
                                        .to_owned(),
                                    b"DENIED BODY MUST NOT SEND",
                                ),
                                "/2n14-credentialed.txt"
                                    if request_method == "OPTIONS" => (
                                    "204 No Content",
                                    format!(
                                        "Access-Control-Allow-Origin: {exact_origin}\r\nAccess-Control-Allow-Credentials: true\r\nAccess-Control-Allow-Headers: x-phantom\r\nAccess-Control-Max-Age: 60\r\nCache-Control: no-store\r\n"
                                    ),
                                    b"",
                                ),
                                "/2n14-credentialed.txt" => (
                                    "200 OK",
                                    format!(
                                        "Access-Control-Allow-Origin: {exact_origin}\r\nAccess-Control-Allow-Credentials: true\r\nSet-Cookie: rotated=1; Path=/; SameSite=Lax\r\nCache-Control: no-store\r\n"
                                    ),
                                    b"CREDENTIALED CORS GREEN",
                                ),
                                "/2n14-wildcard-final.txt"
                                    if request_method == "OPTIONS" => (
                                    "204 No Content",
                                    format!(
                                        "Access-Control-Allow-Origin: {exact_origin}\r\nAccess-Control-Allow-Credentials: true\r\nAccess-Control-Allow-Headers: x-phantom\r\nAccess-Control-Max-Age: 60\r\nCache-Control: no-store\r\n"
                                    ),
                                    b"",
                                ),
                                "/2n14-wildcard-final.txt" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Credentials: true\r\nSet-Cookie: denied=1; Path=/; SameSite=Lax\r\nCache-Control: no-store\r\n".to_owned(),
                                    b"CREDENTIAL WILDCARD MUST NOT ESCAPE",
                                ),
                                "/2n14-acac-case.txt"
                                    if request_method == "OPTIONS" => (
                                    "204 No Content",
                                    format!(
                                        "Access-Control-Allow-Origin: {exact_origin}\r\nAccess-Control-Allow-Credentials: true\r\nAccess-Control-Allow-Headers: x-phantom\r\nAccess-Control-Max-Age: 60\r\nCache-Control: no-store\r\n"
                                    ),
                                    b"",
                                ),
                                "/2n14-acac-case.txt" => (
                                    "200 OK",
                                    format!(
                                        "Access-Control-Allow-Origin: {exact_origin}\r\nAccess-Control-Allow-Credentials: True\r\nSet-Cookie: deniedcase=1; Path=/; SameSite=Lax\r\nCache-Control: no-store\r\n"
                                    ),
                                    b"CREDENTIAL ACAC CASE MUST NOT ESCAPE",
                                ),
                                "/2n14-preflight-wildcard.txt"
                                    if request_method == "OPTIONS" => (
                                    "204 No Content",
                                    "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Credentials: true\r\nAccess-Control-Allow-Headers: *\r\nAccess-Control-Max-Age: 60\r\nCache-Control: no-store\r\n".to_owned(),
                                    b"",
                                ),
                                "/2n14-preflight-wildcard.txt" => (
                                    "200 OK",
                                    format!(
                                        "Access-Control-Allow-Origin: {exact_origin}\r\nAccess-Control-Allow-Credentials: true\r\nCache-Control: no-store\r\n"
                                    ),
                                    b"PRELIGHT WILDCARD MUST NOT REACH MAIN GET",
                                ),
                                "/2n12-main-custom-header.txt"
                                    if request_method == "OPTIONS" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Access-Control-Allow-Headers: x-phantom\r\n",
                                        "Access-Control-Max-Age: 60\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n12-main-custom-header.txt" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n"
                                        .to_owned(),
                                    b"CORS PREFLIGHTED MAIN GET GREEN",
                                ),
                                "/2n12-final-deny.txt"
                                    if request_method == "OPTIONS" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Access-Control-Allow-Headers: x-phantom\r\n",
                                        "Access-Control-Max-Age: 60\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n12-final-deny.txt" => (
                                    "200 OK",
                                    "Cache-Control: no-store\r\n".to_owned(),
                                    b"CORS FINAL DENY MUST NOT ESCAPE",
                                ),
                                "/2n12-redirect-custom.txt"
                                    if request_method == "OPTIONS" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Access-Control-Allow-Headers: x-phantom\r\n",
                                        "Access-Control-Max-Age: 60\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n12-redirect-custom.txt" => (
                                    "302 Found",
                                    concat!(
                                        "Location: /2n12-main-custom-header.txt\r\n",
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n11-preflight-put-json.txt" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Access-Control-Allow-Methods: PUT\r\n",
                                        "Access-Control-Allow-Headers: content-type,x-phantom\r\n",
                                        "Access-Control-Max-Age: 60\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n11-preflight-wildcard.txt" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Access-Control-Allow-Methods: *\r\n",
                                        "Access-Control-Allow-Headers: *, authorization\r\n",
                                        "Access-Control-Max-Age: 60\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n11-preflight-deny-method.txt" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Access-Control-Allow-Methods: POST\r\n",
                                        "Access-Control-Allow-Headers: content-type\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n11-preflight-deny-header.txt" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Access-Control-Allow-Methods: PUT\r\n",
                                        "Access-Control-Allow-Headers: x-other\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n11-preflight-no-acao.txt" => (
                                    "204 No Content",
                                    concat!(
                                        "Access-Control-Allow-Methods: PUT\r\n",
                                        "Access-Control-Allow-Headers: content-type\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n11-preflight-redirect.txt" => (
                                    "302 Found",
                                    concat!(
                                        "Location: /2n11-preflight-put-json.txt\r\n",
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Cache-Control: no-store\r\n"
                                    )
                                    .to_owned(),
                                    b"",
                                ),
                                "/2n9-safe-headers.txt" => (
                                    "200 OK",
                                    concat!(
                                        "Access-Control-Allow-Origin: *\r\n",
                                        "Cache-Control: no-store\r\n",
                                        "Content-Language: pt-BR\r\n",
                                        "Expires: Wed, 21 Oct 2026 07:28:00 GMT\r\n",
                                        "Last-Modified: Tue, 20 Oct 2026 07:28:00 GMT\r\n",
                                        "Pragma: no-cache\r\n",
                                        "Set-Cookie: phantom-secret=blocked\r\n",
                                        "X-Phantom-Secret: blocked\r\n"
                                    )
                                    .to_owned(),
                                    b"CORS SAFE HEADERS GREEN",
                                ),
                                "/2n7-cache-star.txt" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: *\r\nCache-Control: max-age=60\r\nVary: Origin\r\n".to_owned(),
                                    b"CORS CACHE GREEN",
                                ),
                                "/2n7-slow-star.txt" => (
                                    "200 OK",
                                    "Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n".to_owned(),
                                    b"CORS SLOW OLD RESULT",
                                ),
                                _ => (
                                    "404 Not Found",
                                    "Cache-Control: no-store\r\n".to_owned(),
                                    b"missing",
                                ),
                            };

                            let headers = format!(
                                "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let outcome = stream
                                .write_all(headers.as_bytes())
                                .and_then(|()| stream.write_all(body))
                                .and_then(|()| stream.flush());
                            if let Ok(mut events) = worker_events.lock() {
                                events.push(format!("response: {path}; {status}; {outcome:?}"));
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            });

            Ok(Self {
                address,
                stop,
                events,
                worker: Some(worker),
            })
        }
    }

    impl Drop for CorsTransportServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    fn corpus_painted_text(tab: &BrowserTab) -> String {
        tab.engine.paint_list().commands().iter().filter_map(|command| match command {
            PaintCommand::Text { text, .. } => tab.engine.paint_list().text(*text),
            _ => None,
        }).collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn runtime_integration_corpus_static_external_fetch_timer()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        for (page, expected) in [
            ("/static.html", "Static painted"),
            ("/external.html", "Loaded script in order"),
            ("/async.html", "Fetch completed"),
        ] {
            start_navigation(&network, &mut tab, format!("{}{page}", server.address), NavigationAction::New);
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut peak_pending_fetches = 0_usize;
            let mut peak_pending_timers = 0_usize;
            while Instant::now() < deadline {
                poll_tab_navigation(&mut tab, &network, 1.0);
                poll_pending_page_scripts(&mut tab, 1.0);
                peak_pending_timers = peak_pending_timers
                    .max(tab.engine.pending_javascript_timer_count());
                pump_page_timers(&mut tab, Instant::now(), 1.0);
                poll_page_fetches(&mut tab, 1.0);
                peak_pending_fetches = peak_pending_fetches.max(tab.pending_script_fetches.len());
                let painted = corpus_painted_text(&tab);
                if tab.navigation_phase() == NavigationPhase::Failed { break; }
                if tab.navigation_phase() == NavigationPhase::Ready
                    && painted.contains(expected)
                    && (page != "/async.html" || painted.contains("Timer fired"))
                {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            let painted = corpus_painted_text(&tab);
            let events = server.events.lock().map_or_else(|_| vec!["event lock failed".to_owned()], |events| events.clone());
            assert_eq!(tab.navigation_phase(), NavigationPhase::Ready, "{page}: {}; server={events:?}", tab.status);
            assert!(painted.contains(expected), "{page}: painted={painted:?}; status={}; server={events:?}", tab.status);
            if page == "/async.html" {
                assert!(painted.contains("Timer fired"), "{page}: painted={painted:?}; status={}", tab.status);
            }
        }
        Ok(())
    }

    // PHANTOM_2L8_NAVIGATION_ASYNC_ISOLATION_I
    #[test]
    fn old_fetch_completion_cannot_mutate_next_navigation()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        start_navigation(&network, &mut tab, format!("{}/slow.html", server.address), NavigationAction::New);
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_page_fetches(&mut tab, 1.0);
            let request_started = server.events.lock().is_ok_and(|events| {
                events.iter().any(|event| event.starts_with("request: /slow.txt;"))
            });
            if request_started { break; }
            thread::sleep(Duration::from_millis(5));
        }
        let old_events = server.events.lock().map_or_else(|_| Vec::new(), |events| events.clone());
        assert!(old_events.iter().any(|event| event.starts_with("request: /slow.txt;")), "slow fetch did not start: {old_events:?}; status={}", tab.status);
        start_navigation(&network, &mut tab, format!("{}/static.html", server.address), NavigationAction::New);
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_page_fetches(&mut tab, 1.0);
            let slow_responded = server.events.lock().is_ok_and(|events| {
                events.iter().any(|event| event.starts_with("response: /slow.txt;"))
            });
            if tab.navigation_phase() == NavigationPhase::Ready && slow_responded { break; }
            if tab.navigation_phase() == NavigationPhase::Failed { break; }
            thread::sleep(Duration::from_millis(5));
        }
        let painted = corpus_painted_text(&tab);
        let events = server.events.lock().map_or_else(|_| Vec::new(), |events| events.clone());
        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready, "status={}; server={events:?}", tab.status);
        assert!(events.iter().any(|event| event.starts_with("response: /slow.txt;")), "slow response did not complete: {events:?}");
        assert!(painted.contains("Static painted"), "painted={painted:?}; server={events:?}");
        assert!(!painted.contains("Old navigation"), "stale fetch changed the new page: {painted:?}");
        assert!(tab.pending_script_fetches.is_empty());
        Ok(())
    }

    // PHANTOM_2L9_HTML_CSS_DOM_JS_PAINT_CORPUS_I
    #[test]
    fn page_script_dom_style_and_new_node_reach_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        start_navigation(&network, &mut tab, format!("{}/dynamic.html", server.address), NavigationAction::New);
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Ready && painted.contains("Created by script") { break; }
            if tab.navigation_phase() == NavigationPhase::Failed { break; }
            thread::sleep(Duration::from_millis(5));
        }
        let painted = corpus_painted_text(&tab);
        let events = server.events.lock().map_or_else(|_| Vec::new(), |events| events.clone());
        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready, "status={}; server={events:?}", tab.status);
        assert!(painted.contains("Styled by script"), "painted={painted:?}; status={}", tab.status);
        assert!(painted.contains("Created by script"), "painted={painted:?}; status={}", tab.status);
        assert!(!painted.contains("Before script"), "old paint remains: {painted:?}");
        assert!(tab.engine.paint_list().commands().iter().any(|command| match command {
            PaintCommand::Text { text, color, .. } => {
                tab.engine.paint_list().text(*text).is_some_and(|value| value.contains("Styled"))
                    && color.red() == 255 && color.green() == 0 && color.blue() == 0
            }
            _ => false,
        }), "CSS color did not reach paint: {painted:?}");
        Ok(())
    }

    // PHANTOM_2L10_R10_FRAGMENT_AWARE_PAINT_GATE_I
    #[test]
    fn integrated_page_runs_external_inline_fetch_timer_dom_css_and_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        start_navigation(
            &network,
            &mut tab,
            format!("{}/integrated.html", server.address),
            NavigationAction::New,
        );

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("External ready")
                && painted.contains("Inline after external")
                && painted.contains("Fetched integration data")
                && painted.contains("Timer integration ready")
                && painted.contains("Created integration node")
            {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let painted = corpus_painted_text(&tab);
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "status={}; server={events:?}",
            tab.status
        );
        for expected in [
            "Phantom integrated corpus",
            "External ready",
            "Inline after external",
            "Fetched integration data",
            "Timer integration ready",
            "Created integration node",
        ] {
            assert!(
                painted.contains(expected),
                "missing {expected:?}; painted={painted:?}; status={}; server={events:?}",
                tab.status
            );
        }
        assert!(
            !painted.contains("Waiting for external"),
            "stale initial DOM reached final paint: {painted:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /integrated-app.js;")),
            "external script was not requested: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /integrated-data.txt;")),
            "fetch resource was not requested: {events:?}"
        );

        let paint_fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        eprintln!("2L10-R10 DIAG async DOM text={painted:?}");
        eprintln!("2L10-R10 DIAG Paint fragments={paint_fragments:?}");

        // Paint may split one DOM text node into multiple commands (for example
        // at wrapping/shaping boundaries). Validate the actual ordered span
        // instead of assuming "External ready" is stored in one PaintCommand.
        let mut status_span = Vec::new();
        let mut collecting = false;
        for (text, color) in &paint_fragments {
            if !collecting && text.contains("External") {
                collecting = true;
            }
            if collecting {
                status_span.push((text.clone(), *color));
                if text.contains("ready") {
                    break;
                }
            }
        }
        let status_text = status_span
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!("2L10-R10 DIAG status span={status_span:?}");

        assert!(
            status_text.contains("External") && status_text.contains("ready"),
            "could not resolve External ready across PaintCommand fragments; fragments={paint_fragments:?}"
        );
        assert!(
            status_span
                .iter()
                .all(|(_, color)| *color == (255, 0, 0)),
            "async inline style did not color every status Paint fragment red; status_span={status_span:?}; painted={painted:?}"
        );

        Ok(())
    }

    // PHANTOM_2L11_INTEGRATED_CORPUS_II_MALFORMED_RECOVERY_I
    #[test]
    fn malformed_recovery_page_preserves_scripts_async_css_and_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        start_navigation(
            &network,
            &mut tab,
            format!("{}/recovery.html", server.address),
            NavigationAction::New,
        );

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Recovered external state")
                && painted.contains("Recovery inline after external")
                && painted.contains("Recovered fetch data")
                && painted.contains("Recovery timer ready")
                && painted.contains("Recovery created node")
            {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let painted = corpus_painted_text(&tab);
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "status={}; server={events:?}",
            tab.status
        );
        for expected in [
            "Phantom recovery corpus",
            "Recovered external state",
            "Recovery inline after external",
            "Recovered fetch data",
            "Recovery timer ready",
            "Recovery created node",
        ] {
            assert!(
                painted.contains(expected),
                "missing {expected:?}; painted={painted:?}; status={}; server={events:?}",
                tab.status
            );
        }
        assert!(
            !painted.contains("Waiting recovery"),
            "stale recovery placeholder reached final paint: {painted:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /recovery-app.js;")),
            "recovery external script was not requested: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /recovery-data.txt;")),
            "recovery fetch resource was not requested: {events:?}"
        );

        let report = tab
            .engine
            .html_build_report()
            .ok_or(std::io::Error::other(
                "2L11 expected authoritative HTML build report",
            ))?;
        eprintln!(
            "2L11 DIAG HTML recovery: recoveries={}, tokenizer_errors={}",
            report.recoveries_total, report.tokenizer_parse_errors
        );
        assert!(
            report.recoveries_total > 0,
            "malformed HTML did not exercise tree-builder recovery; recoveries={}",
            report.recoveries_total
        );
        assert!(
            report.tokenizer_parse_errors > 0,
            "duplicate attribute did not exercise tokenizer recovery; tokenizer_errors={}",
            report.tokenizer_parse_errors
        );

        let paint_fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();

        let mut status_span = Vec::new();
        let mut collecting = false;
        for (text, color) in &paint_fragments {
            if !collecting && text.contains("Recovered") {
                collecting = true;
            }
            if collecting {
                status_span.push((text.clone(), *color));
                if text.contains("state") {
                    break;
                }
            }
        }
        let status_text = status_span
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!("2L11 DIAG recovery status span={status_span:?}");

        assert!(
            status_text.contains("Recovered")
                && status_text.contains("external")
                && status_text.contains("state"),
            "could not resolve recovered status across paint fragments; fragments={paint_fragments:?}"
        );
        assert!(
            status_span
                .iter()
                .all(|(_, color)| *color == (0, 128, 0)),
            "valid CSS after malformed selector did not survive recovery; status_span={status_span:?}; painted={painted:?}"
        );

        Ok(())
    }

    // PHANTOM_2L12_NAVIGATION_LIFECYCLE_INTEGRATION_I
    #[test]
    fn navigation_lifecycle_discards_old_async_work_and_starts_fresh_runtime()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let old_url = format!("{}/lifecycle-old.html", server.address);
        let new_url = format!("{}/lifecycle-new.html", server.address);

        start_navigation(
            &network,
            &mut tab,
            old_url.clone(),
            NavigationAction::New,
        );

        let old_deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < old_deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let old_fetch_started = server.events.lock().is_ok_and(|events| {
                events
                    .iter()
                    .any(|event| event.starts_with("request: /lifecycle-old-data.txt;"))
            });
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && old_fetch_started
                && tab.engine.pending_javascript_timer_count() == 1
            {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let old_generation = tab.document_generation;
        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "old page did not reach ready state: status={}",
            tab.status
        );
        assert_eq!(
            tab.engine.pending_javascript_timer_count(),
            1,
            "old page did not retain the timer that lifecycle navigation must isolate"
        );
        let before_navigation_events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());
        assert!(
            before_navigation_events
                .iter()
                .any(|event| event.starts_with("request: /lifecycle-old-data.txt;")),
            "old slow fetch never started: {before_navigation_events:?}"
        );
        assert!(
            !before_navigation_events
                .iter()
                .any(|event| event.starts_with("response: /lifecycle-old-data.txt;")),
            "old slow fetch completed before replacement navigation; isolation gate was not exercised: {before_navigation_events:?}"
        );
        assert_eq!(
            tab.pending_script_fetches.len(),
            1,
            "old fetch receiver must still be pending when replacement navigation begins"
        );

        start_navigation(
            &network,
            &mut tab,
            new_url.clone(),
            NavigationAction::New,
        );

        assert_eq!(tab.document_generation, old_generation.saturating_add(1));
        assert_eq!(tab.navigation_phase(), NavigationPhase::Fetching);
        assert!(
            tab.pending_script_fetches.is_empty(),
            "start_navigation must detach pending fetch receivers from the previous generation"
        );
        assert!(
            tab.pending_page_scripts.is_none(),
            "start_navigation must discard pending page-script sequence from the previous generation"
        );
        assert!(
            tab.javascript_last_tick.is_none(),
            "start_navigation must stop the previous document timer clock while fetching"
        );

        let new_deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < new_deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            let old_response_completed = server.events.lock().is_ok_and(|events| {
                events
                    .iter()
                    .any(|event| event.starts_with("response: /lifecycle-old-data.txt;"))
            });
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && old_response_completed
                && painted.contains("New external ready")
                && painted.contains("New inline after external")
                && painted.contains("New fetch committed")
                && painted.contains("New timer committed")
                && painted.contains("New created node")
            {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let painted = corpus_painted_text(&tab);
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());
        eprintln!(
            "2L12 DIAG generation={} history_index={:?} history={:?} painted={painted:?}",
            tab.document_generation, tab.history_index, tab.history
        );
        eprintln!("2L12 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "new page did not reach ready state: status={}; server={events:?}",
            tab.status
        );
        assert_eq!(tab.address, new_url);
        assert_eq!(tab.title, "Phantom Lifecycle New");
        assert_eq!(tab.document_generation, old_generation.saturating_add(1));
        assert_eq!(tab.history, vec![old_url, new_url]);
        assert_eq!(tab.history_index, Some(1));
        assert!(tab.can_go_back());
        assert!(!tab.can_go_forward());

        for expected in [
            "Lifecycle new page",
            "New external ready",
            "New inline after external",
            "New fetch committed",
            "New timer committed",
            "New created node",
        ] {
            assert!(
                painted.contains(expected),
                "missing {expected:?}; painted={painted:?}; status={}; server={events:?}",
                tab.status
            );
        }
        for stale in [
            "Lifecycle old page",
            "Old fetch pending",
            "Old timer pending",
            "STALE OLD FETCH MUTATION",
            "STALE OLD TIMER MUTATION",
        ] {
            assert!(
                !painted.contains(stale),
                "previous navigation leaked into the new document: {stale:?}; painted={painted:?}"
            );
        }
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("response: /lifecycle-old-data.txt;")),
            "old response never completed, so stale-completion isolation was not exercised: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /lifecycle-new.js;")),
            "new document external script was not requested: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /lifecycle-new-data.txt;")),
            "new document fetch was not requested: {events:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);

        Ok(())
    }


    // PHANTOM_2L13_RUNTIME_INTEGRATION_STRESS_I
    #[test]
    fn runtime_integration_stress_is_deterministic_across_generations()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let mut tab = BrowserTab::new();
        let rounds = [
            ("stress-a.html", "Stress round A", "Phantom Stress A"),
            ("stress-b.html", "Stress round B", "Phantom Stress B"),
            ("stress-c.html", "Stress round C", "Phantom Stress C"),
        ];
        let mut expected_history = Vec::new();
        let mut previous_generation = tab.document_generation;

        for (page, round_label, title) in rounds {
            let network = NetworkClient::new();
            let url = format!("{}/{page}", server.address);
            expected_history.push(url.clone());
            start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

            assert_eq!(
                tab.document_generation,
                previous_generation.saturating_add(1),
                "stress navigation did not advance document generation"
            );
            previous_generation = tab.document_generation;

            let deadline = Instant::now() + Duration::from_secs(8);
            let mut peak_pending_fetches = 0_usize;
            let mut peak_pending_timers = 0_usize;
            while Instant::now() < deadline {
                poll_tab_navigation(&mut tab, &network, 1.0);
                poll_pending_page_scripts(&mut tab, 1.0);
                peak_pending_timers = peak_pending_timers
                    .max(tab.engine.pending_javascript_timer_count());
                pump_page_timers(&mut tab, Instant::now(), 1.0);
                poll_page_fetches(&mut tab, 1.0);
                peak_pending_fetches = peak_pending_fetches.max(tab.pending_script_fetches.len());

                let painted = corpus_painted_text(&tab);
                let complete = [
                    "Runtime external ready",
                    "Stress inline after external",
                    "Stress fetch 1",
                    "Stress fetch 2",
                    "Stress fetch 3",
                    "Stress fetch 4",
                    "Stress fetch 5",
                    "Stress fetch 6",
                    "Stress fetch 7",
                    "Stress timer 1",
                    "Stress timer 2",
                    "Stress timer 3",
                    "Stress timer 4",
                    "Stress timer 5",
                    "Stress timer 6",
                    "Stress timer 7",
                    "Stress node A",
                    "Stress node B",
                    "Stress node C",
                    "Stress node D",
                    "Stress node E",
                ]
                .iter()
                .all(|expected| painted.contains(expected));

                if tab.navigation_phase() == NavigationPhase::Failed {
                    break;
                }
                if tab.navigation_phase() == NavigationPhase::Ready
                    && complete
                    && tab.pending_script_fetches.is_empty()
                    && tab.pending_page_scripts.is_none()
                    && tab.engine.pending_javascript_timer_count() == 0
                {
                    break;
                }
                thread::sleep(Duration::from_millis(3));
            }

            let painted = corpus_painted_text(&tab);
            assert_eq!(
                tab.navigation_phase(),
                NavigationPhase::Ready,
                "stress round failed: round={round_label:?}; status={}",
                tab.status
            );
            assert_eq!(tab.address, url);
            assert_eq!(tab.title, title);
            assert!(
                painted.contains(round_label),
                "current stress round marker missing: {round_label:?}; painted={painted:?}"
            );
            for stale_round in ["Stress round A", "Stress round B", "Stress round C"] {
                if stale_round != round_label {
                    assert!(
                        !painted.contains(stale_round),
                        "prior stress generation leaked into current paint: stale={stale_round:?}; current={round_label:?}; painted={painted:?}"
                    );
                }
            }
            for expected in [
                "Runtime external ready",
                "Stress inline after external",
                "Stress fetch 1",
                "Stress fetch 2",
                "Stress fetch 3",
                "Stress fetch 4",
                "Stress fetch 5",
                "Stress fetch 6",
                "Stress fetch 7",
                "Stress timer 1",
                "Stress timer 2",
                "Stress timer 3",
                "Stress timer 4",
                "Stress timer 5",
                "Stress timer 6",
                "Stress timer 7",
                "Stress node A",
                "Stress node B",
                "Stress node C",
                "Stress node D",
                "Stress node E",
            ] {
                assert!(
                    painted.contains(expected),
                    "stress output missing {expected:?}; round={round_label:?}; painted={painted:?}; status={}",
                    tab.status
                );
            }
            assert!(tab.pending_script_fetches.is_empty());
            assert!(tab.pending_page_scripts.is_none());
            assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
            assert!(
                peak_pending_fetches >= 6,
                "stress gate did not create enough simultaneous fetch pressure: peak={peak_pending_fetches}; round={round_label:?}"
            );
            assert!(
                peak_pending_timers >= 6,
                "stress gate did not create enough timer pressure: peak={peak_pending_timers}; round={round_label:?}"
            );

            let paint_fragments = tab
                .engine
                .paint_list()
                .commands()
                .iter()
                .filter_map(|command| match command {
                    PaintCommand::Text { text, color, .. } => tab.engine.paint_list().text(*text).map(
                        |value| {
                            (
                                value.split_whitespace().collect::<Vec<_>>().join(" "),
                                (color.red(), color.green(), color.blue()),
                            )
                        },
                    ),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let mut status_span = Vec::new();
            let mut collecting = false;
            for (text, color) in &paint_fragments {
                if !collecting && text.contains("Runtime") {
                    collecting = true;
                }
                if collecting {
                    status_span.push((text.clone(), *color));
                    if text.contains("ready") {
                        break;
                    }
                }
            }
            let status_text = status_span
                .iter()
                .map(|(text, _)| text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!(
                "2L13 DIAG round={round_label:?} generation={} peak_fetches={peak_pending_fetches} peak_timers={peak_pending_timers} pending_fetches={} pending_timers={} status_span={status_span:?}",
                tab.document_generation,
                tab.pending_script_fetches.len(),
                tab.engine.pending_javascript_timer_count()
            );
            assert!(
                status_text.contains("Runtime") && status_text.contains("ready"),
                "stress status could not be resolved across paint fragments: {paint_fragments:?}"
            );
            assert!(
                status_span.iter().all(|(_, color)| *color == (0, 0, 255)),
                "async style mutation lost under runtime stress: status_span={status_span:?}; painted={painted:?}"
            );
        }

        assert_eq!(tab.history, expected_history);
        assert_eq!(tab.history_index, Some(2));
        assert!(tab.can_go_back());
        assert!(!tab.can_go_forward());

        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());
        for resource in [
            "/stress-runtime.js",
            "/stress-data-1.txt",
            "/stress-data-2.txt",
            "/stress-data-3.txt",
            "/stress-data-4.txt",
            "/stress-data-5.txt",
            "/stress-data-6.txt",
            "/stress-data-7.txt",
        ] {
            let requests = events
                .iter()
                .filter(|event| event.starts_with(&format!("request: {resource};")))
                .count();
            assert_eq!(
                requests, 3,
                "stress resource must be exercised once per generation: resource={resource}; events={events:?}"
            );
        }
        eprintln!(
            "2L13 DIAG final generation={} history={:?} server_events={}",
            tab.document_generation,
            tab.history,
            events.len()
        );

        Ok(())
    }

    // PHANTOM_2L14_RUNTIME_INTEGRATION_FINAL_GATE_I
    #[test]
    fn runtime_integration_final_gate_mixes_recovery_navigation_and_stress()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let mut tab = BrowserTab::new();
        let stages: [(&str, &str, &str, &[&str]); 5] = [
            (
                "integrated.html",
                "Phantom 2L10 Integrated Corpus",
                "Phantom integrated corpus",
                &[
                    "External ready",
                    "Inline after external",
                    "Fetched integration data",
                    "Timer integration ready",
                    "Created integration node",
                ],
            ),
            (
                "recovery.html",
                "Phantom 2L11 Recovery Corpus",
                "Phantom recovery corpus",
                &[
                    "Recovered external state",
                    "Recovery inline after external",
                    "Recovered fetch data",
                    "Recovery timer ready",
                    "Recovery created node",
                ],
            ),
            (
                "lifecycle-new.html",
                "Phantom Lifecycle New",
                "Lifecycle new page",
                &[
                    "New external ready",
                    "New inline after external",
                    "New fetch committed",
                    "New timer committed",
                    "New created node",
                ],
            ),
            (
                "stress-a.html",
                "Phantom Stress A",
                "Stress round A",
                &[
                    "Runtime external ready",
                    "Stress inline after external",
                    "Stress fetch 7",
                    "Stress timer 7",
                    "Stress node E",
                ],
            ),
            (
                "integrated.html",
                "Phantom 2L10 Integrated Corpus",
                "Phantom integrated corpus",
                &[
                    "External ready",
                    "Inline after external",
                    "Fetched integration data",
                    "Timer integration ready",
                    "Created integration node",
                ],
            ),
        ];
        let generation_markers = [
            "Phantom integrated corpus",
            "Phantom recovery corpus",
            "Lifecycle new page",
            "Stress round A",
        ];
        let mut expected_history = Vec::new();
        let mut previous_generation = tab.document_generation;

        for (stage_index, &(page, title, primary_marker, expected)) in stages.iter().enumerate() {
            let network = NetworkClient::new();
            let url = format!("{}/{page}", server.address);
            expected_history.push(url.clone());
            start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

            assert_eq!(
                tab.document_generation,
                previous_generation.saturating_add(1),
                "2L14 stage {stage_index} did not advance document generation"
            );
            previous_generation = tab.document_generation;

            let deadline = Instant::now() + Duration::from_secs(8);
            let mut peak_pending_fetches = 0_usize;
            let mut peak_pending_timers = 0_usize;
            while Instant::now() < deadline {
                poll_tab_navigation(&mut tab, &network, 1.0);
                poll_pending_page_scripts(&mut tab, 1.0);
                peak_pending_timers = peak_pending_timers
                    .max(tab.engine.pending_javascript_timer_count());
                pump_page_timers(&mut tab, Instant::now(), 1.0);
                poll_page_fetches(&mut tab, 1.0);
                peak_pending_fetches = peak_pending_fetches.max(tab.pending_script_fetches.len());

                let painted = corpus_painted_text(&tab);
                let complete = painted.contains(primary_marker)
                    && expected.iter().all(|item| painted.contains(item));
                if tab.navigation_phase() == NavigationPhase::Failed {
                    break;
                }
                if tab.navigation_phase() == NavigationPhase::Ready
                    && complete
                    && tab.pending_script_fetches.is_empty()
                    && tab.pending_page_scripts.is_none()
                    && tab.engine.pending_javascript_timer_count() == 0
                {
                    break;
                }
                thread::sleep(Duration::from_millis(3));
            }

            let painted = corpus_painted_text(&tab);
            assert_eq!(
                tab.navigation_phase(),
                NavigationPhase::Ready,
                "2L14 stage failed: index={stage_index}; page={page}; status={}",
                tab.status
            );
            assert_eq!(tab.address, url);
            assert_eq!(tab.title, title);
            assert!(
                painted.contains(primary_marker),
                "2L14 primary marker missing: stage={stage_index}; marker={primary_marker:?}; painted={painted:?}"
            );
            for item in expected {
                assert!(
                    painted.contains(item),
                    "2L14 output missing: stage={stage_index}; item={item:?}; painted={painted:?}"
                );
            }
            for stale_marker in generation_markers {
                if stale_marker != primary_marker {
                    assert!(
                        !painted.contains(stale_marker),
                        "2L14 stale generation leaked into paint: stage={stage_index}; stale={stale_marker:?}; painted={painted:?}"
                    );
                }
            }
            assert!(tab.pending_script_fetches.is_empty());
            assert!(tab.pending_page_scripts.is_none());
            assert_eq!(tab.engine.pending_javascript_timer_count(), 0);

            if page == "recovery.html" {
                let report = tab
                    .engine
                    .html_build_report()
                    .ok_or(std::io::Error::other(
                        "2L14 expected HTML recovery report",
                    ))?;
                assert!(report.recoveries_total > 0);
                assert!(report.tokenizer_parse_errors > 0);
            }
            if page == "stress-a.html" {
                assert!(
                    peak_pending_fetches >= 6,
                    "2L14 final gate did not exercise enough fetch pressure: peak={peak_pending_fetches}"
                );
                assert!(
                    peak_pending_timers >= 6,
                    "2L14 final gate did not exercise enough timer pressure: peak={peak_pending_timers}"
                );
            }

            eprintln!(
                "2L14 DIAG stage={stage_index} page={page:?} generation={} peak_fetches={peak_pending_fetches} peak_timers={peak_pending_timers} history_index={:?} pending_fetches={} pending_timers={} painted={painted:?}",
                tab.document_generation,
                tab.history_index,
                tab.pending_script_fetches.len(),
                tab.engine.pending_javascript_timer_count()
            );
        }

        assert_eq!(tab.document_generation, 5);
        assert_eq!(tab.history, expected_history);
        assert_eq!(tab.history_index, Some(4));
        assert!(tab.can_go_back());
        assert!(!tab.can_go_forward());
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);

        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());
        for resource in [
            "/integrated-app.js",
            "/integrated-data.txt",
            "/recovery-app.js",
            "/recovery-data.txt",
            "/lifecycle-new.js",
            "/lifecycle-new-data.txt",
            "/stress-runtime.js",
            "/stress-data-7.txt",
        ] {
            assert!(
                events
                    .iter()
                    .any(|event| event.starts_with(&format!("request: {resource};"))),
                "2L14 final gate did not observe required request: {resource}; events={events:?}"
            );
        }
        eprintln!(
            "2L14 DIAG final generation={} history_len={} history_index={:?} server_events={}",
            tab.document_generation,
            tab.history.len(),
            tab.history_index,
            events.len()
        );

        Ok(())
    }

    #[test]
    fn alpha10_tab_width_keeps_full_size_when_space_is_available() {
        let width = regular_tab_width_for(1600.0, 0, 3, false);

        assert!((width - REGULAR_TAB_MAX_WIDTH).abs() <= f32::EPSILON);
    }

    #[test]
    fn alpha10_tab_width_responds_to_strip_pressure() {
        let roomy = regular_tab_width_for(1200.0, 0, 5, false);
        let pressured = regular_tab_width_for(800.0, 0, 5, false);
        let pinned_pressure = regular_tab_width_for(800.0, 2, 5, false);

        assert!(pressured < roomy);
        assert!(pinned_pressure < pressured);
        assert!(pinned_pressure >= REGULAR_TAB_MIN_WIDTH);
    }

    #[test]
    fn alpha10_recently_closed_control_is_reserved_in_tab_budget() {
        let without_history = regular_tab_width_for(800.0, 0, 5, false);
        let with_history = regular_tab_width_for(800.0, 0, 5, true);

        assert!(with_history < without_history);
    }

    #[test]
    fn alpha10_compact_title_remains_unicode_safe() {
        let title = compact_tab_title("Navegação segura", 30.0);

        assert!(title.ends_with('…'));
        assert!(title.starts_with("Nave"));
    }

    #[test]
    fn alpha12_regular_tab_keeps_icon_slot_without_close_button() {
        assert!((regular_tab_title_reserve(false) - 28.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn alpha12_regular_tab_reserves_close_button_without_moving_icon_slot() {
        assert!((regular_tab_title_reserve(true) - 56.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn alpha14_active_indicator_is_inset_inside_tab_surface() {
        let tab = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(120.0, 30.0));
        let indicator = active_tab_indicator_rect(tab);

        assert!((indicator.left() - 16.0).abs() < f32::EPSILON);
        assert!((indicator.right() - 124.0).abs() < f32::EPSILON);
        assert!((indicator.height() - TAB_ACTIVE_INDICATOR_HEIGHT).abs() < f32::EPSILON);
        assert_eq!(indicator.bottom(), tab.bottom());
    }

    #[test]
    fn alpha16_tab_surface_uses_chrome_fill_without_button_pill() {
        let panel = egui::Color32::from_rgb(21, 22, 24);
        assert_eq!(tab_surface_fill(panel), panel);
    }

    #[test]
    fn alpha17_web_text_fragments_are_not_rewrapped_by_native_ui() {
        assert_eq!(web_text_wrap_mode(), egui::TextWrapMode::Extend);
    }

    #[test]
    fn image_composition_snaps_fractional_geometry_to_device_pixels() {
        let rect = egui::Rect::from_min_max(egui::pos2(10.24, 20.26), egui::pos2(110.26, 70.74));
        let snapped = snap_rect_to_device_pixels(rect, 2.0);

        assert_eq!(snapped.min, egui::pos2(10.0, 20.5));
        assert_eq!(snapped.max, egui::pos2(110.5, 70.5));
    }

    #[test]
    fn image_composition_sanitizes_invalid_device_scale() {
        let rect = egui::Rect::from_min_max(egui::pos2(0.4, 0.4), egui::pos2(20.6, 10.6));

        assert_eq!(
            snap_rect_to_device_pixels(rect, f32::NAN),
            snap_rect_to_device_pixels(rect, 1.0)
        );
    }

    #[test]
    fn fixed_paint_origin_tracks_viewport_offset_only_for_viewport_attachment() {
        let origin = egui::pos2(20.0, 30.0);
        let offset = egui::vec2(40.0, 120.0);

        assert_eq!(
            paint_command_origin(origin, offset, PaintAttachment::Document),
            origin
        );
        assert_eq!(
            paint_command_origin(origin, offset, PaintAttachment::Viewport),
            egui::pos2(60.0, 150.0)
        );
    }

    #[test]
    fn viewport_relayout_threshold_ignores_subpixel_noise() {
        assert!(!layout_viewport_changed(800.0, 600.0, 800.25, 599.75));
        assert!(layout_viewport_changed(800.0, 600.0, 801.0, 600.0));
        assert!(layout_viewport_changed(800.0, 600.0, 800.0, 601.0));
    }

    #[test]
    fn alpha13_pinned_and_regular_tabs_share_identity_visual_size() {
        assert!((TAB_IDENTITY_ICON_SIZE - 16.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn image_batch_drop_signals_cancellation() {
        let (_sender, receiver) = mpsc::channel::<ImageLoadEvent>();
        let cancelled = Arc::new(AtomicBool::new(false));

        {
            let _pending = PendingImageBatch {
                receiver,
                remaining: 1,
                total: 1,
                generation: 7,
                cancelled: Arc::clone(&cancelled),
            };
        }

        assert!(cancelled.load(Ordering::Acquire));
    }

    #[test]
    fn animation_clock_freezes_while_inactive() {
        let started = Instant::now();
        let animation = AnimatedTexture {
            frames: Vec::new(),
            loop_count: AnimationLoopCount::Infinite,
            cycle_duration: Duration::from_millis(100),
            clock: Mutex::new(AnimationClock {
                elapsed: Duration::ZERO,
                last_updated: started,
                active: false,
            }),
        };

        animation.set_active(true, started);
        let paused_at = started + Duration::from_millis(40);
        animation.set_active(false, paused_at);

        assert_eq!(
            animation.elapsed_at(paused_at + Duration::from_secs(5)),
            Duration::from_millis(40),
        );
    }

    #[test]
    fn https_navigation_identity_uses_canonical_typed_origin() {
        let mut tab = BrowserTab::new();
        tab.mark_navigation_ready();
        tab.history = vec!["https://EXAMPLE.com:443/path".to_owned()];
        tab.history_index = Some(0);

        let identity = navigation_origin_identity(&tab);

        assert!(identity.as_ref().is_some_and(|(secure, tooltip)| {
            *secure && tooltip.contains("https://example.com")
        }));
    }

    #[test]
    fn http_navigation_identity_is_marked_unencrypted() {
        let mut tab = BrowserTab::new();
        tab.mark_navigation_ready();
        tab.history = vec!["http://example.com/path".to_owned()];
        tab.history_index = Some(0);

        let identity = navigation_origin_identity(&tab);

        assert!(
            identity.as_ref().is_some_and(|(secure, tooltip)| {
                !*secure && tooltip.contains("sem criptografia")
            })
        );
    }

    #[test]
    fn committed_page_scripts_update_page_before_title_and_rendering()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut tab = BrowserTab::new();
        tab.engine.load_html(
            "<title>Before</title><p id='target'>Before</p><script>document.getElementById('target').textContent = 'After';</script>",
        )?;
        assert_eq!(
            execute_committed_page_scripts(&mut tab, "https://example.test/", 1.0)?,
            1
        );
        assert!(
            tab.engine
                .paint_list()
                .commands()
                .iter()
                .any(|command| match command {
                    PaintCommand::Text { text, .. } => tab
                        .engine
                        .paint_list()
                        .text(*text)
                        .is_some_and(|value| value.contains("After")),
                    _ => false,
                })
        );
        Ok(())
    }

    #[test]
    fn page_timer_pump_updates_dom_and_resets_on_navigation()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut tab = BrowserTab::new();
        let url = "https://example.test/";
        tab.engine.load_html("<p id='target'>Before</p><script>setTimeout(function() { document.getElementById('target').textContent = 'After'; }, 40);</script>")?;
        tab.address = url.to_owned();
        assert_eq!(execute_committed_page_scripts(&mut tab, url, 1.0)?, 1);
        assert_eq!(tab.engine.pending_javascript_timer_count(), 1);
        tab.mark_navigation_ready();
        let start = Instant::now();
        tab.javascript_last_tick = Some(start);
        pump_page_timers(&mut tab, start + Duration::from_millis(20), 1.0);
        assert_eq!(tab.engine.pending_javascript_timer_count(), 1);
        pump_page_timers(&mut tab, start + Duration::from_millis(50), 1.0);
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        assert!(tab.engine.paint_list().commands().iter().any(|command| match command {
            PaintCommand::Text { text, .. } => tab.engine.paint_list().text(*text).is_some_and(|value| value.contains("After")),
            _ => false,
        }));
        tab.clear_navigation_state();
        tab.engine.load_html("<p>Next page</p>")?;
        assert!(tab.javascript_last_tick.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }

    #[test]
    fn page_fetch_cross_origin_requires_cors_response()
    -> Result<(), Box<dyn std::error::Error>> {
        let origin_server = TextSubresourcePolicyServer::new()?;
        let document_url = format!("{}/2n5-page.html", origin_server.address);
        let cors_server = CorsTransportServer::new(HttpUrl::parse(&document_url)?.origin().as_str().to_owned())?;
        let denied_url = format!("{}/2n7-no-acao.txt", cors_server.address);

        let mut tab = BrowserTab::new();
        tab.engine.load_html(&format!(
            "<p id='target'>Before</p><script>fetch('{denied_url}').then(function() {{ document.getElementById('target').textContent = 'LEAKED'; }}).catch(function() {{ document.getElementById('target').textContent = 'Blocked'; }});</script>"
        ))?;
        tab.address = document_url.clone();
        execute_committed_page_scripts(&mut tab, &document_url, 1.0)?;
        tab.mark_navigation_ready();

        for _ in 0..120 {
            poll_page_fetches(&mut tab, 1.0);
            if corpus_painted_text(&tab).contains("Blocked") {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        assert!(painted.contains("Blocked"), "{painted}");
        assert!(!painted.contains("LEAKED"), "{painted}");
        assert!(tab.pending_script_fetches.is_empty());
        Ok(())
    }

    #[test]
    fn old_fetch_completion_is_discarded_after_navigation()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut tab = BrowserTab::new();
        tab.engine.load_html("<p>Next</p>")?;
        tab.address = "https://example.test/next".to_owned();
        tab.document_generation = 2;
        tab.mark_navigation_ready();
        let (sender, receiver) = mpsc::channel();
        sender.send(Err(NetworkError::Request("old result".to_owned())))?;
        tab.pending_script_fetches.push(PendingScriptFetch { request_id: 1, generation: 1, receiver });
        poll_page_fetches(&mut tab, 1.0);
        assert!(tab.pending_script_fetches.is_empty());
        assert!(!tab.status.contains("old result"));
        Ok(())
    }

    #[test]
    fn external_script_policy_preserves_surrounding_inline_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut tab = BrowserTab::new();
        let url = "https://example.test/page";
        tab.engine.load_html("<p id='target'>Before</p><script>var value = 'first';</script><script src='file:///invalid.js'></script><script>document.getElementById('target').textContent = value;</script>")?;
        tab.address = url.to_owned();
        assert_eq!(start_page_scripts(&mut tab, url, 1.0)?, 2);
        assert!(tab.pending_page_scripts.is_none());
        assert!(tab.engine.paint_list().commands().iter().any(|command| match command {
            PaintCommand::Text { text, .. } => tab.engine.paint_list().text(*text).is_some_and(|value| value.contains("first")),
            _ => false,
        }));
        Ok(())
    }

    #[test]
    fn navigation_state_starts_empty_and_has_single_phase_source() {
        let tab = BrowserTab::new();

        assert_eq!(tab.navigation_phase(), NavigationPhase::Empty);
        assert!(!tab.is_loading());
        assert!(!tab.has_committed_document());
        assert!(tab.navigation_error().is_none());
    }

    #[test]
    fn navigation_state_transitions_fetch_parse_ready() {
        let (_sender, receiver) = mpsc::channel::<Result<DocumentResponse, DocumentLoadError>>();
        let mut tab = BrowserTab::new();

        tab.begin_fetching(PendingNavigation {
            receiver,
            action: NavigationAction::Reload,
            generation: 3,
        });

        assert_eq!(tab.navigation_phase(), NavigationPhase::Fetching);
        assert!(tab.is_loading());
        assert_eq!(tab.loading_action(), Some(NavigationAction::Reload));

        tab.begin_parsing(NavigationAction::Reload);

        assert_eq!(tab.navigation_phase(), NavigationPhase::Parsing);
        assert!(tab.is_loading());

        tab.mark_navigation_ready();

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert!(!tab.is_loading());
        assert!(tab.has_committed_document());
        assert!(tab.navigation_error().is_none());
    }

    #[test]
    fn navigation_failure_owns_the_document_error() {
        let mut tab = BrowserTab::new();

        tab.fail_navigation(DocumentPageError::new("Falha de teste", "erro controlado"));

        assert_eq!(tab.navigation_phase(), NavigationPhase::Failed);
        assert!(!tab.has_committed_document());
        assert!(
            tab.navigation_error()
                .is_some_and(|error| error.title == "Falha de teste")
        );
    }

    #[test]
    fn eager_images_are_prioritized_before_lazy_images() -> Result<(), Box<dyn std::error::Error>> {
        let mut tab = BrowserTab::new();
        tab.engine
            .load_html("<img src=\"lazy.png\" loading=\"lazy\"><img src=\"eager.png\">")?;
        let base_url = HttpUrl::parse("https://example.com/page")?;

        let requests = collect_image_requests(&mut tab, &base_url, 1.0);

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].loading, ImageLoading::Eager);
        assert_eq!(requests[1].loading, ImageLoading::Lazy);

        Ok(())
    }
    #[test]
    fn fetchpriority_orders_images_before_loading_distance()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut tab = BrowserTab::new();
        tab.engine.load_html(
            "<img src=\"low.png\" fetchpriority=\"low\"><img src=\"auto.png\"><img src=\"high.png\" fetchpriority=\"high\">",
        )?;
        let base_url = HttpUrl::parse("https://example.com/page")?;

        let requests = collect_image_requests(&mut tab, &base_url, 1.0);

        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].priority, ResourcePriority::High);
        assert_eq!(requests[1].priority, ResourcePriority::Auto);
        assert_eq!(requests[2].priority, ResourcePriority::Low);

        Ok(())
    }

    #[test]
    fn preload_is_scheduled_before_same_priority_image() -> Result<(), Box<dyn std::error::Error>> {
        let mut tab = BrowserTab::new();
        tab.engine.load_html(
            "<link rel=\"preload\" as=\"image\" href=\"hero.png\" fetchpriority=\"high\"><img src=\"visible.png\" fetchpriority=\"high\">",
        )?;
        let base_url = HttpUrl::parse("https://example.com/page")?;

        let mut requests = collect_preload_requests(&tab, &base_url, 1.0);
        requests.extend(collect_image_requests(&mut tab, &base_url, 1.0));
        requests.sort_by(resource_request_order);

        assert_eq!(requests.len(), 2);
        assert!(requests[0].preload_only);
        assert!(!requests[1].preload_only);

        Ok(())
    }

    // PHANTOM_2M1_PROMISE_ASSIMILATION_COMPATIBILITY_I
    #[test]
    fn promise_returned_from_then_is_assimilated_across_fetch_response_text()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/promise-assimilation.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        let mut peak_pending_fetches = 0_usize;
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);
            peak_pending_fetches = peak_pending_fetches.max(tab.pending_script_fetches.len());

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Promise assimilation OK")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();

        // PHANTOM_2M1_R2_FRAGMENT_AWARE_PAINT_GATE_I
        // Paint may split one DOM text node into several commands or keep
        // multiple words in one command. Search for the exact ordered status
        // span instead of filtering individual fragments by token equality.
        let target_status = "Promise assimilation OK";
        let mut status_span = Vec::new();
        'candidate: for (start, (first_text, _)) in fragments.iter().enumerate() {
            if !first_text.starts_with("Promise") {
                continue;
            }

            let mut candidate = Vec::new();
            for (text, color) in fragments.iter().skip(start) {
                candidate.push((text.clone(), *color));
                let candidate_text = candidate
                    .iter()
                    .map(|(text, _)| text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");

                if candidate_text == target_status {
                    status_span = candidate;
                    break 'candidate;
                }
                if candidate_text.len() > target_status.len() {
                    break;
                }
            }
        }
        let status_text = status_span
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M1 DIAG generation={} peak_fetches={} pending_fetches={} pending_timers={} painted={painted:?}",
            tab.document_generation,
            peak_pending_fetches,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count()
        );
        eprintln!("2M1 DIAG status span={status_span:?}");
        eprintln!("2M1 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M1 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            peak_pending_fetches >= 1,
            "2M1 did not exercise a real fetch: events={events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /promise-data.txt;")),
            "2M1 promise data request was not observed: events={events:?}"
        );
        assert!(
            painted.contains("Promise assimilation OK"),
            "2M1 returned-Promise assimilation failed; expected resolved text, painted={painted:?}"
        );
        assert!(
            !painted.contains("[object Object]"),
            "2M1 chained .then received the Promise object instead of its fulfillment value: painted={painted:?}"
        );
        assert_eq!(
            status_text,
            target_status,
            "2M1 could not resolve the full status across PaintCommand fragments; fragments={fragments:?}"
        );
        assert!(
            status_span
                .iter()
                .all(|(_, color)| *color == (128, 0, 128)),
            "2M1 resolved second .then did not color every status Paint fragment purple; status_span={status_span:?}; fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }

    // PHANTOM_2M2_PROMISE_RESOLUTION_PROCEDURE_II
    #[test]
    fn promise_resolution_adopts_pending_chain_and_recovers_returned_rejection()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/promise-resolution.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        let mut peak_pending_fetches = 0_usize;
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);
            peak_pending_fetches = peak_pending_fetches.max(tab.pending_script_fetches.len());

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Pending promise adopted")
                && painted.contains("Recovered")
                && painted.contains("sync-tail-first-first-text-second-catch-expected rejection")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();

        let target_status = "Pending promise adopted";
        let mut status_span = Vec::new();
        'candidate: for (start, (first_text, _)) in fragments.iter().enumerate() {
            if !first_text.starts_with("Pending") {
                continue;
            }
            let mut candidate = Vec::new();
            for (text, color) in fragments.iter().skip(start) {
                candidate.push((text.clone(), *color));
                let candidate_text = candidate
                    .iter()
                    .map(|(text, _)| text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                if candidate_text == target_status {
                    status_span = candidate;
                    break 'candidate;
                }
                if candidate_text.len() > target_status.len() {
                    break;
                }
            }
        }
        let status_text = status_span
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M2 DIAG generation={} peak_fetches={} pending_fetches={} pending_timers={} painted={painted:?}",
            tab.document_generation,
            peak_pending_fetches,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count()
        );
        eprintln!("2M2 DIAG status span={status_span:?}");
        eprintln!("2M2 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M2 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            peak_pending_fetches >= 1,
            "2M2 did not exercise pending fetch-backed Promise adoption: events={events:?}"
        );
        for request in ["/promise-first.txt", "/promise-second.txt"] {
            assert!(
                events
                    .iter()
                    .any(|event| event.starts_with(&format!("request: {request};"))),
                "2M2 did not observe {request}: events={events:?}"
            );
        }
        assert!(
            painted.contains("First promise value First stage"),
            "2M2 first Response.text Promise did not resolve: painted={painted:?}"
        );
        assert!(
            painted.contains(target_status),
            "2M2 pending returned Promise was not adopted: painted={painted:?}"
        );
        assert!(
            painted.contains("Recovered"),
            "2M2 returned rejected Promise did not recover through catch: painted={painted:?}"
        );
        assert!(
            painted.contains("sync-tail-first-first-text-second-catch-expected rejection"),
            "2M2 Promise/microtask ordering or rejection reason propagation diverged: painted={painted:?}"
        );
        assert!(
            !painted.contains("[object Object]"),
            "2M2 leaked a Promise object as a fulfillment value: painted={painted:?}"
        );
        assert_eq!(
            status_text,
            target_status,
            "2M2 could not resolve the status across Paint fragments: fragments={fragments:?}"
        );
        assert!(
            status_span
                .iter()
                .all(|(_, color)| *color == (0, 128, 0)),
            "2M2 adopted pending Promise did not reach Paint in green: status_span={status_span:?}; fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }

    // PHANTOM_2M3_PROMISE_CYCLE_SAFETY_I
    #[test]
    fn promise_resolution_rejects_indirect_cycles_and_keeps_non_callable_then_plain()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/promise-cycle.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Cycle rejected promise resolution cycle")
                && painted.contains("Plain then property kept")
                && painted.contains("Cycle recovered")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();

        let target_status = "Cycle rejected promise resolution cycle";
        let mut status_span = Vec::new();
        'candidate: for (start, (first_text, _)) in fragments.iter().enumerate() {
            if !first_text.starts_with("Cycle") {
                continue;
            }
            let mut candidate = Vec::new();
            for (text, color) in fragments.iter().skip(start) {
                candidate.push((text.clone(), *color));
                let candidate_text = candidate
                    .iter()
                    .map(|(text, _)| text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                if candidate_text == target_status {
                    status_span = candidate;
                    break 'candidate;
                }
                if candidate_text.len() > target_status.len() {
                    break;
                }
            }
        }
        let status_text = status_span
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M3 DIAG generation={} pending_fetches={} pending_timers={} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count()
        );
        eprintln!("2M3 DIAG cycle span={status_span:?}");
        eprintln!("2M3 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M3 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /promise-cycle.js;")),
            "2M3 external cycle script was not requested: events={events:?}"
        );
        assert!(
            painted.contains(target_status),
            "2M3 indirect Promise cycle was not deterministically rejected: painted={painted:?}"
        );
        assert!(
            painted.contains("Cycle recovered"),
            "2M3 rejected cycle did not recover through catch/then: painted={painted:?}"
        );
        assert!(
            painted.contains("Plain then property kept"),
            "2M3 non-callable then property was not preserved as a plain value: painted={painted:?}"
        );
        assert!(
            painted.contains("sync-tail-first-second-catch-promise resolution cycle-Cycle recovered"),
            "2M3 microtask/cycle recovery order diverged: painted={painted:?}"
        );
        assert_eq!(
            status_text,
            target_status,
            "2M3 could not resolve cycle status across Paint fragments: fragments={fragments:?}"
        );
        assert!(
            status_span
                .iter()
                .all(|(_, color)| *color == (176, 0, 32)),
            "2M3 cycle rejection did not reach Paint in expected red: status_span={status_span:?}; fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }

    // PHANTOM_2M4_THENABLE_ASSIMILATION_I
    // PHANTOM_2M4_R1_THENABLE_JOB_ISOLATION_I
    #[test]
    fn promise_resolution_assimilates_callable_thenables_and_first_settlement_wins()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/thenable-assimilation.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Async thenable adopted")
                && painted.contains("Returned thenable adopted")
                && painted.contains("Thenable rejected")
                && painted.contains("Reject recovered")
                && painted.contains("First settlement wins")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();

        let target_status = "Async thenable adopted";
        let mut status_span = Vec::new();
        'candidate: for (start, (first_text, _)) in fragments.iter().enumerate() {
            if !first_text.starts_with("Async") {
                continue;
            }
            let mut candidate = Vec::new();
            for (text, color) in fragments.iter().skip(start) {
                candidate.push((text.clone(), *color));
                let candidate_text = candidate
                    .iter()
                    .map(|(text, _)| text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                if candidate_text == target_status {
                    status_span = candidate;
                    break 'candidate;
                }
                if candidate_text.len() > target_status.len() {
                    break;
                }
            }
        }
        let status_text = status_span
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M4 DIAG generation={} pending_fetches={} pending_timers={} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count()
        );
        eprintln!("2M4 DIAG async thenable span={status_span:?}");
        eprintln!("2M4 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M4 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /thenable-assimilation.js;")),
            "2M4 external thenable script was not requested: events={events:?}"
        );
        assert!(
            painted.contains("Async thenable adopted"),
            "2M4 Promise.resolve did not assimilate async callable thenable: painted={painted:?}"
        );
        assert!(
            painted.contains("Returned thenable adopted"),
            "2M4 .then callback returned thenable was not assimilated: painted={painted:?}"
        );
        assert!(
            painted.contains("Thenable rejected") && painted.contains("Reject recovered"),
            "2M4 rejected thenable did not propagate through catch/recovery: painted={painted:?}"
        );
        assert!(
            painted.contains("First settlement wins") && !painted.contains("Late reject won"),
            "2M4 first-settlement-wins invariant failed: painted={painted:?}"
        );
        assert!(
            painted.contains("sync-tail-then-fulfilled"),
            "2M4 thenable job did not run after the synchronous tail and before fulfillment: painted={painted:?}"
        );
        assert_eq!(
            status_text,
            target_status,
            "2M4 could not resolve async thenable status across Paint fragments: fragments={fragments:?}"
        );
        assert!(
            status_span
                .iter()
                .all(|(_, color)| *color == (0, 96, 100)),
            "2M4 async thenable result did not reach Paint in expected teal: status_span={status_span:?}; fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }

    // PHANTOM_2M5_THENABLE_RESOLUTION_PROCEDURE_II
    #[test]
    fn promise_resolution_flattens_nested_thenables_and_isolates_job_errors()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/thenable-resolution.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Nested thenable final")
                && painted.contains("Self thenable rejected Self recovered")
                && painted.contains("Thenable job error rejected Error recovered")
                && painted.contains("sync-tail-outer-inner-fulfilled")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();

        let target_nested = "Nested thenable final";
        let mut nested_span = Vec::new();
        'candidate: for (start, (first_text, _)) in fragments.iter().enumerate() {
            if !first_text.starts_with("Nested") {
                continue;
            }
            let mut candidate = Vec::new();
            for (text, color) in fragments.iter().skip(start) {
                candidate.push((text.clone(), *color));
                let candidate_text = candidate
                    .iter()
                    .map(|(text, _)| text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                if candidate_text == target_nested {
                    nested_span = candidate;
                    break 'candidate;
                }
                if candidate_text.len() > target_nested.len() {
                    break;
                }
            }
        }
        let nested_text = nested_span
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M5 DIAG generation={} pending_fetches={} pending_timers={} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count()
        );
        eprintln!("2M5 DIAG nested span={nested_span:?}");
        eprintln!("2M5 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M5 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /thenable-resolution.js;")),
            "2M5 external thenable resolution script was not requested: events={events:?}"
        );
        assert!(
            painted.contains("Nested thenable final"),
            "2M5 nested thenable did not recursively resolve to final value: painted={painted:?}"
        );
        assert!(
            painted.contains("Self thenable rejected Self recovered"),
            "2M5 self-resolving thenable did not reject and recover: painted={painted:?}"
        );
        assert!(
            painted.contains("Thenable job error rejected Error recovered"),
            "2M5 thenable invocation error did not become a recoverable rejection: painted={painted:?}"
        );
        assert!(
            painted.contains("sync-tail-outer-inner-fulfilled"),
            "2M5 nested thenable job ordering is incorrect: painted={painted:?}"
        );
        assert_eq!(
            nested_text,
            target_nested,
            "2M5 could not resolve nested thenable text across Paint fragments: fragments={fragments:?}"
        );
        assert!(
            nested_span
                .iter()
                .all(|(_, color)| *color == (46, 125, 50)),
            "2M5 nested thenable result did not reach Paint in expected green: nested_span={nested_span:?}; fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M6_PROMISE_THENABLE_COMPATIBILITY_FINAL_GATE_I
    #[test]
    fn promise_thenable_compatibility_final_gate_integrates_fetch_resolution_and_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/promise-final.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        let mut peak_fetches = 0usize;
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);
            peak_fetches = peak_fetches.max(tab.pending_script_fetches.len());

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Final fetch text")
                && painted.contains("Nested final value")
                && painted.contains("Recovered final rejection")
                && painted.contains("First final settlement")
                && painted.contains("sync-tail-fetch-text-outer-inner-nested-catch-noisy-done")
                && painted.contains("Promise Thenable final gate GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();

        let target_final = "Promise Thenable final gate GREEN";
        let mut final_span = Vec::new();
        'candidate: for (start, (first_text, _)) in fragments.iter().enumerate() {
            if !first_text.starts_with("Promise") {
                continue;
            }
            let mut candidate = Vec::new();
            for (text, color) in fragments.iter().skip(start) {
                candidate.push((text.clone(), *color));
                let candidate_text = candidate
                    .iter()
                    .map(|(text, _)| text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                if candidate_text == target_final {
                    final_span = candidate;
                    break 'candidate;
                }
                if candidate_text.len() > target_final.len() {
                    break;
                }
            }
        }
        let final_text = final_span
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M6 DIAG generation={} peak_fetches={} pending_fetches={} pending_timers={} painted={painted:?}",
            tab.document_generation,
            peak_fetches,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count()
        );
        eprintln!("2M6 DIAG final span={final_span:?}");
        eprintln!("2M6 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M6 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            peak_fetches >= 1,
            "2M6 never observed the page fetch Promise as pending: peak_fetches={peak_fetches}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /promise-final.js;")),
            "2M6 external final-gate script was not requested: events={events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /promise-final-data.txt;")),
            "2M6 fetch resource was not requested: events={events:?}"
        );
        assert!(painted.contains("Final fetch text"));
        assert!(painted.contains("Nested final value"));
        assert!(painted.contains("Recovered final rejection"));
        assert!(painted.contains("First final settlement"));
        assert!(!painted.contains("Late final reject"));
        assert!(
            painted.contains("sync-tail-fetch-text-outer-inner-nested-catch-noisy-done"),
            "2M6 Promise/thenable job order diverged: painted={painted:?}"
        );
        assert_eq!(
            final_text,
            target_final,
            "2M6 could not resolve final status across Paint fragments: fragments={fragments:?}"
        );
        assert!(
            final_span
                .iter()
                .all(|(_, color)| *color == (27, 94, 32)),
            "2M6 final status did not reach Paint in expected green: final_span={final_span:?}; fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M7_FUNCTION_CONTEXT_COMPATIBILITY_I
    #[test]
    fn javascript_function_context_supports_this_closure_and_async_capture()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/function-context.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        let mut peak_timers = 0usize;
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            peak_timers = peak_timers.max(tab.engine.pending_javascript_timer_count());
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Closure captured")
                && painted.contains("This bound")
                && painted.contains("Closure timer")
                && painted.contains("Function context GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M7 DIAG generation={} peak_timers={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            peak_timers,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M7 DIAG fragments={fragments:?}");
        eprintln!("2M7 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M7 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /function-context.js;")),
            "2M7 external function-context script was not requested: events={events:?}"
        );
        assert!(
            painted.contains("Closure captured"),
            "2M7 lexical closure did not retain outer binding: painted={painted:?}; status={}",
            tab.status
        );
        assert!(
            painted.contains("This bound"),
            "2M7 method call did not bind `this` to its receiver: painted={painted:?}; status={}",
            tab.status
        );
        assert!(
            peak_timers >= 1,
            "2M7 async closure timer was never scheduled: peak_timers={peak_timers}; painted={painted:?}"
        );
        assert!(
            painted.contains("Closure timer"),
            "2M7 callback did not preserve lexical closure across timer task: painted={painted:?}"
        );
        assert!(painted.contains("Function context GREEN"));
        assert!(
            fragments.iter().any(|(text, color)| {
                text.contains("GREEN") && *color == (0, 87, 184)
            }),
            "2M7 final function-context state did not reach expected blue Paint: fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M8_DOM_EVENT_TARGET_I
    #[test]
    fn javascript_dom_event_target_add_remove_dispatch_reaches_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/event-target.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("capture-first-last")
                && painted.contains("Listener removed")
                && painted.contains("Listener this bound")
                && painted.contains("Event object bound")
                && painted.contains("Event target GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M8 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M8 DIAG fragments={fragments:?}");
        eprintln!("2M8 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M8 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /event-target.js;")),
            "2M8 external EventTarget script was not requested: events={events:?}"
        );
        assert!(
            painted.contains("capture-first-last"),
            "2M8 listener order/duplicate/removal semantics diverged: painted={painted:?}"
        );
        assert!(painted.contains("Listener removed"));
        assert!(
            painted.contains("Listener this bound"),
            "2M8 listener `this` was not bound to EventTarget: painted={painted:?}"
        );
        assert!(
            painted.contains("Event object bound"),
            "2M8 event type/target/currentTarget surface diverged: painted={painted:?}"
        );
        assert!(painted.contains("Event target GREEN"));
        assert!(
            fragments.iter().any(|(text, color)| {
                text.contains("GREEN") && *color == (106, 27, 154)
            }),
            "2M8 final EventTarget state did not reach expected purple Paint: fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M9_DOM_EVENT_PROPAGATION_II
    #[test]
    fn javascript_dom_event_propagation_capture_target_bubble_reaches_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/event-propagation.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Capture target bubble order")
                && painted.contains("Event phases 1-2-3-reset")
                && painted.contains("Propagation context bound")
                && painted.contains("Event propagation GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M9 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M9 DIAG fragments={fragments:?}");
        eprintln!("2M9 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M9 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /event-propagation.js;")),
            "2M9 external event propagation script was not requested: events={events:?}"
        );
        assert!(
            painted.contains("Capture target bubble order"),
            "2M9 capture/target/bubble order diverged: painted={painted:?}"
        );
        assert!(
            painted.contains("Event phases 1-2-3-reset"),
            "2M9 eventPhase surface diverged: painted={painted:?}"
        );
        assert!(
            painted.contains("Propagation context bound"),
            "2M9 target/currentTarget/this context diverged: painted={painted:?}"
        );
        assert!(painted.contains("Event propagation GREEN"));
        assert!(
            fragments.iter().any(|(text, color)| {
                text.contains("GREEN") && *color == (0, 105, 92)
            }),
            "2M9 final propagation state did not reach expected teal Paint: fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M10_DOM_EVENT_CONTROL_III
    #[test]
    fn javascript_dom_event_control_cancellation_stop_and_once_reach_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/event-control.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Default prevented dispatch false")
                && painted.contains("Stop propagation same target")
                && painted.contains("Stop immediate first only")
                && painted.contains("Once listener one call")
                && painted.contains("Event control GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M10 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M10 DIAG fragments={fragments:?}");
        eprintln!("2M10 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M10 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /event-control.js;")),
            "2M10 external event-control script was not requested: events={events:?}"
        );
        assert!(
            painted.contains("Default prevented dispatch false"),
            "2M10 preventDefault/defaultPrevented/dispatch return diverged: painted={painted:?}"
        );
        assert!(
            painted.contains("Stop propagation same target"),
            "2M10 stopPropagation semantics diverged: painted={painted:?}"
        );
        assert!(
            painted.contains("Stop immediate first only"),
            "2M10 stopImmediatePropagation semantics diverged: painted={painted:?}"
        );
        assert!(
            painted.contains("Once listener one call"),
            "2M10 once-listener semantics diverged: painted={painted:?}"
        );
        assert!(painted.contains("Event control GREEN"));
        assert!(
            fragments.iter().any(|(text, color)| {
                text.contains("GREEN") && *color == (21, 101, 192)
            }),
            "2M10 final event-control state did not reach expected blue Paint: fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M11_DOM_EVENT_LISTENER_SEMANTICS_IV
    #[test]
    fn javascript_dom_event_listener_semantics_final_gate_reaches_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/event-listeners.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Passive default preserved")
                && painted.contains("Dispatch mutation stable")
                && painted.contains("Listener object handled")
                && painted.contains("Duplicate identity stable")
                && painted.contains("Reentrant once stable")
                && painted.contains("Event listener FINAL GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M11 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M11 DIAG fragments={fragments:?}");
        eprintln!("2M11 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M11 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /event-listeners.js;")),
            "2M11 external event-listener script was not requested: events={events:?}"
        );
        for marker in [
            "Passive default preserved",
            "Dispatch mutation stable",
            "Listener object handled",
            "Duplicate identity stable",
            "Reentrant once stable",
            "Event listener FINAL GREEN",
        ] {
            assert!(
                painted.contains(marker),
                "2M11 marker missing {marker:?}: painted={painted:?}"
            );
        }
        // PHANTOM_2M11_R1_FINAL_PAINT_GATE_FIX
        // Paint text is emitted as word-level fragments, so the exact final marker
        // is already validated in `painted` above while color is checked on its
        // terminal GREEN token instead of requiring "FINAL GREEN" in one fragment.
        assert!(
            fragments.iter().any(|(text, color)| {
                text == "GREEN" && *color == (46, 125, 50)
            }),
            "2M11 final event-listener state did not reach expected green Paint: fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M12_DOM_TOKENLIST_CLASSLIST_I
    #[test]
    fn javascript_dom_class_list_add_remove_toggle_contains_reaches_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/classlist.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Class add remove active")
                && painted.contains("Toggle force stable")
                && painted.contains("Contains stable")
                && painted.contains("DOM classList GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M12 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M12 DIAG fragments={fragments:?}");
        eprintln!("2M12 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M12 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /classlist.js;")),
            "2M12 external classList script was not requested: events={events:?}"
        );
        for marker in [
            "Class add remove active",
            "Toggle force stable",
            "Contains stable",
            "DOM classList GREEN",
        ] {
            assert!(
                painted.contains(marker),
                "2M12 marker missing {marker:?}: painted={painted:?}"
            );
        }
        assert!(
            fragments.iter().any(|(text, color)| {
                text == "GREEN" && *color == (142, 36, 170)
            }),
            "2M12 final classList state did not reach expected purple Paint: fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M13_CSS_STYLE_DECLARATION_I
    #[test]
    fn javascript_dom_style_direct_set_property_remove_and_css_text_reach_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/style.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Direct style blue")
                && painted.contains("Property methods stable")
                && painted.contains("cssText live stable")
                && painted.contains("DOM style GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M13 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M13 DIAG fragments={fragments:?}");
        eprintln!("2M13 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M13 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /style.js;")),
            "2M13 external style script was not requested: events={events:?}"
        );
        for marker in [
            "Direct style blue",
            "Property methods stable",
            "cssText live stable",
            "DOM style GREEN",
        ] {
            assert!(
                painted.contains(marker),
                "2M13 marker missing {marker:?}: painted={painted:?}"
            );
        }
        assert!(
            fragments.iter().any(|(text, color)| {
                text == "target" && *color == (21, 101, 192)
            }),
            "2M13 direct style color did not reach target Paint: fragments={fragments:?}"
        );
        assert!(
            fragments.iter().any(|(text, color)| {
                text == "GREEN" && *color == (46, 125, 50)
            }),
            "2M13 final style state did not reach expected green Paint: fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M14_DOM_DATASET_ATTRIBUTE_REFLECTION_I
    #[test]
    fn javascript_dom_dataset_attribute_reflection_reaches_css_and_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/dataset.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Dataset identity stable")
                && painted.contains("Camel data 42")
                && painted.contains("Live attr ready")
                && painted.contains("Toggle attribute true-false-true")
                && painted.contains("Dataset removal live")
                && painted.contains("DOM dataset GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M14 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M14 DIAG fragments={fragments:?}");
        eprintln!("2M14 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M14 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /dataset.js;")),
            "2M14 external dataset script was not requested: events={events:?}"
        );
        for marker in [
            "Dataset identity stable",
            "Camel data 42",
            "Live attr ready",
            "Toggle attribute true-false-true",
            "Dataset removal live",
            "DOM dataset GREEN",
        ] {
            assert!(
                painted.contains(marker),
                "2M14 marker missing {marker:?}: painted={painted:?}"
            );
        }
        assert!(
            fragments.iter().any(|(text, color)| {
                text == "target" && *color == (0, 137, 123)
            }),
            "2M14 data-state attribute selector did not recolor target Paint: fragments={fragments:?}"
        );
        assert!(
            fragments.iter().any(|(text, color)| {
                text == "GREEN" && *color == (46, 125, 50)
            }),
            "2M14 final dataset state did not reach expected green Paint: fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M15_HTML_ATTRIBUTE_PROPERTY_REFLECTION_II
    #[test]
    fn javascript_html_attribute_property_reflection_reaches_css_layout_and_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/reflection.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Reflection updated pt-BR ltr true false")
                && painted.contains("Attribute names 5 dir title")
                && painted.contains("Revealed by property")
                && painted.contains("DOM reflection GREEN")
                && !painted.contains("Hidden property leak")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M15 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M15 DIAG fragments={fragments:?}");
        eprintln!("2M15 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M15 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /reflection.js;")),
            "2M15 external reflection script was not requested: events={events:?}"
        );
        for marker in [
            "Reflection updated pt-BR ltr true false",
            "Attribute names 5 dir title",
            "Revealed by property",
            "DOM reflection GREEN",
        ] {
            assert!(
                painted.contains(marker),
                "2M15 marker missing {marker:?}: painted={painted:?}"
            );
        }
        assert!(
            !painted.contains("Hidden property leak"),
            "2M15 hidden=true did not remove the hidden target from Layout/Paint: painted={painted:?}"
        );
        assert!(
            fragments.iter().any(|(text, color)| {
                text == "property" && *color == (21, 101, 192)
            }),
            "2M15 lang property -> attribute selector did not reach blue Paint: fragments={fragments:?}"
        );
        assert!(
            fragments.iter().any(|(text, color)| {
                text == "GREEN" && *color == (46, 125, 50)
            }),
            "2M15 final reflection state did not reach expected green Paint: fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M16_WEB_GLOBALS_CORE_BUILTINS_I
    #[test]
    fn javascript_web_globals_and_core_builtins_reach_dom_css_and_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/builtins.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Math score 6")
                && painted.contains("Parsed 42 and 3.5")
                && painted.contains("Builtins checks stable")
                && painted.contains("Web globals active")
                && painted.contains("Web builtins GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M16 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M16 DIAG fragments={fragments:?}");
        eprintln!("2M16 DIAG server={events:?}");

        assert_eq!(
            tab.navigation_phase(),
            NavigationPhase::Ready,
            "2M16 page did not reach Ready: status={}; events={events:?}",
            tab.status
        );
        assert_eq!(tab.address, url);
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("request: /builtins.js;")),
            "2M16 external builtins script was not requested: events={events:?}"
        );
        for marker in [
            "Math score 6",
            "Parsed 42 and 3.5",
            "Builtins checks stable",
            "Web globals active",
            "Web builtins GREEN",
        ] {
            assert!(
                painted.contains(marker),
                "2M16 marker missing {marker:?}: painted={painted:?}"
            );
        }
        assert!(
            fragments.iter().any(|(text, color)| {
                text == "active" && *color == (21, 101, 192)
            }),
            "2M16 builtin result did not drive target CSS/Paint: fragments={fragments:?}"
        );
        assert!(
            fragments.iter().any(|(text, color)| {
                text == "GREEN" && *color == (46, 125, 50)
            }),
            "2M16 final builtin state did not reach green Paint: fragments={fragments:?}"
        );
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M17_WEB_CORE_LIBRARY_II
    #[test]
    fn javascript_web_core_library_object_array_string_json_reaches_dom_css_and_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/core-library.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Array state 2-4-6")
                && painted.contains("String state PHANTOM")
                && painted.contains("JSON keys name,scores")
                && painted.contains("Core library active")
                && painted.contains("Web core library GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M17 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M17 DIAG fragments={fragments:?}");
        eprintln!("2M17 DIAG server={events:?}");

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, url);
        assert!(events.iter().any(|event| event.starts_with("request: /core-library.js;")));
        for marker in [
            "Array state 2-4-6",
            "String state PHANTOM",
            "JSON keys name,scores",
            "Core library active",
            "Web core library GREEN",
        ] {
            assert!(painted.contains(marker), "2M17 marker missing {marker:?}: {painted:?}");
        }
        assert!(fragments.iter().any(|(text, color)| text == "active" && *color == (0, 105, 92)));
        assert!(fragments.iter().any(|(text, color)| text == "GREEN" && *color == (46, 125, 50)));
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }

    // PHANTOM_2M18_WEB_CORE_LIBRARY_III
    #[test]
    fn javascript_web_core_library_callbacks_object_entries_and_string_helpers_reach_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/core-library-iii.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Core callbacks active")
                && painted.contains("Array callbacks 3-6-9-12 2-4 10 true-true-2")
                && painted.contains("Object entries a:1,b:2 own-true")
                && painted.contains("String helpers beta|gamma Phantom engine")
                && painted.contains("Web core library III GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M18 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M18 DIAG fragments={fragments:?}");
        eprintln!("2M18 DIAG server={events:?}");

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, url);
        assert!(events.iter().any(|event| event.starts_with("request: /core-library-iii.js;")));
        for marker in [
            "Core callbacks active",
            "Array callbacks 3-6-9-12 2-4 10 true-true-2",
            "Object entries a:1,b:2 own-true",
            "String helpers beta|gamma Phantom engine",
            "Web core library III GREEN",
        ] {
            assert!(painted.contains(marker), "2M18 marker missing {marker:?}: {painted:?}");
        }
        assert!(fragments.iter().any(|(text, color)| text == "active" && *color == (0, 105, 92)));
        assert!(fragments.iter().any(|(text, color)| text == "GREEN" && *color == (46, 125, 50)));
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }

    // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
    #[test]
    fn javascript_web_core_library_final_gate_reaches_dom_css_layout_and_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/core-library-final.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Core library final active")
                && painted.contains("Array final 6-5-4-3-2 tail 2 first 1")
                && painted.contains("Object final alpha,beta 2")
                && painted.contains("String final PHAN 007 x-x")
                && painted.contains("Numeric final 3 -1 true")
                && painted.contains("Web core library FINAL GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M19 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M19 DIAG fragments={fragments:?}");
        eprintln!("2M19 DIAG server={events:?}");

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, url);
        assert!(events.iter().any(|event| event.starts_with("request: /core-library-final.js;")));
        for marker in [
            "Core library final active",
            "Array final 6-5-4-3-2 tail 2 first 1",
            "Object final alpha,beta 2",
            "String final PHAN 007 x-x",
            "Numeric final 3 -1 true",
            "Web core library FINAL GREEN",
        ] {
            assert!(painted.contains(marker), "2M19 marker missing {marker:?}: {painted:?}");
        }
        assert!(fragments.iter().any(|(text, color)| text == "active" && *color == (0, 105, 92)));
        assert!(fragments.iter().any(|(text, color)| text == "GREEN" && *color == (46, 125, 50)));
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }

    // PHANTOM_2M20_DOM_TREE_NAVIGATION_MUTATION_COHERENCE_I
    #[test]
    fn javascript_dom_tree_navigation_and_mutation_coherence_reaches_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/dom-tree.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Tree navigation stable")
                && painted.contains("Append order coherent")
                && painted.contains("DOM tree navigation GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M20 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M20 DIAG fragments={fragments:?}");
        eprintln!("2M20 DIAG server={events:?}");

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, url);
        assert!(events.iter().any(|event| event.starts_with("request: /dom-tree.js;")));
        for marker in [
            "Tree navigation stable",
            "Append order coherent",
            "DOM tree navigation GREEN",
        ] {
            assert!(painted.contains(marker), "2M20 marker missing {marker:?}: {painted:?}");
        }
        assert!(fragments.iter().any(|(text, color)| text == "stable" && *color == (0, 105, 92)));
        assert!(fragments.iter().any(|(text, color)| text == "GREEN" && *color == (46, 125, 50)));
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }

    // PHANTOM_2M21_DOM_SELECTOR_API_COMPATIBILITY_II
    #[test]
    fn javascript_dom_selector_compound_lists_scope_and_closest_reach_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/dom-selectors.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Selector list document order stable")
                && painted.contains("Matches closest compound stable")
                && painted.contains("Element scope descendants stable")
                && painted.contains("Hero selected")
                && painted.contains("DOM selector API II GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M21 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M21 DIAG fragments={fragments:?}");
        eprintln!("2M21 DIAG server={events:?}");

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, url);
        assert!(events.iter().any(|event| event.starts_with("request: /dom-selectors.js;")));
        for marker in [
            "Selector list document order stable",
            "Matches closest compound stable",
            "Element scope descendants stable",
            "Hero selected",
            "DOM selector API II GREEN",
        ] {
            assert!(painted.contains(marker), "2M21 marker missing {marker:?}: {painted:?}");
        }
        assert!(fragments.iter().any(|(text, color)| text == "stable" && *color == (0, 105, 92)));
        assert!(fragments.iter().any(|(text, color)| text == "Hero" && *color == (94, 53, 177)));
        assert!(fragments.iter().any(|(text, color)| text == "GREEN" && *color == (46, 125, 50)));
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }

    // PHANTOM_2M22_DOM_MUTATION_API_II
    #[test]
    fn javascript_dom_mutation_api_reorders_replaces_removes_and_reaches_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/dom-mutations.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Mutation order stable e-c-a-b")
                && painted.contains("removeChild identity disconnected stable")
                && painted.contains("Mutation selected")
                && painted.contains("DOM mutation API II GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M22 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M22 DIAG fragments={fragments:?}");
        eprintln!("2M22 DIAG server={events:?}");

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, url);
        assert!(events.iter().any(|event| event.starts_with("request: /dom-mutations.js;")));
        for marker in [
            "Mutation order stable e-c-a-b",
            "removeChild identity disconnected stable",
            "Mutation selected",
            "DOM mutation API II GREEN",
        ] {
            assert!(painted.contains(marker), "2M22 marker missing {marker:?}: {painted:?}");
        }
        assert!(fragments.iter().any(|(text, color)| text == "stable" && *color == (0, 105, 92)));
        assert!(fragments.iter().any(|(text, color)| text == "Mutation" && *color == (94, 53, 177)));
        assert!(fragments.iter().any(|(text, color)| text == "GREEN" && *color == (46, 125, 50)));
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }

    // PHANTOM_2M23_HTML_ELEMENT_INTERACTION_I
    #[test]
    fn javascript_html_element_reflection_and_synthetic_click_reach_dom_css_layout_and_paint()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/html-elements.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("Synthetic click target go")
                && painted.contains("Input reflection beta email true true true")
                && painted.contains("Link reflection /next _blank noopener")
                && painted.contains("Form reflection /submit post multipart/form-data")
                && painted.contains("HTML element interaction GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2M23 DIAG generation={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={painted:?}",
            tab.document_generation,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status
        );
        eprintln!("2M23 DIAG fragments={fragments:?}");
        eprintln!("2M23 DIAG server={events:?}");

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, url);
        assert!(events.iter().any(|event| event.starts_with("request: /html-elements.js;")));
        for marker in [
            "Synthetic click target go",
            "Input reflection beta email true true true",
            "Link reflection /next _blank noopener",
            "Form reflection /submit post multipart/form-data",
            "HTML element interaction GREEN",
        ] {
            assert!(painted.contains(marker), "2M23 marker missing {marker:?}: {painted:?}");
        }
        assert!(fragments.iter().any(|(text, color)| text == "Synthetic" && *color == (46, 125, 50)));
        assert!(fragments.iter().any(|(text, color)| text == "GREEN" && *color == (46, 125, 50)));
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M24_NATIVE_INTERACTION_JAVASCRIPT_EVENTS_I
    #[test]
    fn native_interaction_dispatches_trusted_click_before_default_action()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/native-interaction.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);

            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, url);

        let allowed_node = tab
            .engine
            .link_regions()
            .iter()
            .find(|link| link.href() == "/allowed-target.html")
            .map_or(0, |link| link.node_id());
        assert_ne!(allowed_node, 0);

        let allowed_default = dispatch_native_click_before_default_action(
            &mut tab,
            allowed_node,
            &url,
            1.0,
        )?;
        assert!(allowed_default);
        assert_eq!(
            tab.engine.link_for_node(allowed_node).map(|link| link.href()),
            Some("/mutated-target.html")
        );
        assert!(corpus_painted_text(&tab).contains("Trusted link true"));

        let blocked_node = tab
            .engine
            .link_regions()
            .iter()
            .find(|link| link.href() == "/blocked-target.html")
            .map_or(0, |link| link.node_id());
        assert_ne!(blocked_node, 0);

        let blocked_default = dispatch_native_click_before_default_action(
            &mut tab,
            blocked_node,
            &url,
            1.0,
        )?;
        assert!(!blocked_default);
        assert!(corpus_painted_text(&tab).contains("Blocked link true"));

        let submit_node = tab
            .engine
            .form_control_regions()
            .into_iter()
            .find(|control| {
                control.kind() == FormControlKind::Submit && control.label() == "Submit"
            })
            .map_or(0, |control| control.id().as_u64());
        assert_ne!(submit_node, 0);

        let submit_default = dispatch_native_click_before_default_action(
            &mut tab,
            submit_node,
            &url,
            1.0,
        )?;
        assert!(!submit_default);
        assert!(corpus_painted_text(&tab).contains("Blocked submit true"));

        let plain_node = tab
            .engine
            .document()
            .nodes()
            .find_map(|node| match node.kind() {
                phantom_engine::NodeKind::Element(element)
                    if element.attribute("id") == Some("plain") =>
                {
                    Some(node.id())
                }
                _ => None,
            })
            .ok_or(std::io::Error::other("2M24 plain target missing"))?;
        let plain_rect = tab
            .engine
            .layout()
            .boxes()
            .iter()
            .find(|layout_box| layout_box.source_node() == plain_node)
            .map(|layout_box| layout_box.rect())
            .ok_or(std::io::Error::other("2M24 plain layout box missing"))?;
        let hit = tab.engine.dom_element_at(
            plain_rect.x() + plain_rect.width().min(4.0),
            plain_rect.y() + plain_rect.height().min(4.0),
        );
        assert_eq!(hit, Some(plain_node.as_u64()));

        let plain_default = dispatch_native_click_before_default_action(
            &mut tab,
            plain_node.as_u64(),
            &url,
            1.0,
        )?;
        assert!(plain_default);
        let painted = corpus_painted_text(&tab);
        assert!(painted.contains("Plain trusted true"));

        let fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(fragments.iter().any(|(text, color)| {
            text == "Plain" && *color == (94, 53, 177)
        }));
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2M25_FINAL_WEB_COMPATIBILITY_INTEGRATION_GATE
    #[test]
    fn web_compatibility_2m_final_integrates_full_stack_and_native_navigation()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = CorpusServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/web-compat-final.html", server.address);
        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        let mut peak_fetches = 0_usize;
        let mut peak_timers = 0_usize;
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            peak_fetches = peak_fetches.max(tab.pending_script_fetches.len());
            peak_timers = peak_timers.max(tab.engine.pending_javascript_timer_count());
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);
            peak_fetches = peak_fetches.max(tab.pending_script_fetches.len());

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("2M FINAL ASYNC GREEN")
                && tab.pending_script_fetches.is_empty()
                && tab.pending_page_scripts.is_none()
                && tab.engine.pending_javascript_timer_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());
        eprintln!(
            "2M25 FINAL DIAG generation={} peak_fetches={} peak_timers={} pending_fetches={} pending_timers={} phase={:?} status={:?} painted={:?}",
            tab.document_generation,
            peak_fetches,
            peak_timers,
            tab.pending_script_fetches.len(),
            tab.engine.pending_javascript_timer_count(),
            tab.navigation_phase(),
            tab.status,
            painted
        );
        eprintln!("2M25 FINAL DIAG server={events:?}");

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, url);
        assert!(events.iter().any(|event| {
            event.starts_with("request: /web-compat-final.js;")
        }));
        assert!(events.iter().any(|event| {
            event.starts_with("request: /web-compat-final-data.txt;")
        }));
        assert!(peak_fetches >= 1, "2M25 fetch path was not observed");
        assert!(peak_timers >= 1, "2M25 timer path was not observed");
        for marker in [
            "2M FINAL ASYNC GREEN final-data",
            "selector 2",
            "builtins 6-8 a,b PHANTOM",
            "function CTX-CLOSURE",
            "tree mutation-zone 3",
            "synthetic false",
            "reflection 2M final beta search true",
            "async sync-micro-thenable-timer-fetch",
            "Style direct GREEN",
        ] {
            assert!(
                painted.contains(marker),
                "2M25 integrated marker missing {marker:?}: {painted:?}"
            );
        }

        let initial_fragments = tab
            .engine
            .paint_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PaintCommand::Text { text, color, .. } => tab
                    .engine
                    .paint_list()
                    .text(*text)
                    .map(|value| {
                        (
                            value.split_whitespace().collect::<Vec<_>>().join(" "),
                            (color.red(), color.green(), color.blue()),
                        )
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(initial_fragments.iter().any(|(text, color)| {
            text.contains("ASYNC") && *color == (46, 125, 50)
        }));
        assert!(initial_fragments.iter().any(|(text, color)| {
            text == "Style" && *color == (21, 101, 192)
        }));

        let allowed_node = tab
            .engine
            .document()
            .nodes()
            .find_map(|node| match node.kind() {
                phantom_engine::NodeKind::Element(element)
                    if element.attribute("id") == Some("continue-link") =>
                {
                    Some(node.id().as_u64())
                }
                _ => None,
            })
            .ok_or(std::io::Error::other("2M25 allowed link missing"))?;
        assert_eq!(
            tab.engine.link_for_node(allowed_node).map(|link| link.href()),
            Some("/wrong-target.html")
        );
        let allowed_default = dispatch_native_click_before_default_action(
            &mut tab,
            allowed_node,
            &url,
            1.0,
        )?;
        assert!(allowed_default);
        let painted = corpus_painted_text(&tab);
        assert!(painted.contains("Native Ctrue-Ttrue-Btrue"));
        let allowed_href = tab
            .engine
            .link_for_node(allowed_node)
            .map(|link| link.href().to_owned())
            .ok_or(std::io::Error::other("2M25 allowed link disappeared after trusted click"))?;
        assert_eq!(allowed_href, "/web-compat-final-target.html");

        let blocked_node = tab
            .engine
            .link_regions()
            .iter()
            .find(|link| link.href() == "/blocked-target.html")
            .map_or(0, |link| link.node_id());
        assert_ne!(blocked_node, 0);
        let blocked_default = dispatch_native_click_before_default_action(
            &mut tab,
            blocked_node,
            &url,
            1.0,
        )?;
        assert!(!blocked_default);
        let painted = corpus_painted_text(&tab);
        assert!(painted.contains("Blocked trusted true"));

        let target = HttpUrl::parse(&url)?.resolve(&allowed_href)?;
        start_navigation(
            &network,
            &mut tab,
            target.as_str().to_owned(),
            NavigationAction::New,
        );
        let navigation_deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < navigation_deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            pump_page_timers(&mut tab, Instant::now(), 1.0);
            poll_page_fetches(&mut tab, 1.0);
            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("2M FINAL NAVIGATION GREEN")
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, target.as_str());
        assert!(painted.contains("2M FINAL NAVIGATION GREEN"));
        assert!(tab.pending_script_fetches.is_empty());
        assert!(tab.pending_page_scripts.is_none());
        assert_eq!(tab.engine.pending_javascript_timer_count(), 0);
        Ok(())
    }


    // PHANTOM_2N6_CORS_FOUNDATION_I
    #[test]
    fn cors_foundation_classifies_typed_origins_and_admits_only_single_allow_origin()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = HttpUrl::parse("http://EXAMPLE.test:80/page")?;
        let same_origin = HttpUrl::parse("http://example.test/data")?;
        let different_port = HttpUrl::parse("http://example.test:8080/data")?;
        let different_scheme = HttpUrl::parse("https://example.test/data")?;

        assert_eq!(
            classify_page_fetch_cors(&document, &same_origin),
            PageFetchCorsDecision::SameOrigin
        );
        assert_eq!(
            classify_page_fetch_cors(&document, &different_port),
            PageFetchCorsDecision::CorsRequired {
                request_origin: "http://example.test".to_owned(),
            }
        );
        assert_eq!(
            classify_page_fetch_cors(&document, &different_scheme),
            PageFetchCorsDecision::CorsRequired {
                request_origin: "http://example.test".to_owned(),
            }
        );

        let origin = document.origin();
        assert!(cors_response_origin_denial(origin.as_str(), Some("*")).is_none());
        assert!(
            cors_response_origin_denial(origin.as_str(), Some("http://example.test")).is_none()
        );
        assert!(
            cors_response_origin_denial(origin.as_str(), Some("  http://example.test  ")).is_none()
        );
        assert!(cors_response_origin_denial(origin.as_str(), None).is_some());
        assert!(cors_response_origin_denial(origin.as_str(), Some("null")).is_some());
        assert!(
            cors_response_origin_denial(origin.as_str(), Some("http://other.test")).is_some()
        );
        assert!(
            cors_response_origin_denial(
                origin.as_str(),
                Some("http://example.test, http://other.test"),
            )
            .is_some()
        );
        Ok(())
    }

    // PHANTOM_2N15_NON_SIMPLE_METHOD_BODY_TRANSPORT_I
    #[test]
    fn non_simple_post_put_patch_preflight_then_send_bounded_body()
    -> Result<(), Box<dyn std::error::Error>> {
        let document_server = CorsTransportServer::new("http://unused.invalid".to_owned())?;
        let document = HttpUrl::parse(&format!("{}/page.html", document_server.address))?;
        let cors_server = CorsTransportServer::new(document.origin().as_str().to_owned())?;
        let client = NetworkClient::new();

        let post = format!("{}/2n15-post-json.txt", cors_server.address);
        let post_response = client.fetch_text_cors_with_body(
            &post,
            &document,
            phantom_net::CorsBodyMethod::Post,
            &[("Content-Type", "application/json"), ("X-Phantom", "post")],
            br#"{"message":"phantom"}"#,
        )?;
        assert_eq!(post_response.body(), "POST BODY GREEN");

        let put = format!("{}/2n15-put.txt", cors_server.address);
        let put_response = client.fetch_text_cors_with_body(
            &put,
            &document,
            phantom_net::CorsBodyMethod::Put,
            &[("Content-Type", "text/plain")],
            b"replace-body",
        )?;
        assert_eq!(put_response.body(), "PUT BODY GREEN");

        let patch = format!("{}/2n15-patch.txt", cors_server.address);
        let patch_response = client.fetch_text_cors_with_body(
            &patch,
            &document,
            phantom_net::CorsBodyMethod::Patch,
            &[("Content-Type", "application/json"), ("X-Phantom", "patch")],
            br#"{"replace":true}"#,
        )?;
        assert_eq!(patch_response.body(), "PATCH BODY GREEN");

        let denied = format!("{}/2n15-deny-method.txt", cors_server.address);
        assert!(client
            .fetch_text_cors_with_body(
                &denied,
                &document,
                phantom_net::CorsBodyMethod::Put,
                &[("Content-Type", "text/plain")],
                b"must-not-send",
            )
            .is_err());

        let events = cors_server
            .events
            .lock()
            .map_err(|_| std::io::Error::other("2N-15 event lock poisoned"))?;

        assert!(events.iter().any(|event| {
            event.contains("/2n15-post-json.txt")
                && event.contains("method=OPTIONS")
                && event.contains("acrm=POST")
                && event.contains("body=")
        }));
        assert!(events.iter().any(|event| {
            event.contains("/2n15-post-json.txt")
                && event.contains("method=POST")
                && event.contains("content-type=application/json")
                && event.contains("x-phantom=post")
                && event.contains(r#"body={"message":"phantom"}"#)
        }));
        assert!(events.iter().any(|event| {
            event.contains("/2n15-put.txt")
                && event.contains("method=PUT")
                && event.contains("body=replace-body")
        }));
        assert!(events.iter().any(|event| {
            event.contains("/2n15-patch.txt")
                && event.contains("method=PATCH")
                && event.contains(r#"body={"replace":true}"#)
        }));
        assert!(events.iter().any(|event| {
            event.contains("/2n15-deny-method.txt") && event.contains("method=OPTIONS")
        }));
        assert!(!events.iter().any(|event| {
            event.contains("/2n15-deny-method.txt") && event.contains("method=PUT")
        }));
        Ok(())
    }

    // PHANTOM_2N14_CREDENTIALED_CORS_TRANSPORT_I
    #[test]
    fn credentialed_cors_transport_sends_cookie_after_preflight_and_commits_set_cookie_only_on_admission()
    -> Result<(), Box<dyn std::error::Error>> {
        let document_server = CorsTransportServer::new("http://unused.invalid".to_owned())?;
        let document = HttpUrl::parse(&format!("{}/page.html", document_server.address))?;
        let cors_server = CorsTransportServer::new(document.origin().as_str().to_owned())?;
        let client = NetworkClient::with_cookie_store_limit(8);

        let target = HttpUrl::parse(&format!("{}/2n14-credentialed.txt", cors_server.address))?;
        assert_eq!(
            client.store_set_cookie(
                &document,
                &target,
                phantom_net::CredentialsMode::Include,
                "sid=transport; Path=/; SameSite=Lax",
            )?,
            phantom_net::CookieStoreOutcome::Stored
        );

        let response = client.fetch_text_cors_credentialed_get_with_headers(
            target.as_str(),
            &document,
            &[("X-Phantom", "credentialed")],
            phantom_net::CookieSiteContext::SameSite,
        )?;
        assert_eq!(response.body(), "CREDENTIALED CORS GREEN");
        assert_eq!(
            response.access_control_allow_origin(),
            Some(document.origin().as_str())
        );
        assert_eq!(response.access_control_allow_credentials(), Some("true"));

        let events = cors_server
            .events
            .lock()
            .map_err(|_| std::io::Error::other("CORS event lock poisoned"))?;
        let target_events = events
            .iter()
            .filter(|event| event.contains("/2n14-credentialed.txt"))
            .collect::<Vec<_>>();
        assert!(target_events.iter().any(|event| {
            event.contains("method=OPTIONS")
                && event.contains("acrh=x-phantom")
                && event.contains("cookie=<none>")
        }));
        assert!(target_events.iter().any(|event| {
            event.contains("method=GET")
                && event.contains("x-phantom=credentialed")
                && event.contains("cookie=sid=transport")
        }));
        drop(events);

        let cookie_header = client
            .cookie_header_for_request(
                &document,
                &target,
                phantom_net::CredentialsMode::Include,
                phantom_net::CookieSiteContext::SameSite,
            )?
            .ok_or(std::io::Error::other("credentialed cookie header missing"))?;
        assert!(cookie_header.contains("sid=transport"));
        assert!(cookie_header.contains("rotated=1"));

        let wildcard = format!("{}/2n14-wildcard-final.txt", cors_server.address);
        assert!(client
            .fetch_text_cors_credentialed_get_with_headers(
                &wildcard,
                &document,
                &[("X-Phantom", "wildcard-denied")],
                phantom_net::CookieSiteContext::SameSite,
            )
            .is_err());

        let case_denied = format!("{}/2n14-acac-case.txt", cors_server.address);
        assert!(client
            .fetch_text_cors_credentialed_get_with_headers(
                &case_denied,
                &document,
                &[("X-Phantom", "case-denied")],
                phantom_net::CookieSiteContext::SameSite,
            )
            .is_err());

        let cookie_header = client
            .cookie_header_for_request(
                &document,
                &target,
                phantom_net::CredentialsMode::Include,
                phantom_net::CookieSiteContext::SameSite,
            )?
            .ok_or(std::io::Error::other("credentialed cookie header disappeared"))?;
        assert!(!cookie_header.contains("denied=1"));
        assert!(!cookie_header.contains("deniedcase=1"));

        let preflight_wildcard =
            format!("{}/2n14-preflight-wildcard.txt", cors_server.address);
        assert!(client
            .fetch_text_cors_credentialed_get_with_headers(
                &preflight_wildcard,
                &document,
                &[("X-Phantom", "preflight-denied")],
                phantom_net::CookieSiteContext::SameSite,
            )
            .is_err());

        let events = cors_server
            .events
            .lock()
            .map_err(|_| std::io::Error::other("CORS event lock poisoned after denial"))?;
        let wildcard_events = events
            .iter()
            .filter(|event| event.contains("/2n14-preflight-wildcard.txt"))
            .collect::<Vec<_>>();
        assert!(wildcard_events.iter().any(|event| {
            event.contains("method=OPTIONS") && event.contains("cookie=<none>")
        }));
        assert!(!wildcard_events.iter().any(|event| event.contains("method=GET")));
        Ok(())
    }

    // PHANTOM_2N13_CREDENTIALS_COOKIE_FOUNDATION_I
    #[test]
    fn credential_cookie_foundation_stays_detached_from_transport_and_partitioned()
    -> Result<(), Box<dyn std::error::Error>> {
        let client = NetworkClient::with_cookie_store_limit(8);
        let document = HttpUrl::parse("https://app.example.test/page")?;
        let response = HttpUrl::parse("https://api.example.test/session/login")?;
        let request = HttpUrl::parse("https://api.example.test/session/data")?;

        assert_eq!(
            client.store_set_cookie(
                &document,
                &response,
                phantom_net::CredentialsMode::SameOrigin,
                "sid=blocked; Path=/session; Secure; SameSite=None",
            )?,
            phantom_net::CookieStoreOutcome::Ignored
        );

        assert_eq!(
            client.store_set_cookie(
                &document,
                &response,
                phantom_net::CredentialsMode::Include,
                "sid=foundation; Path=/session; Secure; HttpOnly; SameSite=None",
            )?,
            phantom_net::CookieStoreOutcome::Stored
        );

        assert_eq!(
            client.cookie_header_for_request(
                &document,
                &request,
                phantom_net::CredentialsMode::Omit,
                phantom_net::CookieSiteContext::CrossSite,
            )?,
            None
        );
        assert_eq!(
            client.cookie_header_for_request(
                &document,
                &request,
                phantom_net::CredentialsMode::Include,
                phantom_net::CookieSiteContext::CrossSite,
            )?
            .as_deref(),
            Some("sid=foundation")
        );

        let other_top_level = HttpUrl::parse("https://other.example.test/page")?;
        assert_eq!(
            client.cookie_header_for_request(
                &other_top_level,
                &request,
                phantom_net::CredentialsMode::Include,
                phantom_net::CookieSiteContext::CrossSite,
            )?,
            None
        );

        Ok(())
    }

    // PHANTOM_2N12_PREFLIGHT_METHODS_HEADERS_ADMISSION_II
    #[test]
    fn cors_preflighted_main_get_sends_author_headers_validates_final_cors_and_clears_on_failure()
    -> Result<(), Box<dyn std::error::Error>> {
        let document_server = CorsTransportServer::new("http://unused.invalid".to_owned())?;
        let document = HttpUrl::parse(&format!("{}/page.html", document_server.address))?;
        let cors_server = CorsTransportServer::new(document.origin().as_str().to_owned())?;
        let client = NetworkClient::new();

        let target = format!("{}/2n12-main-custom-header.txt", cors_server.address);
        let response = client.fetch_text_cors_get_with_headers(
            &target,
            &document,
            &[("X-Phantom", "admitted")],
        )?;
        assert_eq!(response.body(), "CORS PREFLIGHTED MAIN GET GREEN");

        let events = cors_server
            .events
            .lock()
            .map_err(|_| "CORS transport event lock poisoned")?;
        let target_events = events
            .iter()
            .filter(|event| event.contains("/2n12-main-custom-header.txt"))
            .collect::<Vec<_>>();
        assert!(target_events.iter().any(|event| {
            event.contains("method=OPTIONS")
                && event.contains("acrm=GET")
                && event.contains("acrh=x-phantom")
        }));
        assert!(target_events.iter().any(|event| {
            event.contains("method=GET") && event.contains("x-phantom=admitted")
        }));
        drop(events);

        let denied = format!("{}/2n12-final-deny.txt", cors_server.address);
        assert!(client
            .fetch_text_cors_get_with_headers(
                &denied,
                &document,
                &[("X-Phantom", "blocked")],
            )
            .is_err());

        let before_retry = cors_server
            .events
            .lock()
            .map_err(|_| "CORS transport event lock poisoned")?
            .iter()
            .filter(|event| {
                event.contains("/2n12-final-deny.txt") && event.contains("method=OPTIONS")
            })
            .count();
        assert_eq!(before_retry, 1);

        assert!(client
            .fetch_text_cors_get_with_headers(
                &denied,
                &document,
                &[("X-Phantom", "blocked")],
            )
            .is_err());

        let after_retry = cors_server
            .events
            .lock()
            .map_err(|_| "CORS transport event lock poisoned")?
            .iter()
            .filter(|event| {
                event.contains("/2n12-final-deny.txt") && event.contains("method=OPTIONS")
            })
            .count();
        assert_eq!(after_retry, 2);

        let redirect = format!("{}/2n12-redirect-custom.txt", cors_server.address);
        assert!(client
            .fetch_text_cors_get_with_headers(
                &redirect,
                &document,
                &[("X-Phantom", "redirect")],
            )
            .is_err());

        assert!(client
            .fetch_text_cors_get_with_headers(
                &target,
                &document,
                &[("Bad Header", "x")],
            )
            .is_err());

        Ok(())
    }

    // PHANTOM_2N11_CORS_PREFLIGHT_FOUNDATION_I
    #[test]
    fn cors_preflight_options_admission_and_partitioned_cache_are_fail_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        let document_server = CorsTransportServer::new("http://unused.invalid".to_owned())?;
        let document = HttpUrl::parse(&format!("{}/page.html", document_server.address))?;
        let cors_server = CorsTransportServer::new(document.origin().as_str().to_owned())?;
        let client = NetworkClient::new();

        let target = format!("{}/2n11-preflight-put-json.txt", cors_server.address);
        let headers = [("Content-Type", "application/json"), ("X-Phantom", "1")];

        assert_eq!(
            client.ensure_cors_preflight(&target, &document, "PUT", &headers)?,
            phantom_net::CorsPreflightOutcome::Performed
        );
        assert_eq!(
            client.ensure_cors_preflight(&target, &document, "PUT", &headers)?,
            phantom_net::CorsPreflightOutcome::Cached
        );

        let events = cors_server
            .events
            .lock()
            .map_err(|_| "CORS transport event lock poisoned")?;
        let preflights = events
            .iter()
            .filter(|event| event.starts_with("request: /2n11-preflight-put-json.txt; method=OPTIONS"))
            .collect::<Vec<_>>();
        assert_eq!(preflights.len(), 1);
        assert!(preflights[0].contains(&format!("origin={}", document.origin().as_str())));
        assert!(preflights[0].contains("acrm=PUT"));
        assert!(
            preflights[0].contains("acrh=content-type,x-phantom")
                || preflights[0].contains("acrh=x-phantom,content-type")
        );
        drop(events);

        let wildcard = format!("{}/2n11-preflight-wildcard.txt", cors_server.address);
        assert_eq!(
            client.ensure_cors_preflight(
                &wildcard,
                &document,
                "PATCH",
                &[("Authorization", "Bearer test")],
            )?,
            phantom_net::CorsPreflightOutcome::Performed
        );

        let deny_method = format!("{}/2n11-preflight-deny-method.txt", cors_server.address);
        assert!(client
            .ensure_cors_preflight(
                &deny_method,
                &document,
                "PUT",
                &[("Content-Type", "application/json")],
            )
            .is_err());

        let deny_header = format!("{}/2n11-preflight-deny-header.txt", cors_server.address);
        assert!(client
            .ensure_cors_preflight(
                &deny_header,
                &document,
                "PUT",
                &[("Content-Type", "application/json")],
            )
            .is_err());

        let no_acao = format!("{}/2n11-preflight-no-acao.txt", cors_server.address);
        assert!(client
            .ensure_cors_preflight(
                &no_acao,
                &document,
                "PUT",
                &[("Content-Type", "application/json")],
            )
            .is_err());

        let redirect = format!("{}/2n11-preflight-redirect.txt", cors_server.address);
        assert!(client
            .ensure_cors_preflight(
                &redirect,
                &document,
                "PUT",
                &[("Content-Type", "application/json")],
            )
            .is_err());

        assert_eq!(
            client.ensure_cors_preflight(
                &target,
                &document,
                "GET",
                &[("Accept", "text/plain")],
            )?,
            phantom_net::CorsPreflightOutcome::NotRequired
        );

        Ok(())
    }

    // PHANTOM_2N10_SIMPLE_REQUEST_MATRIX_I
    #[test]
    fn cors_simple_request_matrix_blocks_future_preflight_and_forbidden_paths() {
        assert!(cors_simple_request_denial("GET", &[]).is_none());
        assert!(
            cors_simple_request_denial("POST", &[("Content-Type", "text/plain")]).is_none()
        );

        let preflight =
            cors_simple_request_denial("POST", &[("Content-Type", "application/json")]);
        assert!(preflight.is_some_and(|reason| reason.contains("preflight required")));

        let custom = cors_simple_request_denial("GET", &[("X-Phantom", "1")]);
        assert!(custom.is_some_and(|reason| reason.contains("preflight required")));

        let forbidden = cors_simple_request_denial("GET", &[("Cookie", "secret=1")]);
        assert!(forbidden.is_some_and(|reason| reason.contains("forbidden")));

        let trace = cors_simple_request_denial("TRACE", &[]);
        assert!(trace.is_some_and(|reason| reason.contains("forbidden")));
    }

    // PHANTOM_2N7_CORS_TRANSPORT_I
    // PHANTOM_2N9_CORS_SAFELISTED_RESPONSE_HEADERS_I
    #[test]
    fn cors_safelisted_response_headers_reach_browser_host_without_forbidden_headers()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = HttpUrl::parse("http://127.0.0.1:9/document.html")?;
        let cors_server =
            CorsTransportServer::new(document.origin().as_str().to_owned())?;
        let target = HttpUrl::parse(&format!(
            "{}/2n9-safe-headers.txt",
            cors_server.address
        ))?;
        let client = NetworkClient::new();

        let response = fetch_text_subresource(
            &client,
            &target,
            &document,
            TextSubresourcePolicy::SameOriginFetch,
        )?;
        let headers = response.cors_safelisted_response_headers();

        assert_eq!(response.body(), "CORS SAFE HEADERS GREEN");
        assert_eq!(headers.len(), 7);
        assert_eq!(headers.get("cache-control"), Some("no-store"));
        assert_eq!(headers.get("Content-Language"), Some("pt-BR"));
        assert_eq!(headers.get("content-type"), Some("text/plain; charset=utf-8"));
        assert_eq!(
            headers.get("expires"),
            Some("Wed, 21 Oct 2026 07:28:00 GMT")
        );
        assert_eq!(
            headers.get("last-modified"),
            Some("Tue, 20 Oct 2026 07:28:00 GMT")
        );
        assert_eq!(headers.get("pragma"), Some("no-cache"));
        let expected_length = response.body_bytes().to_string();
        assert_eq!(headers.get("content-length"), Some(expected_length.as_str()));
        assert_eq!(headers.get("set-cookie"), None);
        assert_eq!(headers.get("access-control-allow-origin"), None);
        assert_eq!(headers.get("x-phantom-secret"), None);
        Ok(())
    }

    #[test]
    fn cors_transport_simple_get_sends_origin_and_admits_only_valid_final_acao()
    -> Result<(), Box<dyn std::error::Error>> {
        let origin_server = TextSubresourcePolicyServer::new()?;
        let document = HttpUrl::parse(&format!("{}/2n5-page.html", origin_server.address))?;
        let cors_server = CorsTransportServer::new(document.origin().as_str().to_owned())?;
        let client = NetworkClient::with_max_body_bytes(MAX_SCRIPT_FETCH_BODY_BYTES);

        for (path, expected) in [
            ("/2n7-allow-star.txt", "CORS STAR GREEN"),
            ("/2n7-allow-exact.txt", "CORS EXACT GREEN"),
            ("/2n7-redirect.txt", "CORS STAR GREEN"),
        ] {
            let target = HttpUrl::parse(&format!("{}{path}", cors_server.address))?;
            let response = fetch_text_subresource(
                &client,
                &target,
                &document,
                TextSubresourcePolicy::SameOriginFetch,
            )?;
            assert_eq!(response.body(), expected);
        }

        for path in [
            "/2n7-no-acao.txt",
            "/2n7-wrong-origin.txt",
            "/2n7-multi-origin.txt",
        ] {
            let target = HttpUrl::parse(&format!("{}{path}", cors_server.address))?;
            let result = fetch_text_subresource(
                &client,
                &target,
                &document,
                TextSubresourcePolicy::SameOriginFetch,
            );
            let error = match result {
                Err(error) => error,
                Ok(_) => {
                    return Err(std::io::Error::other(format!(
                        "{path} unexpectedly passed CORS admission"
                    ))
                    .into());
                }
            };
            assert!(
                error.to_string().contains("CORS response admission blocked"),
                "{error}"
            );
        }

        let same_origin = HttpUrl::parse(&format!("{}/2n5-probe.txt", origin_server.address))?;
        let response = fetch_text_subresource(
            &client,
            &same_origin,
            &document,
            TextSubresourcePolicy::SameOriginFetch,
        )?;
        assert_eq!(response.body(), "probe");

        thread::sleep(Duration::from_millis(20));
        let events = cors_server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());
        let expected_origin = format!("origin={}", document.origin());
        let cors_requests = events
            .iter()
            .filter(|event| event.starts_with("request: /2n7-"))
            .collect::<Vec<_>>();
        assert!(!cors_requests.is_empty(), "events={events:?}");
        assert!(
            cors_requests
                .iter()
                .all(|event| event.contains(&expected_origin)),
            "events={events:?}"
        );
        Ok(())
    }

    #[test]
    fn cors_transport_page_fetch_delivers_authorized_body_to_javascript()
    -> Result<(), Box<dyn std::error::Error>> {
        let origin_server = TextSubresourcePolicyServer::new()?;
        let document_url = format!("{}/2n5-page.html", origin_server.address);
        let document = HttpUrl::parse(&document_url)?;
        let cors_server = CorsTransportServer::new(document.origin().as_str().to_owned())?;
        let target = format!("{}/2n7-allow-star.txt", cors_server.address);

        let mut tab = BrowserTab::new();
        tab.engine.load_html(&format!(
            "<p id='target'>Before</p><script>fetch('{target}').then(function(response) {{ return response.text(); }}).then(function(text) {{ document.getElementById('target').textContent = text; }}).catch(function() {{ document.getElementById('target').textContent = 'FAILED'; }});</script>"
        ))?;
        tab.address = document_url.clone();
        execute_committed_page_scripts(&mut tab, &document_url, 1.0)?;
        tab.mark_navigation_ready();

        for _ in 0..120 {
            poll_page_fetches(&mut tab, 1.0);
            let painted = corpus_painted_text(&tab);
            if painted.contains("CORS STAR GREEN") || painted.contains("FAILED") {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        assert!(painted.contains("CORS STAR GREEN"), "{painted}");
        assert!(!painted.contains("FAILED"), "{painted}");
        assert!(tab.pending_script_fetches.is_empty());
        Ok(())
    }

    #[test]
    fn cors_transport_cache_partition_and_generation_isolation_remain_bounded()
    -> Result<(), Box<dyn std::error::Error>> {
        let origin_a = TextSubresourcePolicyServer::new()?;
        let origin_c = TextSubresourcePolicyServer::new()?;
        let document_a = HttpUrl::parse(&format!("{}/2n5-page.html", origin_a.address))?;
        let document_c = HttpUrl::parse(&format!("{}/2n5-page.html", origin_c.address))?;
        let cors_server = CorsTransportServer::new(document_a.origin().as_str().to_owned())?;
        let client = NetworkClient::with_max_body_bytes(MAX_SCRIPT_FETCH_BODY_BYTES);
        let target = HttpUrl::parse(&format!("{}/2n7-cache-star.txt", cors_server.address))?;

        let first = fetch_text_subresource(
            &client,
            &target,
            &document_a,
            TextSubresourcePolicy::SameOriginFetch,
        )?;
        let second = fetch_text_subresource(
            &client,
            &target,
            &document_a,
            TextSubresourcePolicy::SameOriginFetch,
        )?;
        let third = fetch_text_subresource(
            &client,
            &target,
            &document_c,
            TextSubresourcePolicy::SameOriginFetch,
        )?;

        assert_eq!(first.cache_status(), phantom_net::CacheStatus::Miss);
        assert_eq!(second.cache_status(), phantom_net::CacheStatus::Fresh);
        assert_eq!(third.cache_status(), phantom_net::CacheStatus::Miss);

        thread::sleep(Duration::from_millis(20));
        let cache_requests = cors_server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| {
                events
                    .iter()
                    .filter(|event| event.starts_with("request: /2n7-cache-star.txt"))
                    .cloned()
                    .collect::<Vec<_>>()
            });
        assert_eq!(cache_requests.len(), 2, "events={cache_requests:?}");

        let slow_target = format!("{}/2n7-slow-star.txt", cors_server.address);
        let mut tab = BrowserTab::new();
        let document_url = document_a.as_str().to_owned();
        tab.engine.load_html(&format!(
            "<p id='target'>OLD</p><script>fetch('{slow_target}').then(function(response) {{ return response.text(); }}).then(function(text) {{ document.getElementById('target').textContent = text; }});</script>"
        ))?;
        tab.address = document_url.clone();
        tab.document_generation = 7;
        execute_committed_page_scripts(&mut tab, &document_url, 1.0)?;
        tab.mark_navigation_ready();
        poll_page_fetches(&mut tab, 1.0);
        assert!(!tab.pending_script_fetches.is_empty());

        tab.clear_navigation_state();
        tab.document_generation = 8;
        tab.engine.load_html("<p id='target'>NEW GENERATION</p>")?;
        tab.address = format!("{}/next.html", origin_a.address);
        tab.mark_navigation_ready();

        for _ in 0..100 {
            poll_page_fetches(&mut tab, 1.0);
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        assert!(painted.contains("NEW GENERATION"), "{painted}");
        assert!(!painted.contains("CORS SLOW OLD RESULT"), "{painted}");
        assert!(tab.pending_script_fetches.is_empty());
        Ok(())
    }

    // PHANTOM_2N8_CORS_REDIRECT_ADMISSION_I
    #[test]
    fn cors_redirect_hop_requires_acao_before_following_location()
    -> Result<(), Box<dyn std::error::Error>> {
        let origin_server = TextSubresourcePolicyServer::new()?;
        let document = HttpUrl::parse(&format!("{}/2n5-page.html", origin_server.address))?;
        let cors_server = CorsTransportServer::new(document.origin().as_str().to_owned())?;
        let client = NetworkClient::with_max_body_bytes(MAX_SCRIPT_FETCH_BODY_BYTES);

        for (path, expected) in [
            ("/2n8-redirect-star.txt", "CORS REDIRECT STAR GREEN"),
            ("/2n8-redirect-exact.txt", "CORS REDIRECT EXACT GREEN"),
        ] {
            let target = HttpUrl::parse(&format!("{}{path}", cors_server.address))?;
            let response = fetch_text_subresource(
                &client,
                &target,
                &document,
                TextSubresourcePolicy::SameOriginFetch,
            )?;
            assert_eq!(response.body(), expected);
            assert_eq!(response.redirect_count(), 1);
        }

        for path in [
            "/2n8-redirect-no-acao.txt",
            "/2n8-redirect-wrong-origin.txt",
        ] {
            let target = HttpUrl::parse(&format!("{}{path}", cors_server.address))?;
            let result = fetch_text_subresource(
                &client,
                &target,
                &document,
                TextSubresourcePolicy::SameOriginFetch,
            );
            let error = match result {
                Err(error) => error,
                Ok(_) => {
                    return Err(std::io::Error::other(format!(
                        "{path} unexpectedly followed an unauthorized CORS redirect"
                    ))
                    .into());
                }
            };
            assert!(matches!(error, NetworkError::CorsRedirectBlocked { .. }));
        }

        thread::sleep(Duration::from_millis(30));
        let events = cors_server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());
        let denied_final_requests = events
            .iter()
            .filter(|event| event.starts_with("request: /2n8-denied-final.txt"))
            .count();
        assert_eq!(denied_final_requests, 0, "events={events:?}");
        Ok(())
    }

    #[test]
    fn cors_redirect_denial_rejects_javascript_fetch_without_body_leak()
    -> Result<(), Box<dyn std::error::Error>> {
        let origin_server = TextSubresourcePolicyServer::new()?;
        let document_url = format!("{}/2n5-page.html", origin_server.address);
        let document = HttpUrl::parse(&document_url)?;
        let cors_server = CorsTransportServer::new(document.origin().as_str().to_owned())?;
        let target = format!("{}/2n8-redirect-no-acao.txt", cors_server.address);

        let mut tab = BrowserTab::new();
        tab.engine.load_html(&format!(
            "<p id='target'>Before</p><script>fetch('{target}').then(function(response) {{ return response.text(); }}).then(function(text) {{ document.getElementById('target').textContent = text; }}).catch(function() {{ document.getElementById('target').textContent = 'CORS REDIRECT BLOCKED'; }});</script>"
        ))?;
        tab.address = document_url.clone();
        execute_committed_page_scripts(&mut tab, &document_url, 1.0)?;
        tab.mark_navigation_ready();

        for _ in 0..120 {
            poll_page_fetches(&mut tab, 1.0);
            let painted = corpus_painted_text(&tab);
            if painted.contains("CORS REDIRECT BLOCKED")
                || painted.contains("CORS REDIRECT DENIED FINAL MUST NOT LOAD")
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        assert!(painted.contains("CORS REDIRECT BLOCKED"), "{painted}");
        assert!(
            !painted.contains("CORS REDIRECT DENIED FINAL MUST NOT LOAD"),
            "{painted}"
        );
        assert!(tab.pending_script_fetches.is_empty());

        thread::sleep(Duration::from_millis(20));
        let events = cors_server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());
        assert!(
            !events
                .iter()
                .any(|event| event.starts_with("request: /2n8-denied-final.txt")),
            "events={events:?}"
        );
        Ok(())
    }

    // PHANTOM_2N5_TEXT_SUBRESOURCE_POLICY_II
    #[test]
    fn text_subresource_policy_requires_cors_and_blocks_private_classic_targets()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = TextSubresourcePolicyServer::new()?;
        let target = HttpUrl::parse(&format!("{}/2n5-probe.txt", server.address))?;
        let foreign_origin = HttpUrl::parse("http://127.0.0.1:9/document.html")?;
        let public_origin = HttpUrl::parse("https://public.example/document.html")?;
        let client = NetworkClient::with_max_body_bytes(MAX_SCRIPT_FETCH_BODY_BYTES);

        assert!(fetch_text_subresource(
            &client,
            &target,
            &foreign_origin,
            TextSubresourcePolicy::SameOriginFetch,
        ).is_err());
        assert!(fetch_text_subresource(
            &client,
            &target,
            &public_origin,
            TextSubresourcePolicy::ClassicScript,
        ).is_err());
        assert!(fetch_text_subresource(
            &client,
            &target,
            &public_origin,
            TextSubresourcePolicy::Stylesheet,
        ).is_err());

        thread::sleep(Duration::from_millis(30));
        let events = server.events.lock().map_or_else(|_| Vec::new(), |events| events.clone());
        assert_eq!(
            events
                .iter()
                .filter(|event| event.as_str() == "request: /2n5-probe.txt")
                .count(),
            1,
            "events={events:?}"
        );
        Ok(())
    }

    #[test]
    fn unified_text_subresource_client_caches_css_script_and_revalidates_fetch()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = TextSubresourcePolicyServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/2n5-page.html", server.address);

        for pass in 0..2 {
            start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);
            let deadline = Instant::now() + Duration::from_secs(8);
            while Instant::now() < deadline {
                poll_tab_navigation(&mut tab, &network, 1.0);
                poll_pending_external_stylesheets(&mut tab, &network, 1.0);
                poll_pending_page_scripts(&mut tab, 1.0);
                poll_page_fetches(&mut tab, 1.0);
                let painted = corpus_painted_text(&tab);
                if tab.navigation_phase() == NavigationPhase::Failed { break; }
                if tab.navigation_phase() == NavigationPhase::Ready
                    && painted.contains("2N5 SCRIPT GREEN")
                    && painted.contains("2N5 FETCH GREEN")
                    && tab.pending_external_stylesheets.is_none()
                    && tab.pending_page_scripts.is_none()
                    && tab.pending_script_fetches.is_empty()
                {
                    break;
                }
                thread::sleep(Duration::from_millis(3));
            }
            assert_eq!(tab.navigation_phase(), NavigationPhase::Ready, "pass={pass}: {}", tab.status);
            let painted = corpus_painted_text(&tab);
            assert!(painted.contains("2N5 SCRIPT GREEN"), "pass={pass}: {painted:?}");
            assert!(painted.contains("2N5 FETCH GREEN"), "pass={pass}: {painted:?}");
        }

        let policy_node = tab.engine.document().nodes().find_map(|node| match node.kind() {
            phantom_engine::NodeKind::Element(element) if element.attribute("id") == Some("policy") => {
                Some(node.id())
            }
            _ => None,
        }).ok_or(std::io::Error::other("2N5 policy node missing"))?;
        let width = tab.engine.layout().boxes().iter()
            .filter(|layout_box| layout_box.source_node() == policy_node)
            .map(|layout_box| layout_box.rect().width())
            .fold(0.0_f32, f32::max);
        assert!(width >= 330.0, "2N5 stylesheet width={width}");

        let events = server.events.lock().map_or_else(|_| Vec::new(), |events| events.clone());
        let count = |needle: &str| events.iter().filter(|event| event.as_str() == needle).count();
        eprintln!(
            "2N5 DIAG phase={:?} status={:?} address={:?} painted={:?}",
            tab.navigation_phase(), tab.status, tab.address, corpus_painted_text(&tab),
        );
        eprintln!("2N5 DIAG server={events:?}");
        assert_eq!(count("request: /2n5-page.html"), 2, "events={events:?}");
        assert_eq!(count("request: /2n5-policy.css"), 1, "events={events:?}");
        assert_eq!(count("request: /2n5-policy.js"), 1, "events={events:?}");
        assert_eq!(count("request: /2n5-data.txt"), 2, "events={events:?}");
        assert!(events.iter().any(|event| event == "revalidate: if-none-match=\"2n5-data-v1\""));
        assert!(events.iter().any(|event| event.contains("response: /2n5-data.txt; 304 Not Modified")));
        Ok(())
    }

    // PHANTOM_2N4_CSS_IMPORT_GRAPH_I
    #[test]
    fn css_import_scanner_is_leading_bounded_and_conservative() {
        let source = r#"
            /* before */
            @charset "utf-8";
            @import "a.css";
            @import url(b.css);
            @import "media.css" screen;
            .rule { color: red; }
            @import "late.css";
        "#;
        let imports = leading_css_import_statements(source);
        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].href.as_deref(), Some("a.css"));
        assert_eq!(imports[1].href.as_deref(), Some("b.css"));
        assert_eq!(imports[2].href.as_deref(), None);
        assert!(source[imports[0].start..imports[0].end].contains("@import"));
        assert!(!imports.iter().any(|statement| {
            source[statement.start..statement.end].contains("late.css")
        }));
    }

    #[test]
    fn external_stylesheet_import_graph_resolves_nested_cache_cycles_and_limits()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = StylesheetImportServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/2n4-page.html", server.address);

        for pass in 0..2 {
            start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);
            let deadline = Instant::now() + Duration::from_secs(8);
            while Instant::now() < deadline {
                poll_tab_navigation(&mut tab, &network, 1.0);
                poll_pending_external_stylesheets(&mut tab, &network, 1.0);
                poll_pending_page_scripts(&mut tab, 1.0);
                poll_page_fetches(&mut tab, 1.0);
                let painted = corpus_painted_text(&tab);
                if tab.navigation_phase() == NavigationPhase::Failed {
                    break;
                }
                if tab.navigation_phase() == NavigationPhase::Ready
                    && painted.contains("2N4 IMPORT GREEN")
                    && painted.contains("SCRIPT AFTER IMPORTS")
                    && tab.pending_external_stylesheets.is_none()
                {
                    break;
                }
                thread::sleep(Duration::from_millis(3));
            }
            assert_eq!(tab.navigation_phase(), NavigationPhase::Ready, "pass={pass}: {}", tab.status);
            let painted = corpus_painted_text(&tab);
            assert!(painted.contains("2N4 IMPORT GREEN"), "pass={pass}: {painted:?}");
            assert!(painted.contains("DEEP GREEN"), "pass={pass}: {painted:?}");
            assert!(painted.contains("REDIRECT GREEN"), "pass={pass}: {painted:?}");
            assert!(painted.contains("BUDGET GREEN"), "pass={pass}: {painted:?}");
            assert!(painted.contains("SCRIPT AFTER IMPORTS"), "pass={pass}: {painted:?}");
            assert!(tab.status.contains("CSS externo 27 ok / 5 falha"), "pass={pass}: {}", tab.status);
        }

        let node_for_id = |id: &str| {
            tab.engine.document().nodes().find_map(|node| match node.kind() {
                phantom_engine::NodeKind::Element(element) if element.attribute("id") == Some(id) => {
                    Some(node.id())
                }
                _ => None,
            })
        };
        let imported = node_for_id("imported")
            .ok_or(std::io::Error::other("2N4 imported node missing"))?;
        let deep = node_for_id("deep")
            .ok_or(std::io::Error::other("2N4 deep node missing"))?;
        let redirected = node_for_id("redirected")
            .ok_or(std::io::Error::other("2N4 redirected node missing"))?;
        let order = node_for_id("order")
            .ok_or(std::io::Error::other("2N4 order node missing"))?;
        let cycle = node_for_id("cycle")
            .ok_or(std::io::Error::other("2N4 cycle node missing"))?;
        let limit = node_for_id("limit")
            .ok_or(std::io::Error::other("2N4 limit node missing"))?;
        let budget_node = node_for_id("budget")
            .ok_or(std::io::Error::other("2N4 budget node missing"))?;

        let color_of = |node_id| {
            tab.engine.styles().get(node_id).map(|style| {
                let color = style.color();
                (color.red(), color.green(), color.blue())
            })
        };
        assert_eq!(color_of(imported), Some((12, 120, 30)));
        assert_eq!(color_of(deep), Some((110, 40, 170)));
        assert_eq!(color_of(redirected), Some((20, 130, 160)));
        assert_eq!(color_of(order), Some((30, 40, 200)));
        assert_eq!(color_of(cycle), Some((40, 130, 60)));
        assert_eq!(color_of(limit), Some((25, 140, 70)));
        assert_eq!(color_of(budget_node), Some((45, 115, 145)));

        let imported_width = tab
            .engine
            .layout()
            .boxes()
            .iter()
            .filter(|layout_box| layout_box.source_node() == imported)
            .map(|layout_box| layout_box.rect().width())
            .fold(0.0_f32, f32::max);
        let cycle_width = tab
            .engine
            .layout()
            .boxes()
            .iter()
            .filter(|layout_box| layout_box.source_node() == cycle)
            .map(|layout_box| layout_box.rect().width())
            .fold(0.0_f32, f32::max);
        let budget_width = tab
            .engine
            .layout()
            .boxes()
            .iter()
            .filter(|layout_box| layout_box.source_node() == budget_node)
            .map(|layout_box| layout_box.rect().width())
            .fold(0.0_f32, f32::max);
        assert!(imported_width >= 300.0, "2N4 imported width={imported_width}");
        assert!(cycle_width >= 240.0, "2N4 cycle width={cycle_width}");
        assert!(budget_width >= 280.0, "2N4 budget width={budget_width}");

        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());
        let count = |needle: &str| events.iter().filter(|event| event.as_str() == needle).count();
        eprintln!(
            "2N4 DIAG phase={:?} status={:?} address={:?} painted={:?}",
            tab.navigation_phase(),
            tab.status,
            tab.address,
            corpus_painted_text(&tab),
        );
        eprintln!("2N4 DIAG server={events:?}");

        assert_eq!(count("request: /2n4-page.html"), 2, "events={events:?}");
        assert_eq!(count("request: /2n4-root.css"), 1, "events={events:?}");
        assert_eq!(count("request: /2n4-budget-root.css"), 1, "events={events:?}");
        assert_eq!(count("request: /2n4-budget-child.css"), 1, "events={events:?}");
        assert_eq!(count("request: /nested/base.css"), 1, "events={events:?}");
        assert_eq!(count("request: /nested/deep/deep.css"), 2, "events={events:?}");
        assert_eq!(count("request: /cycle-a.css"), 1, "events={events:?}");
        assert_eq!(count("request: /cycle-b.css"), 1, "events={events:?}");
        assert_eq!(count("request: /limit-4.css"), 1, "events={events:?}");
        assert_eq!(count("request: /limit-5.css"), 0, "events={events:?}");
        assert!(events.iter().any(|event| event == "request: /2n4-redirect.css"));
        assert!(events.iter().any(|event| event == "request: /2n4-redirect-final.css"));
        assert!(events.iter().any(|event| event == "request: /2n4-missing.css"));
        assert!(events.iter().any(|event| event == "request: /2n4-wrong.css"));
        assert!(events.iter().any(|event| event == "revalidate: if-none-match=\"2n4-deep-v1\""));
        assert!(events.iter().any(|event| {
            event.contains("response: /nested/deep/deep.css; 304 Not Modified")
        }));
        assert!(tab.pending_external_stylesheets.is_none());
        Ok(())
    }

    // PHANTOM_2N3_STYLESHEET_LIFECYCLE_CACHE_III
    #[test]
    fn external_stylesheet_cache_revalidation_media_and_generation_isolation()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = StylesheetLifecycleServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let cache_url = format!("{}/2n3-cache.html", server.address);

        for pass in 0..2 {
            start_navigation(&network, &mut tab, cache_url.clone(), NavigationAction::New);
            let deadline = Instant::now() + Duration::from_secs(8);
            while Instant::now() < deadline {
                poll_tab_navigation(&mut tab, &network, 1.0);
                poll_pending_external_stylesheets(&mut tab, &network, 1.0);
                poll_pending_page_scripts(&mut tab, 1.0);
                poll_page_fetches(&mut tab, 1.0);
                let painted = corpus_painted_text(&tab);
                if tab.navigation_phase() == NavigationPhase::Failed { break; }
                if tab.navigation_phase() == NavigationPhase::Ready
                    && painted.contains("2N3 CACHE GREEN")
                    && tab.pending_external_stylesheets.is_none() { break; }
                thread::sleep(Duration::from_millis(3));
            }
            assert_eq!(tab.navigation_phase(), NavigationPhase::Ready, "pass={pass}");
            assert!(corpus_painted_text(&tab).contains("2N3 CACHE GREEN"));
        }

        let responsive = tab.engine.document().nodes().find_map(|node| match node.kind() {
            phantom_engine::NodeKind::Element(element) if element.attribute("id") == Some("responsive") => Some(node.id()),
            _ => None,
        }).ok_or(std::io::Error::other("2N3 responsive node missing"))?;
        let wide_color = tab.engine.styles().get(responsive).map(|style| style.color())
            .ok_or(std::io::Error::other("2N3 wide style missing"))?;
        assert_eq!((wide_color.red(), wide_color.green(), wide_color.blue()), (198, 40, 40));
        tab.engine.relayout_with_viewport(600.0, 768.0)?;
        let narrow_color = tab.engine.styles().get(responsive).map(|style| style.color())
            .ok_or(std::io::Error::other("2N3 narrow style missing"))?;
        assert_eq!((narrow_color.red(), narrow_color.green(), narrow_color.blue()), (21, 101, 192));
        tab.engine.relayout_with_viewport(1024.0, 768.0)?;

        let events_after_cache = server.events.lock().map_or_else(|_| Vec::new(), |events| events.clone());
        let cached_requests = events_after_cache.iter().filter(|event| event.as_str() == "request: /2n3-cached.css").count();
        let revalidate_requests = events_after_cache.iter().filter(|event| event.as_str() == "request: /2n3-revalidate.css").count();
        assert_eq!(cached_requests, 1, "events={events_after_cache:?}");
        assert_eq!(revalidate_requests, 2, "events={events_after_cache:?}");
        assert!(events_after_cache.iter().any(|event| event == "revalidate: if-none-match=\"2n3-v1\""));
        assert!(events_after_cache.iter().any(|event| event.contains("response: /2n3-revalidate.css; 304 Not Modified")));

        let slow_url = format!("{}/2n3-slow.html", server.address);
        start_navigation(&network, &mut tab, slow_url, NavigationAction::New);
        let pending_deadline = Instant::now() + Duration::from_secs(8);
        let mut slow_generation = 0_u64;
        while Instant::now() < pending_deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            if tab.pending_external_stylesheets.is_some() {
                slow_generation = tab.document_generation;
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }
        assert_ne!(slow_generation, 0, "slow stylesheet phase was not observed");

        let clean_url = format!("{}/2n3-clean.html", server.address);
        start_navigation(&network, &mut tab, clean_url.clone(), NavigationAction::New);
        assert!(tab.document_generation > slow_generation);
        assert!(tab.pending_external_stylesheets.is_none());

        let clean_deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < clean_deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_external_stylesheets(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            poll_page_fetches(&mut tab, 1.0);
            if tab.navigation_phase() == NavigationPhase::Failed { break; }
            if tab.navigation_phase() == NavigationPhase::Ready
                && corpus_painted_text(&tab).contains("2N3 CLEAN GREEN") { break; }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let events = server.events.lock().map_or_else(|_| Vec::new(), |events| events.clone());
        eprintln!(
            "2N3 DIAG generation={} phase={:?} status={:?} address={:?} painted={painted:?}",
            tab.document_generation, tab.navigation_phase(), tab.status, tab.address,
        );
        eprintln!("2N3 DIAG server={events:?}");
        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, clean_url);
        assert!(painted.contains("2N3 CLEAN GREEN"));
        assert!(!painted.contains("STALE STYLE APPLIED"));
        assert!(tab.pending_external_stylesheets.is_none());
        assert!(events.iter().any(|event| event == "request: /2n3-slow.css"));
        assert!(events.iter().any(|event| event == "request: /2n3-clean.html"));
        Ok(())
    }

    // PHANTOM_2N2_EXTERNAL_STYLESHEETS_I
    #[test]
    fn external_stylesheets_load_before_scripts_preserve_order_and_survive_partial_failure()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = ExternalStyleServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let url = format!("{}/2n2-page.html", server.address);

        start_navigation(&network, &mut tab, url.clone(), NavigationAction::New);

        let deadline = Instant::now() + Duration::from_secs(8);
        let mut observed_stylesheet_phase = false;
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            observed_stylesheet_phase |= tab.pending_external_stylesheets.is_some();
            poll_pending_external_stylesheets(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            poll_page_fetches(&mut tab, 1.0);

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("SCRIPT AFTER CSS")
                && tab.pending_external_stylesheets.is_none()
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());
        eprintln!(
            "2N2 DIAG generation={} stylesheet_phase={} phase={:?} status={:?} address={:?} painted={painted:?}",
            tab.document_generation,
            observed_stylesheet_phase,
            tab.navigation_phase(),
            tab.status,
            tab.address,
        );
        eprintln!("2N2 DIAG server={events:?}");

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, url);
        assert!(observed_stylesheet_phase, "stylesheet phase was not observed");
        assert!(painted.contains("SCRIPT AFTER CSS"), "painted={painted:?}");

        let target = tab
            .engine
            .document()
            .nodes()
            .find_map(|node| match node.kind() {
                phantom_engine::NodeKind::Element(element)
                    if element.attribute("id") == Some("target") =>
                {
                    Some(node.id())
                }
                _ => None,
            })
            .ok_or(std::io::Error::other("2N2 target missing"))?;
        let order = tab
            .engine
            .document()
            .nodes()
            .find_map(|node| match node.kind() {
                phantom_engine::NodeKind::Element(element)
                    if element.attribute("id") == Some("order") =>
                {
                    Some(node.id())
                }
                _ => None,
            })
            .ok_or(std::io::Error::other("2N2 order target missing"))?;
        let layout_node = tab
            .engine
            .document()
            .nodes()
            .find_map(|node| match node.kind() {
                phantom_engine::NodeKind::Element(element)
                    if element.attribute("id") == Some("layout") =>
                {
                    Some(node.id())
                }
                _ => None,
            })
            .ok_or(std::io::Error::other("2N2 layout target missing"))?;

        let target_color = tab
            .engine
            .styles()
            .get(target)
            .map(|style| style.color())
            .ok_or(std::io::Error::other("2N2 target style missing"))?;
        assert_eq!(
            (target_color.red(), target_color.green(), target_color.blue()),
            (46, 125, 50)
        );
        let order_color = tab
            .engine
            .styles()
            .get(order)
            .map(|style| style.color())
            .ok_or(std::io::Error::other("2N2 order style missing"))?;
        assert_eq!(
            (order_color.red(), order_color.green(), order_color.blue()),
            (106, 27, 154)
        );
        let layout_width = tab
            .engine
            .layout()
            .boxes()
            .iter()
            .filter(|layout_box| layout_box.source_node() == layout_node)
            .map(|layout_box| layout_box.rect().width())
            .fold(0.0_f32, f32::max);
        assert!(layout_width >= 320.0, "2N2 layout width={layout_width}");

        for expected in [
            "request: /2n2-page.html",
            "request: /2n2-missing.css",
            "request: /2n2-redirect.css",
            "request: /2n2-main.css",
            "request: /2n2-late.css",
            "request: /2n2-wrong.css",
        ] {
            assert!(
                events.iter().any(|event| event == expected),
                "missing server event {expected:?}: {events:?}"
            );
        }
        assert!(!events.iter().any(|event| event == "request: /2n2-alternate.css"));
        assert!(!events.iter().any(|event| event == "request: /2n2-print.css"));
        assert!(tab.pending_external_stylesheets.is_none());
        assert!(tab.pending_page_scripts.is_none());

        Ok(())
    }

    // PHANTOM_2N1_REAL_WEB_NETWORK_I
    #[test]
    fn real_web_network_pipeline_handles_redirect_gzip_defer_and_partial_failure()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = RealWebServer::new()?;
        let network = NetworkClient::new();
        let mut tab = BrowserTab::new();
        let entry_url = format!("{}/2n1-entry", server.address);
        let final_url = format!("{}/2n1-page.html", server.address);

        start_navigation(
            &network,
            &mut tab,
            entry_url,
            NavigationAction::New,
        );

        let deadline = Instant::now() + Duration::from_secs(8);
        let mut peak_fetches = 0_usize;
        while Instant::now() < deadline {
            poll_tab_navigation(&mut tab, &network, 1.0);
            poll_pending_page_scripts(&mut tab, 1.0);
            poll_page_fetches(&mut tab, 1.0);
            peak_fetches = peak_fetches.max(tab.pending_script_fetches.len());

            let painted = corpus_painted_text(&tab);
            if tab.navigation_phase() == NavigationPhase::Failed {
                break;
            }
            if tab.navigation_phase() == NavigationPhase::Ready
                && painted.contains("2N1 REAL WEB GREEN network-data")
                && tab.pending_page_scripts.is_none()
                && tab.pending_script_fetches.is_empty()
            {
                break;
            }
            thread::sleep(Duration::from_millis(3));
        }

        let painted = corpus_painted_text(&tab);
        let events = server
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());

        eprintln!(
            "2N1 DIAG generation={} peak_fetches={} phase={:?} status={:?} address={:?} painted={painted:?}",
            tab.document_generation,
            peak_fetches,
            tab.navigation_phase(),
            tab.status,
            tab.address,
        );
        eprintln!("2N1 DIAG server={events:?}");

        assert_eq!(tab.navigation_phase(), NavigationPhase::Ready);
        assert_eq!(tab.address, final_url);
        assert!(
            painted.contains("2N1 REAL WEB GREEN network-data"),
            "painted={painted:?}"
        );
        assert!(
            painted.contains("Deferred legacy MIME loaded"),
            "painted={painted:?}"
        );
        assert!(peak_fetches >= 1, "2N1 page fetch path was not observed");
        for expected in [
            "request: /2n1-entry",
            "request: /2n1-page.html",
            "request: /2n1-missing.js",
            "request: /2n1-deferred.js",
            "request: /2n1-data.txt",
        ] {
            assert!(
                events.iter().any(|event| event == expected),
                "missing server event {expected:?}: {events:?}"
            );
        }
        assert!(tab.pending_page_scripts.is_none());
        assert!(tab.pending_script_fetches.is_empty());

        Ok(())
    }

}
