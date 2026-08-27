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

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

use eframe::egui;
use lucide_icons::{Icon, LUCIDE_FONT_BYTES};
use phantom_engine::{
    Engine, ObjectFit, ObjectPosition, PaintColor, PaintCommand, PaintFontFamily,
    PaintFontStyle, PaintFontWeight, PaintList, PaintRect, PaintTextRange,
};
use phantom_image::{
    DecodedImage, ImageDecodeLimits, ImageDecoder, ImageMetadata,
    ImageResourceId, RasterImageDecoder,
};
use phantom_net::{HttpUrl, NetworkClient, NetworkError, TextResponse};

const APP_NAME: &str = "Phantom";
const APP_VERSION: &str = "0.0.1";
const LUCIDE_FONT_NAME: &str = "phantom-lucide";
const MAX_IMAGES_PER_DOCUMENT: usize = 64;
const MAX_TAB_RASTER_BYTES: u64 = 256 * 1024 * 1024;

const PAGE_LOGO_BYTES: &[u8] =
    include_bytes!("../assets/branding/phantom-logo.png");

const APP_ICON_BYTES: &[u8] =
    include_bytes!("../assets/branding/phantom-app-icon-1024.png");

#[derive(Clone, Copy, Debug)]
enum NavigationAction {
    New,
    History(usize),
    Reload,
}

struct PendingNavigation {
    receiver: Receiver<Result<TextResponse, NetworkError>>,
    action: NavigationAction,
}

struct ImageLoadRequest {
    resources: Vec<ImageResourceId>,
    url: HttpUrl,
}

struct LoadedImage {
    metadata: ImageMetadata,
    decoded: DecodedImage,
}

struct ImageLoadEvent {
    resources: Vec<ImageResourceId>,
    cache_key: String,
    result: Result<LoadedImage, String>,
}

struct CachedImage {
    metadata: ImageMetadata,
    texture: egui::TextureHandle,
    raster_bytes: u64,
    last_used: u64,
}

struct PendingImageBatch {
    receiver: Receiver<ImageLoadEvent>,
    remaining: usize,
    total: usize,
}

struct BrowserTab {
    engine: Engine,
    address: String,
    title: String,
    status: String,
    pending: Option<PendingNavigation>,
    pending_images: Option<PendingImageBatch>,
    image_textures: BTreeMap<ImageResourceId, egui::TextureHandle>,
    image_cache: BTreeMap<String, CachedImage>,
    cache_clock: u64,
    loaded_images: usize,
    failed_images: usize,
    raster_bytes: u64,
    page_loaded: bool,
    history: Vec<String>,
    history_index: Option<usize>,
}

impl BrowserTab {
    fn new() -> Self {
        Self {
            engine: Engine::new(),
            address: String::new(),
            title: "Nova aba".to_owned(),
            status: "Nova aba · JavaScript OFF · Telemetria OFF".to_owned(),
            pending: None,
            pending_images: None,
            image_textures: BTreeMap::new(),
            image_cache: BTreeMap::new(),
            cache_clock: 0,
            loaded_images: 0,
            failed_images: 0,
            raster_bytes: 0,
            page_loaded: false,
            history: Vec::new(),
            history_index: None,
        }
    }

    fn can_go_back(&self) -> bool {
        self.history_index.is_some_and(|index| index > 0)
    }

    fn can_go_forward(&self) -> bool {
        self.history_index
            .is_some_and(|index| index.saturating_add(1) < self.history.len())
    }

    fn is_loading(&self) -> bool {
        self.pending.is_some()
    }

    fn is_loading_images(&self) -> bool {
        self.pending_images.is_some()
    }
}

#[derive(Clone, Copy)]
enum NavigationUiCommand {
    Go,
    Back,
    Forward,
    Reload,
}

struct PhantomApp {
    network: NetworkClient,
    tabs: Vec<BrowserTab>,
    active_tab: usize,
    page_logo: Option<egui::TextureHandle>,
    app_icon: Option<egui::TextureHandle>,
    floating_bar_forced: bool,
    focus_address_next_frame: bool,
    window_maximized: bool,
}

