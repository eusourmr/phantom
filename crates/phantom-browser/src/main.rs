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
    Engine, PaintColor, PaintCommand, PaintFontFamily, PaintFontStyle, PaintFontWeight, PaintList,
    PaintRect, PaintTextRange,
};
use phantom_image::{
    DecodedImage, ImageDecodeLimits, ImageDecoder, ImageMetadata, ImageResourceId,
    RasterImageDecoder,
};
use phantom_net::{HttpUrl, NetworkClient, NetworkError, TextResponse};

const APP_NAME: &str = "Phantom";
const APP_VERSION: &str = "0.0.1";
const LUCIDE_FONT_NAME: &str = "phantom-lucide";
const MAX_IMAGES_PER_DOCUMENT: usize = 64;
const MAX_TAB_RASTER_BYTES: u64 = 256 * 1024 * 1024;

const PAGE_LOGO_BYTES: &[u8] = include_bytes!("../assets/branding/phantom-logo.png");

const APP_ICON_BYTES: &[u8] = include_bytes!("../assets/branding/phantom-app-icon-1024.png");

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
    resource: ImageResourceId,
    url: HttpUrl,
}

struct LoadedImage {
    metadata: ImageMetadata,
    decoded: DecodedImage,
}

struct ImageLoadEvent {
    resource: ImageResourceId,
    result: Result<LoadedImage, String>,
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
            page_logo: load_texture(&context.egui_ctx, "phantom-page-logo", PAGE_LOGO_BYTES),
            app_icon: load_texture(&context.egui_ctx, "phantom-app-icon", APP_ICON_BYTES),
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
        let focus_location =
            context.input(|input| input.modifiers.command && input.key_pressed(egui::Key::L));

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
            poll_tab_navigation(tab, &network);
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
                            egui::Button::new(egui::RichText::new(title).size(13.0)).fill(fill),
                        );

                        if tab_response.clicked() {
                            activate_tab = Some(index);
                        }

                        let close_response = icon_button(ui, Icon::X, true, [28.0, 30.0], 15.0)
                            .on_hover_text("Fechar aba");

                        if close_response.clicked() {
                            close_tab = Some(index);
                        }
                    }

                    if icon_button(ui, Icon::Plus, true, [32.0, 30.0], 17.0)
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

                    if icon_button(ui, Icon::Minus, true, [36.0, 30.0], 16.0)
                        .on_hover_text("Minimizar")
                        .clicked()
                    {
                        minimize = true;
                    }

                    if icon_button(ui, Icon::Square, true, [36.0, 30.0], 14.0)
                        .on_hover_text("Maximizar / restaurar")
                        .clicked()
                    {
                        toggle_maximize = true;
                    }

                    if icon_button(ui, Icon::X, true, [36.0, 30.0], 17.0)
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
            context.send_viewport_cmd(egui::ViewportCommand::Maximized(self.window_maximized));
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
            ui.label(egui::RichText::new("Phantom Independent Web Browser").size(17.0));
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

                let horizontal_offset = ((canvas_width - document_width) * 0.5).max(0.0);

                let origin = canvas_rect.min + egui::vec2(horizontal_offset, 18.0);

                paint_page(ui, origin, paint, &tab.image_textures);
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
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(210)))
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

        if self
            .tabs
            .iter()
            .any(|tab| tab.is_loading() || tab.is_loading_images())
        {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }

        self.top_chrome(ui);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .inner_margin(10),
            )
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

fn poll_tab_navigation(tab: &mut BrowserTab, network: &NetworkClient) {
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
                    tab.loaded_images = 0;
                    tab.failed_images = 0;
                    tab.raster_bytes = 0;

                    tab.status = format!(
                        "HTTP {status_code} · {body_bytes} bytes · {} nós · {} caixas · {} comandos",
                        tab.engine.document().len(),
                        tab.engine.layout().len(),
                        tab.engine.paint_list().len()
                    );

                    start_image_loading(network, tab, &final_url);
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

fn start_image_loading(network: &NetworkClient, tab: &mut BrowserTab, document_url: &str) {
    tab.pending_images = None;

    let Ok(base_url) = HttpUrl::parse(document_url) else {
        return;
    };

    let requests = collect_image_requests(tab, &base_url);

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
            let limits = ImageDecodeLimits::new(8_192, 8_192, 16_777_216, 67_108_864);

            for request in requests {
                let result = fetch_and_decode_image(&client, &decoder, limits, &request.url);

                if sender
                    .send(ImageLoadEvent {
                        resource: request.resource,
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
            });

            tab.status = format!("Página pronta · carregando {total} imagens…");
        }

        Err(error) => {
            tab.failed_images = total;
            tab.status = format!("Página pronta · worker de imagens indisponível: {error}");
        }
    }
}

