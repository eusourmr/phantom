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

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use lucide_icons::{Icon, LUCIDE_FONT_BYTES};
use phantom_engine::{
    Engine, FormControlId, FormControlKind, FormSubmissionError, ImageLoading, ObjectFit,
    ObjectPosition, PaintColor, PaintCommand, PaintFontFamily, PaintFontStyle, PaintFontWeight,
    PaintList, PaintRect, PaintTextRange, ResourcePriority,
};
use phantom_image::{
    AnimatedImageDecoder, AnimationDecodeLimits, AnimationLoopCount, DecodedAnimation,
    DecodedImage, ImageDecodeLimits, ImageDecoder, ImageMetadata, ImageResourceId,
    RasterImageDecoder, image_is_animated,
};
use phantom_net::{
    BinaryResponse, DocumentLoadError, DocumentResponse, HttpUrl, NetworkClient,
    NetworkIsolationKey,
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
            .exact_size(44.0)
            .frame(
                egui::Frame::new()
                    .fill(panel_fill)
                    .inner_margin(egui::Margin::symmetric(7, 5)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, tab) in self.tabs.iter().enumerate() {
                        let is_active = index == self.active_tab;
                        let title = tab_title(tab);
                        let width = if tab.pinned { 42.0 } else { 168.0 };
                        let fill = if is_active {
                            ui.visuals().extreme_bg_color
                        } else {
                            ui.visuals().faint_bg_color
                        };

                        let tab_frame = egui::Frame::new()
                            .fill(fill)
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(5, 3))
                            .show(ui, |ui| {
                                ui.set_width(width);

                                ui.horizontal(|ui| {
                                    let has_site_icon = tab.site_icon.is_some();
                                    let tab_response = if tab.pinned {
                                        if let Some(site_icon) = &tab.site_icon {
                                            let (rect, response) = ui.allocate_exact_size(
                                                egui::vec2(28.0, 24.0),
                                                egui::Sense::click(),
                                            );
                                            let icon_rect = egui::Rect::from_center_size(
                                                rect.center(),
                                                egui::vec2(18.0, 18.0),
                                            );
                                            ui.painter().image(
                                                site_icon.id(),
                                                icon_rect,
                                                unit_uv(),
                                                egui::Color32::WHITE,
                                            );
                                            response
                                        } else {
                                            ui.add_sized(
                                                [28.0, 24.0],
                                                egui::Button::new(lucide_text(Icon::Pin, 14.0))
                                                    .frame(false),
                                            )
                                        }
                                    } else {
                                        if let Some(site_icon) = &tab.site_icon {
                                            ui.image((site_icon.id(), egui::vec2(16.0, 16.0)));
                                        }

                                        let title_width = if has_site_icon {
                                            (width - 56.0).max(64.0)
                                        } else {
                                            (width - 36.0).max(80.0)
                                        };

                                        ui.add_sized(
                                            [title_width, 24.0],
                                            egui::Button::new(
                                                egui::RichText::new(title.clone()).size(13.0),
                                            )
                                            .frame(false),
                                        )
                                    };
                                    let close_clicked = !tab.pinned
                                        && icon_button(ui, Icon::X, true, [25.0, 24.0], 13.0)
                                            .on_hover_text("Fechar aba · Ctrl+W")
                                            .clicked();

                                    (tab_response, close_clicked)
                                })
                                .inner
                            });

                        let (tab_response, close_clicked) = tab_frame.inner;
                        let tab_response = tab_response.on_hover_text(if tab.pinned {
                            format!("{} · aba fixada", tab.title)
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
                    }

                    if icon_button(ui, Icon::Plus, true, [31.0, 30.0], 16.0)
                        .on_hover_text("Nova aba · Ctrl+T")
                        .clicked()
                    {
                        create_tab = true;
                    }

                    const WINDOW_CONTROL_WIDTH: f32 = 38.0;
                    const WINDOW_CONTROLS_GAP: f32 = 2.0;
                    const WINDOW_CONTROLS_LEFT_GAP: f32 = 8.0;
                    const WINDOW_CONTROLS_RIGHT_PADDING: f32 = 20.0;

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
                    let drag_width = (ui.available_width() - controls_width).max(16.0);
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
    fn empty_page(&self, ui: &mut egui::Ui, tab: &BrowserTab) {
        ui.add_space(70.0);

        ui.vertical_centered(|ui| {
            if let Some(logo) = &self.page_logo {
                ui.image((logo.id(), egui::vec2(172.0, 172.0)));
                ui.add_space(14.0);
            }

            ui.heading("Nova aba");
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Phantom Independent Web Browser").size(17.0));
            ui.add_space(14.0);
            ui.label(egui::RichText::new(&tab.status).small().weak());
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
        let heading = match tab.loading_action() {
            Some(NavigationAction::New) => "Abrindo página",
            Some(NavigationAction::History(_)) => "Restaurando histórico",
            Some(NavigationAction::Reload) => "Recarregando página",
            None => "Carregando",
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
            ui.add_space(6.0);
            ui.label(egui::RichText::new(tab.address.as_str()).small().weak());
        });
    }

    // PHANTOM_2C12_LINK_INTERACTION_UX
    // PHANTOM_2C13_BROWSER_INPUTS_I
    fn rendered_page(&mut self, ui: &mut egui::Ui) {
        let active_index = self.active_tab;

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

        let (visible_range, hovered_link, link_activation, form_activation) = {
            let tab = &mut self.tabs[active_index];
            let engine = &tab.engine;
            let paint = engine.paint_list();
            let image_textures = &tab.image_textures;
            let form_values = &mut tab.form_values;

            let document_width = paint.viewport_width().max(1.0);
            let document_height = paint.content_height().max(1.0);
            let mut visible_range = (0.0_f32, ui.available_height().max(1.0));
            let mut hovered_link = None::<String>;
            let mut link_activation = None::<(String, bool)>;
            let mut form_activation = None;
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

                    paint_page(ui, origin, paint, image_textures);

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
                                if control.enabled()
                                    && response.lost_focus()
                                    && ui.input(|input| input.key_pressed(egui::Key::Enter))
                                {
                                    form_activation = Some((control.form(), None));
                                }
                            }

                            FormControlKind::Submit => {
                                if control.enabled() && response.clicked() {
                                    form_activation = Some((control.form(), Some(control.id())));
                                }
                            }
                        }
                    }

                    if !pointer_over_control
                        && canvas_response.hovered()
                        && let Some(pointer) = ui.ctx().pointer_hover_pos()
                        && let Some(link) =
                            engine.link_at(pointer.x - origin.x, pointer.y - origin.y)
                    {
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

                        if canvas_response.clicked()
                            && let Some(target) = resolved
                        {
                            link_activation = Some((
                                target.as_str().to_owned(),
                                link.opens_new_context() || command_modifier,
                            ));
                        }
                    }
                });

            (
                visible_range,
                hovered_link,
                link_activation,
                form_activation,
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

        if let Some((form, submitter)) = form_activation {
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
        } else if let Some((target, open_new_context)) = link_activation {
            if open_new_context {
                self.open_new_tab();
                let tab = self.active_tab_mut();
                start_navigation(&network, tab, target, NavigationAction::New);
            } else if active_index < self.tabs.len() {
                let tab = &mut self.tabs[active_index];
                start_navigation(&network, tab, target, NavigationAction::New);
            }
        }
    }
    fn floating_navigation(&mut self, context: &egui::Context) {
        let pointer_in_zone = pointer_in_navigation_zone(context);
        let visible = self.floating_bar_forced || pointer_in_zone;

        if !visible {
            return;
        }

        let bar_width = (context.content_rect().width() * 0.66).clamp(440.0, 920.0);
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

        egui::Area::new(egui::Id::new("phantom-floating-navigation"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
            .order(egui::Order::Foreground)
            .show(context, |ui| {
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .corner_radius(18)
                    .inner_margin(egui::Margin::symmetric(12, 10))
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
                    tab.mark_navigation_ready();
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

                    tab.status = format!(
                        "HTTP {status_code} · {body_bytes} bytes · {} nós · {} caixas · {} comandos",
                        tab.engine.document().len(),
                        tab.engine.layout().len(),
                        tab.engine.paint_list().len()
                    );

                    start_site_icon_loading(network, tab, &final_url);
                    start_image_loading(network, tab, &final_url, device_pixel_ratio);
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
        .filter_map(|command| {
            let PaintCommand::Image { rect, resource, .. } = command else {
                return None;
            };
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

fn paint_page(
    ui: &mut egui::Ui,
    origin: egui::Pos2,
    paint: &PaintList,
    image_textures: &BTreeMap<ImageResourceId, ImageTextureBinding>,
) {
    let now = Instant::now();

    for command in paint.commands() {
        match command {
            PaintCommand::FillRect { rect, color } => {
                let target = egui_rect(origin, *rect);

                ui.painter().rect_filled(target, 0.0, egui_color(*color));
            }

            PaintCommand::Image {
                rect,
                resource,
                alt,
                fit,
                position,
            } => {
                let target = egui_rect(origin, *rect);

                if let Some(texture) = image_textures
                    .get(resource)
                    .and_then(|binding| binding.texture_at(now))
                {
                    paint_fitted_image(ui, texture, target, *fit, *position);
                } else {
                    paint_image_placeholder(ui, target, paint, *alt);
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

                ui.put(egui_rect(origin, *rect), egui::Label::new(rich_text).wrap());
            }
        }
    }
}

fn paint_fitted_image(
    ui: &egui::Ui,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    fit: ObjectFit,
    position: ObjectPosition,
) {
    let [width, height] = texture.size();
    let source = egui::vec2(width as f32, height as f32);

    if source.x <= 0.0 || source.y <= 0.0 || target.width() <= 0.0 || target.height() <= 0.0 {
        return;
    }

    let (draw_rect, uv) = object_geometry(target, source, fit, position);
    ui.painter()
        .image(texture.id(), draw_rect, uv, egui::Color32::WHITE);
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
) {
    ui.painter()
        .rect_filled(target, 0.0, egui::Color32::from_gray(36));

    if let Some(alt_range) = alt
        && let Some(content) = paint.text(alt_range)
        && !content.is_empty()
    {
        ui.put(
            target,
            egui::Label::new(
                egui::RichText::new(content)
                    .size(13.0)
                    .color(egui::Color32::from_gray(190)),
            )
            .wrap(),
        );
    }
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
}