impl PhantomApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        configure_fonts(&context.egui_ctx);
        context.egui_ctx.set_visuals(egui::Visuals::light());

        Self {
            network: NetworkClient::new(),
            tabs: vec![BrowserTab::new()],
            active_tab: 0,
            page_logo: load_texture(
                &context.egui_ctx,
                "phantom-page-logo",
                PAGE_LOGO_BYTES,
            ),
            app_icon: load_texture(
                &context.egui_ctx,
                "phantom-app-icon",
                APP_ICON_BYTES,
            ),
            floating_bar_forced: true,
            focus_address_next_frame: true,
            window_maximized: false,
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

    fn handle_shortcuts(&mut self, context: &egui::Context) {
        let focus_location = context.input(|input| {
            input.modifiers.command && input.key_pressed(egui::Key::L)
        });

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
            poll_tab_navigation(
                tab,
                &network,
                context.pixels_per_point(),
            );
            poll_tab_images(tab, context);
        }
    }

    fn execute_navigation_command(&mut self, command: NavigationUiCommand) {
        let network = self.network.clone();
        let tab = self.active_tab_mut();

        match command {
            NavigationUiCommand::Go => {
                let target = tab.address.trim().to_owned();

                if !target.is_empty() {
                    start_navigation(
                        &network,
                        tab,
                        target,
                        NavigationAction::New,
                    );
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
                    start_navigation(
                        &network,
                        tab,
                        target,
                        NavigationAction::Reload,
                    );
                }
            }
        }
    }

    fn top_chrome(&mut self, ui: &mut egui::Ui) {
        let context = ui.ctx().clone();
        let mut activate_tab = None;
        let mut close_tab = None;
        let mut create_tab = false;
        let mut minimize = false;
        let mut toggle_maximize = false;
        let mut close_window = false;
        let mut start_drag = false;

        egui::Panel::top("phantom-top-chrome")
            .exact_size(46.0)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(247, 249, 252))
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(icon) = &self.app_icon {
                        ui.image((icon.id(), egui::vec2(28.0, 28.0)));
                    }

                    ui.add_space(2.0);

                    for (index, tab) in self.tabs.iter().enumerate() {
                        let is_active = index == self.active_tab;
                        let title = tab_title(tab);
                        let fill = if is_active {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        let tab_response = ui.add_sized(
                            [136.0, 30.0],
                            egui::Button::new(
                                egui::RichText::new(title).size(13.0),
                            )
                            .fill(fill),
                        );

                        if tab_response.clicked() {
                            activate_tab = Some(index);
                        }

                        let close_response = icon_button(
                            ui,
                            Icon::X,
                            true,
                            [28.0, 30.0],
                            15.0,
                        )
                        .on_hover_text("Fechar aba");

                        if close_response.clicked() {
                            close_tab = Some(index);
                        }
                    }

                    if icon_button(
                        ui,
                        Icon::Plus,
                        true,
                        [32.0, 30.0],
                        17.0,
                    )
                    .on_hover_text("Nova aba")
                    .clicked()
                    {
                        create_tab = true;
                    }

                    let drag_width = (ui.available_width() - 114.0).max(16.0);
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

                    if icon_button(
                        ui,
                        Icon::Minus,
                        true,
                        [36.0, 30.0],
                        16.0,
                    )
                    .on_hover_text("Minimizar")
                    .clicked()
                    {
                        minimize = true;
                    }

                    if icon_button(
                        ui,
                        Icon::Square,
                        true,
                        [36.0, 30.0],
                        14.0,
                    )
                    .on_hover_text("Maximizar / restaurar")
                    .clicked()
                    {
                        toggle_maximize = true;
                    }

                    if icon_button(
                        ui,
                        Icon::X,
                        true,
                        [36.0, 30.0],
                        17.0,
                    )
                    .on_hover_text("Fechar Phantom")
                    .clicked()
                    {
                        close_window = true;
                    }
                });
            });

        if let Some(index) = activate_tab
            && index < self.tabs.len()
        {
            self.active_tab = index;
            self.floating_bar_forced = false;
            self.focus_address_next_frame = false;
        }

        if let Some(index) = close_tab {
            self.close_tab(index);
        }

        if create_tab {
            self.open_new_tab();
        }

        if start_drag {
            context.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        if minimize {
            context.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }

        if toggle_maximize {
            self.window_maximized = !self.window_maximized;
            context.send_viewport_cmd(egui::ViewportCommand::Maximized(
                self.window_maximized,
            ));
        }

        if close_window {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn page_area(&self, ui: &mut egui::Ui) {
        let tab = self.active_tab();

        if tab.is_loading() {
            self.loading_page(ui, tab);
        } else if tab.page_loaded {
            self.rendered_page(ui, tab);
        } else {
            self.empty_page(ui, tab);
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
            ui.label(
                egui::RichText::new("Phantom Independent Web Browser")
                    .size(17.0),
            );
            ui.add_space(14.0);
            ui.label(egui::RichText::new(&tab.status).small().weak());
        });
    }

    fn loading_page(&self, ui: &mut egui::Ui, tab: &BrowserTab) {
        ui.add_space(90.0);

        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.add_space(12.0);
            ui.heading("Carregando");
            ui.add_space(6.0);
            ui.label(tab.address.as_str());
        });
    }

    fn rendered_page(&self, ui: &mut egui::Ui, tab: &BrowserTab) {
        let paint = tab.engine.paint_list();
        let document_width = paint.viewport_width().max(1.0);
        let document_height = paint.content_height().max(1.0);

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let canvas_width = document_width.max(ui.available_width());
                let canvas_height = document_height + 72.0;

                let (canvas_rect, _response) = ui.allocate_exact_size(
                    egui::vec2(canvas_width, canvas_height),
                    egui::Sense::hover(),
                );

                let horizontal_offset =
                    ((canvas_width - document_width) * 0.5).max(0.0);

                let origin =
                    canvas_rect.min + egui::vec2(horizontal_offset, 18.0);

                paint_page(
                    ui,
                    origin,
                    paint,
                    &tab.image_textures,
                );
            });
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
        let can_reload = self.tabs[active_index].page_loaded
            || !self.tabs[active_index].address.trim().is_empty();
        let loading = self.tabs[active_index].is_loading();

        egui::Area::new(egui::Id::new("phantom-floating-navigation"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
            .order(egui::Order::Foreground)
            .show(context, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(250, 251, 253))
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_gray(210),
                    ))
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

                            let address_width =
                                (ui.available_width() - 58.0).max(180.0);

                            let address_response = ui.add_sized(
                                [address_width, 36.0],
                                egui::TextEdit::singleline(
                                    &mut self.tabs[active_index].address,
                                )
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
                                && ui.input(|input| {
                                    input.key_pressed(egui::Key::Enter)
                                });

                            let go_clicked = ui
                                .add_enabled_ui(!loading, |ui| {
                                    ui.add_sized(
                                        [48.0, 36.0],
                                        egui::Button::new("Ir"),
                                    )
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

                        ui.add_space(3.0);

                        ui.label(
                            egui::RichText::new(&self.tabs[active_index].status)
                                .small()
                                .weak(),
                        );
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

        if self.tabs.iter().any(|tab| {
            tab.is_loading() || tab.is_loading_images()
        }) {
            ui.ctx()
                .request_repaint_after(Duration::from_millis(50));
        }

        self.top_chrome(ui);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::WHITE).inner_margin(10))
            .show(ui, |ui| {
                self.page_area(ui);
            });

        self.floating_navigation(ui.ctx());
    }
}