fn collect_image_requests(tab: &BrowserTab, base_url: &HttpUrl) -> Vec<ImageLoadRequest> {
    let mut requests = Vec::new();

    for image_request in tab.engine.image_requests() {
        let Ok(url) = base_url.resolve(image_request.source()) else {
            continue;
        };

        requests.push(ImageLoadRequest {
            resource: image_request.resource(),
            url,
        });

        if requests.len() >= MAX_IMAGES_PER_DOCUMENT {
            break;
        }
    }

    requests
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
        return Err(format!("HTTP {} ao carregar imagem", response.status(),));
    }

    let metadata = decoder
        .probe(response.body(), limits)
        .map_err(|error| error.to_string())?;

    let decoded = decoder
        .decode(response.body(), limits)
        .map_err(|error| error.to_string())?;

    Ok(LoadedImage { metadata, decoded })
}

fn poll_tab_images(tab: &mut BrowserTab, context: &egui::Context) {
    const MAX_IMAGES_PER_FRAME: usize = 8;

    for _ in 0..MAX_IMAGES_PER_FRAME {
        let receive_result = tab
            .pending_images
            .as_ref()
            .map(|pending| pending.receiver.try_recv());

        match receive_result {
            Some(Ok(event)) => {
                if let Some(pending) = tab.pending_images.as_mut() {
                    pending.remaining = pending.remaining.saturating_sub(1);
                }

                install_loaded_image(tab, context, event);
            }

            Some(Err(TryRecvError::Disconnected)) => {
                let unresolved = tab
                    .pending_images
                    .as_ref()
                    .map_or(0, |pending| pending.remaining);

                tab.failed_images = tab.failed_images.saturating_add(unresolved);
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

fn install_loaded_image(tab: &mut BrowserTab, context: &egui::Context, event: ImageLoadEvent) {
    let LoadedImage { metadata, decoded } = match event.result {
        Ok(loaded) => loaded,
        Err(_error) => {
            tab.failed_images = tab.failed_images.saturating_add(1);
            update_image_status(tab);
            return;
        }
    };

    let viewport_width = tab.engine.layout().viewport_width();

    if tab
        .engine
        .install_image_metadata(event.resource, metadata, viewport_width)
        .is_err()
    {
        tab.failed_images = tab.failed_images.saturating_add(1);
        update_image_status(tab);
        return;
    }

    let decoded_bytes = u64::try_from(decoded.rgba8().len()).unwrap_or(u64::MAX);

    let Some(next_raster_bytes) = tab
        .raster_bytes
        .checked_add(decoded_bytes)
        .filter(|bytes| *bytes <= MAX_TAB_RASTER_BYTES)
    else {
        tab.failed_images = tab.failed_images.saturating_add(1);
        update_image_status(tab);
        return;
    };

    let Some(texture) = decoded_image_texture(context, event.resource, &decoded) else {
        tab.failed_images = tab.failed_images.saturating_add(1);
        update_image_status(tab);
        return;
    };

    tab.image_textures.insert(event.resource, texture);
    tab.raster_bytes = next_raster_bytes;
    tab.loaded_images = tab.loaded_images.saturating_add(1);

    update_image_status(tab);
}

fn decoded_image_texture(
    context: &egui::Context,
    resource: ImageResourceId,
    decoded: &DecodedImage,
) -> Option<egui::TextureHandle> {
    let width = usize::try_from(decoded.size().width()).ok()?;

    let height = usize::try_from(decoded.size().height()).ok()?;

    let color_image = egui::ColorImage::from_rgba_unmultiplied([width, height], decoded.rgba8());

    Some(context.load_texture(
        format!("phantom-web-image-{}", resource.as_u64(),),
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}

fn update_image_status(tab: &mut BrowserTab) {
    if let Some(pending) = tab.pending_images.as_ref() {
        let completed = pending.total.saturating_sub(pending.remaining);

        tab.status = format!(
            "Página pronta · imagens {completed}/{} · {} exibidas · {} falhas",
            pending.total, tab.loaded_images, tab.failed_images,
        );
    } else if tab.loaded_images > 0 || tab.failed_images > 0 {
        tab.status = format!(
            "Página pronta · {} imagens exibidas · {} falhas",
            tab.loaded_images, tab.failed_images,
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

                ui.painter().rect_filled(target, 0.0, egui_color(*color));
            }

            PaintCommand::Image {
                rect,
                resource,
                alt,
            } => {
                let target = egui_rect(origin, *rect);

                if let Some(texture) = image_textures.get(resource) {
                    ui.painter().image(
                        texture.id(),
                        target,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
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
        ui.add_sized(size, egui::Button::new(lucide_text(icon, icon_size)))
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