fn start_navigation(
    network: &NetworkClient,
    tab: &mut BrowserTab,
    target: String,
    action: NavigationAction,
) {
    if tab.pending.is_some() {
        tab.status = "Já existe uma navegação em andamento.".to_owned();
        return;
    }

    tab.pending_images = None;
    tab.address = target.clone();
    tab.status = format!("Carregando {target} …");

    let client = network.clone();
    let (sender, receiver) = mpsc::channel();

    let thread_result = thread::Builder::new()
        .name("phantom-network".to_owned())
        .spawn(move || {
            let result = client.fetch_text(&target);
            let _ = sender.send(result);
        });

    match thread_result {
        Ok(_handle) => {
            tab.pending = Some(PendingNavigation { receiver, action });
        }

        Err(error) => {
            tab.status = format!("Não foi possível iniciar a navegação: {error}");
        }
    }
}

fn poll_tab_navigation(
    tab: &mut BrowserTab,
    network: &NetworkClient,
    device_pixel_ratio: f32,
) {
    let receive_result = tab
        .pending
        .as_ref()
        .map(|pending| (pending.action, pending.receiver.try_recv()));

    match receive_result {
        Some((action, Ok(Ok(response)))) => {
            tab.pending = None;

            let status_code = response.status();
            let body_bytes = response.body_bytes();
            let final_url = response.final_url().to_owned();

            match tab.engine.load_html(response.body()) {
                Ok(()) => {
                    commit_history(tab, action, &final_url);
                    tab.address = final_url.clone();
                    tab.title = title_from_url(&final_url);
                    tab.page_loaded = true;
                    tab.image_textures.clear();
                    tab.image_cache.clear();
                    tab.cache_clock = 0;
                    tab.loaded_images = 0;
                    tab.failed_images = 0;
                    tab.raster_bytes = 0;

                    tab.status = format!(
                        "HTTP {status_code} · {body_bytes} bytes · {} nós · {} caixas · {} comandos",
                        tab.engine.document().len(),
                        tab.engine.layout().len(),
                        tab.engine.paint_list().len()
                    );

                    start_image_loading(
                        network,
                        tab,
                        &final_url,
                        device_pixel_ratio,
                    );
                }

                Err(error) => {
                    tab.status = format!("Falha de renderização: {error}");
                }
            }
        }

        Some((_action, Ok(Err(error)))) => {
            tab.pending = None;
            tab.status = format!("Falha de navegação: {error}");
        }

        Some((_action, Err(TryRecvError::Disconnected))) => {
            tab.pending = None;
            tab.status = "O worker de rede foi encerrado inesperadamente.".to_owned();
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
    tab.pending_images = None;

    let Ok(base_url) = HttpUrl::parse(document_url) else {
        return;
    };

    let requests = collect_image_requests(
        tab,
        &base_url,
        device_pixel_ratio,
    );

    if requests.is_empty() {
        return;
    }

    let total = requests.len();
    let client = network.clone();
    let (sender, receiver) = mpsc::channel();

    let thread_result = thread::Builder::new()
        .name("phantom-images".to_owned())
        .spawn(move || {
            let decoder = RasterImageDecoder;
            let limits = ImageDecodeLimits::new(
                8_192,
                8_192,
                16_777_216,
                67_108_864,
            );

            for request in requests {
                let cache_key = request.url.as_str().to_owned();
                let result = fetch_and_decode_image(
                    &client,
                    &decoder,
                    limits,
                    &request.url,
                );

                if sender
                    .send(ImageLoadEvent {
                        resources: request.resources,
                        cache_key,
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
            tab.pending_images = Some(
                PendingImageBatch {
                    receiver,
                    remaining: total,
                    total,
                },
            );

            tab.status = format!(
                "Página pronta · carregando {total} imagens…"
            );
        }

        Err(error) => {
            tab.failed_images = total;
            tab.status = format!(
                "Página pronta · worker de imagens indisponível: {error}"
            );
        }
    }
}

fn collect_image_requests(
    tab: &mut BrowserTab,
    base_url: &HttpUrl,
    device_pixel_ratio: f32,
) -> Vec<ImageLoadRequest> {
    let discovered = tab
        .engine
        .image_requests_for_device(device_pixel_ratio);

    let mut grouped = BTreeMap::<String, (HttpUrl, Vec<ImageResourceId>)>::new();
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
            .and_modify(|(_, resources)| {
                resources.push(image_request.resource());
            })
            .or_insert_with(|| (url, vec![image_request.resource()]));
    }

    let mut requests = Vec::new();

    for (cache_key, (url, resources)) in grouped {
        if bind_cached_image(tab, &cache_key, &resources) {
            continue;
        }

        requests.push(ImageLoadRequest { resources, url });
    }

    requests
}

fn bind_cached_image(
    tab: &mut BrowserTab,
    cache_key: &str,
    resources: &[ImageResourceId],
) -> bool {
    let Some((metadata, texture)) = tab
        .image_cache
        .get(cache_key)
        .map(|cached| (cached.metadata, cached.texture.clone()))
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
            tab.image_textures.insert(*resource, texture.clone());
            installed = installed.saturating_add(1);
        } else {
            tab.failed_images = tab.failed_images.saturating_add(1);
        }
    }

    tab.loaded_images = tab.loaded_images.saturating_add(installed);
    true
}

fn fetch_and_decode_image(
    network: &NetworkClient,
    decoder: &dyn ImageDecoder,
    limits: ImageDecodeLimits,
    url: &HttpUrl,
) -> Result<LoadedImage, String> {
    let response = network
        .fetch_bytes(url)
        .map_err(|error| error.to_string())?;

    if !(200..=299).contains(&response.status()) {
        return Err(format!(
            "HTTP {} ao carregar imagem",
            response.status(),
        ));
    }

    let metadata = decoder
        .probe(response.body(), limits)
        .map_err(|error| error.to_string())?;

    let decoded = decoder
        .decode(response.body(), limits)
        .map_err(|error| error.to_string())?;

    Ok(LoadedImage {
        metadata,
        decoded,
    })
}

fn poll_tab_images(
    tab: &mut BrowserTab,
    context: &egui::Context,
) {
    const MAX_IMAGES_PER_FRAME: usize = 8;

    for _ in 0..MAX_IMAGES_PER_FRAME {
        let receive_result = tab
            .pending_images
            .as_ref()
            .map(|pending| pending.receiver.try_recv());

        match receive_result {
            Some(Ok(event)) => {
                if let Some(pending) =
                    tab.pending_images.as_mut()
                {
                    pending.remaining =
                        pending.remaining.saturating_sub(1);
                }

                install_loaded_image(
                    tab,
                    context,
                    event,
                );
            }

            Some(Err(TryRecvError::Disconnected)) => {
                let unresolved = tab
                    .pending_images
                    .as_ref()
                    .map_or(0, |pending| pending.remaining);

                tab.failed_images = tab
                    .failed_images
                    .saturating_add(unresolved);
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

fn install_loaded_image(
    tab: &mut BrowserTab,
    context: &egui::Context,
    event: ImageLoadEvent,
) {
    let resource_count = event.resources.len();
    let LoadedImage { metadata, decoded } = match event.result {
        Ok(loaded) => loaded,
        Err(_error) => {
            tab.failed_images = tab.failed_images.saturating_add(resource_count);
            update_image_status(tab);
            return;
        }
    };

    let decoded_bytes = u64::try_from(decoded.rgba8().len()).unwrap_or(u64::MAX);
    if decoded_bytes > MAX_TAB_RASTER_BYTES {
        tab.failed_images = tab.failed_images.saturating_add(resource_count);
        update_image_status(tab);
        return;
    }

    evict_image_cache_for(tab, decoded_bytes);

    let Some(resource_name) = event.resources.first().copied() else {
        update_image_status(tab);
        return;
    };

    let Some(texture) = decoded_image_texture(context, resource_name, &decoded) else {
        tab.failed_images = tab.failed_images.saturating_add(resource_count);
        update_image_status(tab);
        return;
    };

    tab.cache_clock = tab.cache_clock.saturating_add(1);
    tab.image_cache.insert(
        event.cache_key,
        CachedImage {
            metadata,
            texture: texture.clone(),
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
            tab.image_textures.insert(resource, texture.clone());
            installed = installed.saturating_add(1);
        } else {
            tab.failed_images = tab.failed_images.saturating_add(1);
        }
    }

    tab.loaded_images = tab.loaded_images.saturating_add(installed);
    update_image_status(tab);
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
        }
    }
}

fn decoded_image_texture(
    context: &egui::Context,
    resource: ImageResourceId,
    decoded: &DecodedImage,
) -> Option<egui::TextureHandle> {
    let width = usize::try_from(
        decoded.size().width(),
    )
    .ok()?;

    let height = usize::try_from(
        decoded.size().height(),
    )
    .ok()?;

    let color_image =
        egui::ColorImage::from_rgba_unmultiplied(
            [width, height],
            decoded.rgba8(),
        );

    Some(context.load_texture(
        format!(
            "phantom-web-image-{}",
            resource.as_u64(),
        ),
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}

fn update_image_status(tab: &mut BrowserTab) {
    if let Some(pending) =
        tab.pending_images.as_ref()
    {
        let completed = pending
            .total
            .saturating_sub(pending.remaining);

        tab.status = format!(
            "Página pronta · imagens {completed}/{} · {} exibidas · {} falhas",
            pending.total,
            tab.loaded_images,
            tab.failed_images,
        );
    } else if tab.loaded_images > 0
        || tab.failed_images > 0
    {
        tab.status = format!(
            "Página pronta · {} imagens exibidas · {} falhas",
            tab.loaded_images,
            tab.failed_images,
        );
    }
}

fn commit_history(tab: &mut BrowserTab, action: NavigationAction, final_url: &str) {
    match action {
        NavigationAction::New => {
            if let Some(index) = tab.history_index {
                tab.history.truncate(index.saturating_add(1));
            } else {
                tab.history.clear();
            }

            tab.history.push(final_url.to_owned());
            tab.history_index = tab.history.len().checked_sub(1);
        }

        NavigationAction::History(index) => {
            if let Some(entry) = tab.history.get_mut(index) {
                *entry = final_url.to_owned();
                tab.history_index = Some(index);
            }
        }

        NavigationAction::Reload => {
            if let Some(index) = tab.history_index
                && let Some(entry) = tab.history.get_mut(index)
            {
                *entry = final_url.to_owned();
            }
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
    image_textures: &BTreeMap<ImageResourceId, egui::TextureHandle>,
) {
    for command in paint.commands() {
        match command {
            PaintCommand::FillRect { rect, color } => {
                let target = egui_rect(origin, *rect);

                ui.painter().rect_filled(
                    target,
                    0.0,
                    egui_color(*color),
                );
            }

            PaintCommand::Image {
                rect,
                resource,
                alt,
                fit,
                position,
            } => {
                let target = egui_rect(origin, *rect);

                if let Some(texture) = image_textures.get(resource) {
                    paint_fitted_image(
                        ui,
                        texture,
                        target,
                        *fit,
                        *position,
                    );
                } else {
                    paint_image_placeholder(
                        ui,
                        target,
                        paint,
                        *alt,
                    );
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
                    egui_rect(origin, *rect),
                    egui::Label::new(rich_text).wrap(),
                );
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
    ui.painter().image(
        texture.id(),
        draw_rect,
        uv,
        egui::Color32::WHITE,
    );
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
    let scale = (target.width() / source.x)
        .min(target.height() / source.y);
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
    let scale = (target.width() / source.x)
        .max(target.height() / source.y);
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
    ui.painter().rect_filled(
        target,
        0.0,
        egui::Color32::from_gray(36),
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
                    .color(
                        egui::Color32::from_gray(190),
                    ),
            )
            .wrap(),
        );
    }
}

fn egui_rect(origin: egui::Pos2, rect: PaintRect) -> egui::Rect {
    egui::Rect::from_min_size(
        origin + egui::vec2(rect.x(), rect.y()),
        egui::vec2(
            rect.width().max(1.0),
            rect.height().max(1.0),
        ),
    )
}

fn egui_color(color: PaintColor) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(
        color.red(),
        color.green(),
        color.blue(),
        color.alpha(),
    )
}

fn configure_fonts(context: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        LUCIDE_FONT_NAME.to_owned(),
        Arc::new(egui::FontData::from_static(LUCIDE_FONT_BYTES)),
    );

    fonts.families.insert(
        lucide_font_family(),
        vec![LUCIDE_FONT_NAME.to_owned()],
    );

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
        ui.add_sized(size, egui::Button::new(lucide_text(icon, icon_size)))
    })
    .inner
}

fn decode_image(bytes: &[u8]) -> Option<image::RgbaImage> {
    image::load_from_memory(bytes)
        .ok()
        .map(|image| image.to_rgba8())
}

fn load_texture(
    context: &egui::Context,
    name: &str,
    bytes: &[u8],
) -> Option<egui::TextureHandle> {
    let image = decode_image(bytes)?;
    let width = usize::try_from(image.width()).ok()?;
    let height = usize::try_from(image.height()).ok()?;

    let color_image = egui::ColorImage::from_rgba_unmultiplied(
        [width, height],
        image.as_raw(),
    );

    Some(context.load_texture(
        name,
        color_image,
        egui::TextureOptions::LINEAR,
    ))
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
