//! COVALOS Library Studio: discover, inspect and install components without leaving the
//! editor. The catalog lives in `crate::library`; this module is the native interface.
//!
//! Third-party React components cannot run inside the Rust renderer, so their cards show
//! verified metadata, a schematic of their category and the official source on demand.
//! COVALOS-native resources (backgrounds, springs, icon sets) render live previews and have
//! a working playground.
use super::{Editor, theme::*};
use crate::library::{self as lib, Category, Compatibility, Native, Terms};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Id, Layout, Margin, Rect, RichText, ScrollArea, Sense,
    Stroke, TextEdit, Ui, pos2, vec2,
};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum View {
    Discover,
    Category(Category),
    Installed,
    Favorites,
    Sources,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Overview,
    Code,
    Playground,
    Install,
}

enum Progress {
    Line(String),
    Finished(Result<(), String>),
}

enum Install {
    Planning(Receiver<Result<lib::Plan, String>>),
    Review {
        plan: lib::Plan,
        overwrite: bool,
    },
    Running {
        plan: lib::Plan,
        events: Receiver<Progress>,
        log: String,
        before: lib::Snapshot,
        backup: PathBuf,
    },
    Done {
        plan: lib::Plan,
        ok: bool,
        log: String,
        created: Vec<PathBuf>,
        modified: Vec<PathBuf>,
        backup: PathBuf,
    },
    Reverted(String),
    Failed(String),
}

struct Playground {
    density: f32,
    strength: f32,
    pixel: f32,
    stiffness: f32,
    damping: f32,
    icon_size: f32,
    accent: bool,
}

impl Default for Playground {
    fn default() -> Self {
        Self {
            density: 0.45,
            strength: 1.0,
            pixel: 4.0,
            stiffness: 380.0,
            damping: 0.92,
            icon_size: 20.0,
            accent: false,
        }
    }
}

pub(super) struct Studio {
    loaded: bool,
    dir: Option<PathBuf>,
    catalog: lib::Catalog,
    state: lib::State,
    pub(super) query: lib::Query,
    pub(super) view: View,
    results: Vec<usize>,
    key: Option<(String, u64)>,
    pub(super) selected: Option<String>,
    tab: Tab,
    project: Option<lib::Project>,
    project_root: Option<PathBuf>,
    sync: Option<Receiver<(String, Result<lib::SyncInfo, String>)>>,
    sync_left: usize,
    sync_errors: Vec<String>,
    item: Option<(String, Result<lib::RegistryItem, String>)>,
    item_job: Option<(String, Receiver<Result<lib::RegistryItem, String>>)>,
    install: Option<(String, Install)>,
    play: Playground,
    pub(super) focus_search: bool,
    revision: u64,
}

impl Default for Studio {
    fn default() -> Self {
        Self {
            loaded: false,
            dir: None,
            catalog: lib::Catalog::default(),
            state: lib::State::default(),
            query: lib::Query::default(),
            view: View::Discover,
            results: Vec::new(),
            key: None,
            selected: None,
            tab: Tab::Overview,
            project: None,
            project_root: None,
            sync: None,
            sync_left: 0,
            sync_errors: Vec::new(),
            item: None,
            item_job: None,
            install: None,
            play: Playground::default(),
            focus_search: true,
            revision: 0,
        }
    }
}

impl Studio {
    /// Catalog and state load on first open, never at editor startup.
    fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }
        self.dir = lib::home();
        self.catalog = lib::Catalog::load(self.dir.as_deref());
        self.state = lib::State::load(self.dir.as_deref());
        self.loaded = true;
        self.revision += 1;
    }

    #[cfg(test)]
    pub(super) fn load_from(&mut self, dir: PathBuf) {
        self.dir = Some(dir);
        self.catalog = lib::Catalog::load(self.dir.as_deref());
        self.state = lib::State::load(self.dir.as_deref());
        self.loaded = true;
        self.revision += 1;
    }

    fn save_state(&mut self) {
        self.revision += 1;
        if let Some(dir) = &self.dir {
            let _ = self.state.save(dir);
        }
    }

    pub(super) fn select(&mut self, id: &str) {
        if self.selected.as_deref() != Some(id) {
            self.selected = Some(id.to_owned());
            self.tab = Tab::Overview;
            self.state.visit(id);
            self.save_state();
        }
    }

    pub(super) fn start_sync(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.dir.clone() else { return };
        let (send, events) = mpsc::channel();
        let sources: Vec<&'static lib::Source> =
            lib::SOURCES.iter().filter(|s| s.index.is_some()).collect();
        self.sync_left = sources.len();
        self.sync_errors.clear();
        let ctx = ctx.clone();
        thread::spawn(move || {
            for source in sources {
                let result = lib::sync_source(&dir, source);
                if send.send((source.id.to_owned(), result)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
        self.sync = Some(events);
    }

    #[cfg(test)]
    pub(super) fn syncing(&self) -> bool {
        self.sync.is_some()
    }

    fn poll(&mut self) {
        if let Some(events) = &self.sync {
            let mut finished = false;
            loop {
                match events.try_recv() {
                    Ok((id, result)) => {
                        self.sync_left = self.sync_left.saturating_sub(1);
                        if let Err(error) = result {
                            self.sync_errors.push(format!("{id}: {error}"));
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
            if finished || self.sync_left == 0 {
                self.sync = None;
                self.catalog = lib::Catalog::load(self.dir.as_deref());
                self.revision += 1;
            }
        }
        if let Some((id, job)) = &self.item_job
            && let Ok(result) = job.try_recv()
        {
            self.item = Some((id.clone(), result));
            self.item_job = None;
        }
    }

    fn results(&mut self) -> &[usize] {
        let key = (
            format!(
                "{:?}|{:?}|{:?}",
                self.query,
                self.view,
                self.project.as_ref().map(|p| &p.root)
            ),
            self.revision,
        );
        if self.key.as_ref() != Some(&key) {
            let mut query = self.query.clone();
            match self.view {
                View::Category(c) => query.category = Some(c),
                View::Installed => query.installed = true,
                View::Favorites => query.favorites = true,
                View::Discover | View::Sources => {}
            }
            self.results = self
                .catalog
                .search(&query, self.project.as_ref(), &self.state);
            self.key = Some(key);
        }
        &self.results
    }
}

/// Category icon used on cards, nav rows and schematic previews.
fn category_icon(category: Category) -> Icon {
    match category {
        Category::Components => Icon::Blocks,
        Category::Animations => Icon::Wand,
        Category::Backgrounds => Icon::Image,
        Category::Icons => Icon::Star,
        Category::Typography => Icon::Type,
        Category::Templates => Icon::Template,
        Category::Ai => Icon::Sparkles,
        Category::DataViz => Icon::Chart,
    }
}

fn compat_color(c: &Compatibility) -> Color32 {
    match c {
        Compatibility::Bundled | Compatibility::Ready => colors().success,
        Compatibility::NeedsSetup(_) => colors().warning,
        Compatibility::Incompatible(_) => colors().error,
        Compatibility::External(_) => colors().muted,
    }
}

fn compat_label(c: &Compatibility) -> &'static str {
    match c {
        Compatibility::Bundled => "Included in COVALOS",
        Compatibility::Ready => "Ready to install",
        Compatibility::NeedsSetup(_) => "Needs setup",
        Compatibility::Incompatible(_) => "Not compatible",
        Compatibility::External(_) => "External",
    }
}

fn chip(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui
        .fonts_mut(|f| f.layout_no_wrap(text.to_owned(), egui::FontId::proportional(11.0), color));
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(14.0, 6.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 8, color.gamma_multiply(0.14));
    ui.painter()
        .galley(rect.min + vec2(7.0, 3.0), galley, color);
}

fn nav_row(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    count: Option<usize>,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    let hover = super::motion::spring(
        ui.ctx(),
        response.id.with("hover"),
        if response.hovered() { 1.0 } else { 0.0 },
        super::motion::Spring::SNAPPY,
    );
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 7, colors().accent_bg);
    } else if hover > 0.01 {
        painter.rect_filled(rect, 7, colors().hover.gamma_multiply(hover));
    }
    draw_icon_sized(
        &painter,
        rect.left_center() + vec2(16.0, 0.0),
        icon,
        if selected {
            colors().accent
        } else {
            colors().muted
        },
        15.0,
    );
    painter.text(
        rect.left_center() + vec2(32.0, 0.0),
        egui::Align2::LEFT_CENTER,
        tr(label),
        egui::FontId::proportional(BODY),
        if selected {
            colors().text
        } else {
            colors().secondary
        },
    );
    if let Some(count) = count {
        painter.text(
            rect.right_center() - vec2(10.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            count.to_string(),
            egui::FontId::proportional(11.0),
            colors().muted,
        );
    }
    #[cfg(test)]
    super::motion::record(ui, format!("Library {label}"), rect);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::SelectableLabel, true, label));
    response
}

fn toggle_chip(ui: &mut Ui, value: &mut bool, label: &str) -> bool {
    let selected = *value;
    let text = tr(label);
    let galley = ui.fonts_mut(|f| {
        f.layout_no_wrap(
            text.to_owned(),
            egui::FontId::proportional(12.0),
            if selected {
                colors().text
            } else {
                colors().secondary
            },
        )
    });
    let (rect, response) = ui.allocate_exact_size(galley.size() + vec2(20.0, 10.0), Sense::click());
    ui.painter().rect(
        rect,
        13,
        if selected {
            colors().accent_bg
        } else {
            Color32::TRANSPARENT
        },
        Stroke::new(
            1.0_f32,
            if selected {
                colors().accent
            } else {
                colors().border
            },
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.min + vec2(10.0, 5.0), galley, colors().text);
    #[cfg(test)]
    super::motion::record(ui, format!("Filter {label}"), rect);
    if response.clicked() {
        *value = !*value;
        return true;
    }
    false
}

impl Editor {
    pub(super) fn open_library(&mut self) {
        self.studio_open = true;
        self.hub_open = false;
        self.studio.focus_search = true;
    }

    pub(super) fn library_studio(&mut self, ui: &mut Ui) {
        self.studio.ensure_loaded();
        self.studio.poll();
        if self.studio.project_root != self.root {
            self.studio.project_root = self.root.clone();
            self.studio.project = self.root.as_deref().map(lib::Project::detect);
            self.studio.revision += 1;
        }
        self.poll_install();
        let full = ui.available_rect_before_wrap();
        let header = Rect::from_min_size(full.min, vec2(full.width(), 56.0));
        let body = full.with_min_y(header.bottom());
        let nav_width = 212.0_f32.min(full.width() * 0.22);
        let detail_width = if self.studio.selected.is_some() {
            (full.width() * 0.34).clamp(320.0, 460.0)
        } else {
            0.0
        };
        let nav = Rect::from_min_size(body.min, vec2(nav_width, body.height()));
        let detail = Rect::from_min_max(pos2(body.right() - detail_width, body.top()), body.max);
        let main = Rect::from_min_max(
            pos2(nav.right(), body.top()),
            pos2(detail.left(), body.bottom()),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(header), |ui| {
            self.studio_header(ui)
        });
        ui.painter().hline(
            full.x_range(),
            header.bottom() - 0.5,
            Stroke::new(1.0_f32, colors().border),
        );
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(nav.shrink2(vec2(10.0, 10.0))),
            |ui| self.studio_nav(ui),
        );
        ui.painter().vline(
            nav.right() - 0.5,
            body.y_range(),
            Stroke::new(1.0_f32, colors().border),
        );
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(main.shrink2(vec2(18.0, 14.0))),
            |ui| self.studio_main(ui),
        );
        if detail_width > 0.0 {
            ui.painter().rect_filled(detail, 0, colors().surface);
            ui.painter().vline(
                detail.left() + 0.5,
                body.y_range(),
                Stroke::new(1.0_f32, colors().border),
            );
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(detail.shrink2(vec2(18.0, 14.0))),
                |ui| {
                    super::motion::enter(ui, Id::new("library-detail-entry"), super::motion::PANEL);
                    self.studio_detail(ui)
                },
            );
        }
        ui.allocate_rect(full, Sense::hover());
    }

    fn studio_header(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            let (_, mark) = ui.allocate_space(vec2(22.0, 22.0));
            draw_icon_sized(
                ui.painter(),
                mark.center(),
                Icon::Library,
                colors().accent,
                18.0,
            );
            ui.label(RichText::new(tr("Library Studio")).size(LEAD).strong());
            ui.add_space(10.0);
            match &self.studio.project {
                Some(project) => {
                    let frameworks = project.frameworks();
                    if frameworks.is_empty() {
                        chip(ui, tr("No framework detected"), colors().muted);
                    }
                    for name in frameworks.iter().take(5) {
                        chip(ui, name, colors().secondary);
                    }
                    if let Some(pm) = project.package_manager {
                        chip(ui, pm.label(), colors().muted);
                    }
                }
                None => chip(ui, tr("No workspace open"), colors().muted),
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(10.0);
                if icon_button(ui, Icon::Close, "Close Library Studio", false).clicked() {
                    self.studio_open = false;
                }
                if self.studio.sync.is_some() {
                    busy_indicator(ui);
                    ui.label(muted(trf(
                        "Syncing {n} sources…",
                        &[("n", self.studio.sync_left.to_string())],
                    )));
                } else if icon_button(ui, Icon::Refresh, "Sync catalog", false).clicked() {
                    self.studio.start_sync(ui.ctx());
                }
                let total = self.studio.catalog.entries.len().to_string();
                let synced = self
                    .studio
                    .catalog
                    .sync
                    .values()
                    .map(|s| s.synced_at)
                    .max()
                    .filter(|t| *t > 0);
                ui.label(muted(match synced {
                    Some(t) => trf(
                        "{n} entries · synced {age}",
                        &[("n", total.clone()), ("age", age(t))],
                    ),
                    None => trf("{n} entries · offline catalog", &[("n", total.clone())]),
                }));
            });
        });
    }

    fn studio_nav(&mut self, ui: &mut Ui) {
        ScrollArea::vertical()
            .id_salt("library-nav")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let mut view = self.studio.view;
                if nav_row(ui, Icon::Sparkles, "Discover", None, view == View::Discover).clicked() {
                    view = View::Discover;
                }
                ui.add_space(10.0);
                ui.label(
                    RichText::new(tr("CATEGORIES"))
                        .size(10.5)
                        .color(colors().muted),
                );
                ui.add_space(4.0);
                for category in Category::ALL {
                    let count = self
                        .studio
                        .catalog
                        .entries
                        .iter()
                        .filter(|e| e.categories.contains(&category))
                        .count();
                    if nav_row(
                        ui,
                        category_icon(category),
                        category.name(),
                        Some(count),
                        view == View::Category(category),
                    )
                    .clicked()
                    {
                        view = View::Category(category);
                    }
                }
                ui.add_space(10.0);
                ui.label(
                    RichText::new(tr("LIBRARY"))
                        .size(10.5)
                        .color(colors().muted),
                );
                ui.add_space(4.0);
                let installed = self
                    .studio
                    .state
                    .installed
                    .iter()
                    .filter(|i| !i.reverted)
                    .count();
                if nav_row(
                    ui,
                    Icon::Package,
                    "Installed",
                    Some(installed),
                    view == View::Installed,
                )
                .clicked()
                {
                    view = View::Installed;
                }
                let favorites = self.studio.state.favorites.len();
                if nav_row(
                    ui,
                    Icon::Heart,
                    "Favorites",
                    Some(favorites),
                    view == View::Favorites,
                )
                .clicked()
                {
                    view = View::Favorites;
                }
                if nav_row(
                    ui,
                    Icon::Globe,
                    "Sources",
                    Some(lib::SOURCES.len() - 1),
                    view == View::Sources,
                )
                .clicked()
                {
                    view = View::Sources;
                }
                self.studio.view = view;
                ui.add_space(14.0);
                ui.label(
                    RichText::new(tr("FILTERS"))
                        .size(10.5)
                        .color(colors().muted),
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    let q = &mut self.studio.query;
                    toggle_chip(ui, &mut q.free_only, "Free");
                    toggle_chip(ui, &mut q.open_license, "Open license");
                    toggle_chip(ui, &mut q.compatible, "Compatible");
                    toggle_chip(ui, &mut q.offline, "Offline");
                });
            });
    }

    fn studio_main(&mut self, ui: &mut Ui) {
        // Search field with an icon, focus ring and a live result count.
        let (search_rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        let focus = super::motion::spring(
            ui.ctx(),
            Id::new("library-search-focus"),
            if ui.memory(|m| m.has_focus(Id::new("library-search"))) {
                1.0
            } else {
                0.0
            },
            super::motion::Spring::SNAPPY,
        );
        ui.painter().rect(
            search_rect,
            10,
            colors().inset,
            Stroke::new(
                1.0_f32,
                super::motion::blend(colors().border, colors().accent, focus),
            ),
            egui::StrokeKind::Inside,
        );
        draw_icon_sized(
            ui.painter(),
            search_rect.left_center() + vec2(18.0, 0.0),
            Icon::Search,
            colors().muted,
            16.0,
        );
        let response = ui.put(
            Rect::from_min_max(
                search_rect.min + vec2(36.0, 9.0),
                search_rect.max - vec2(120.0, 9.0),
            ),
            TextEdit::singleline(&mut self.studio.query.text)
                .id(Id::new("library-search"))
                .hint_text(tr("Search components, animations, backgrounds, icons…"))
                .frame(false)
                .font(egui::FontId::proportional(15.0)),
        );
        #[cfg(test)]
        super::motion::record(ui, "Library search", search_rect);
        if std::mem::take(&mut self.studio.focus_search) {
            response.request_focus();
        }
        if response.changed() && self.studio.view == View::Sources {
            self.studio.view = View::Discover;
        }
        let count = if self.studio.view == View::Sources {
            lib::SOURCES.len() - 1
        } else {
            self.studio.results().len()
        };
        ui.painter().text(
            search_rect.right_center() - vec2(14.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            trf("{n} results", &[("n", count.to_string())]),
            egui::FontId::proportional(12.0),
            colors().muted,
        );
        ui.add_space(12.0);
        let discover = self.studio.view == View::Discover
            && self.studio.query.text.trim().is_empty()
            && !self.filters_active();
        match self.studio.view {
            View::Sources => self.studio_sources(ui),
            _ if discover => self.studio_discover(ui),
            _ => {
                let results = self.studio.results().to_vec();
                if results.is_empty() {
                    empty_state(
                        ui,
                        if self.studio.catalog.sync.is_empty() {
                            "Only COVALOS resources are available offline. Sync the catalog to add the official registries."
                        } else {
                            "No results. Try a broader term or turn off a filter."
                        },
                    );
                    if self.studio.catalog.sync.is_empty()
                        && self.studio.sync.is_none()
                        && ui.add(primary(tr("Sync catalog"))).clicked()
                    {
                        self.studio.start_sync(ui.ctx());
                    }
                    return;
                }
                ScrollArea::vertical()
                    .id_salt(("library-grid", format!("{:?}", self.studio.view)))
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.card_grid(ui, &results, 240));
            }
        }
    }

    fn filters_active(&self) -> bool {
        let q = &self.studio.query;
        q.free_only || q.open_license || q.compatible || q.offline
    }

    fn studio_discover(&mut self, ui: &mut Ui) {
        ScrollArea::vertical()
            .id_salt("library-discover")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                section_title(
                    ui,
                    "Native to COVALOS",
                    "Rendered by the editor itself, available offline.",
                );
                let native: Vec<usize> = (0..self.studio.catalog.entries.len())
                    .filter(|&i| self.studio.catalog.entries[i].native.is_some())
                    .collect();
                self.card_grid(ui, &native, 8);
                if let Some(project) = self.studio.project.clone()
                    && project.package_json
                {
                    ui.add_space(14.0);
                    section_title(
                        ui,
                        "For this workspace",
                        &format!(
                            "{} · {}",
                            project.frameworks().join(" · "),
                            tr("compatible results first")
                        ),
                    );
                    let query = lib::Query {
                        compatible: true,
                        ..Default::default()
                    };
                    let picks: Vec<usize> = self
                        .studio
                        .catalog
                        .search(&query, Some(&project), &self.studio.state)
                        .into_iter()
                        .filter(|&i| self.studio.catalog.entries[i].native.is_none())
                        .take(8)
                        .collect();
                    if picks.is_empty() {
                        ui.label(muted(
                            "Sync the catalog to see components for this workspace.",
                        ));
                    }
                    self.card_grid(ui, &picks, 8);
                }
                let recent: Vec<usize> = self
                    .studio
                    .state
                    .recent
                    .iter()
                    .filter_map(|id| self.studio.catalog.entries.iter().position(|e| &e.id == id))
                    .take(8)
                    .collect();
                if !recent.is_empty() {
                    ui.add_space(14.0);
                    section_title(ui, "Recently viewed", "");
                    self.card_grid(ui, &recent, 8);
                }
                ui.add_space(14.0);
                section_title(
                    ui,
                    "Sources",
                    "Official registries, licenses and what COVALOS may store.",
                );
                self.source_cards(ui);
            });
    }

    fn card_grid(&mut self, ui: &mut Ui, indices: &[usize], limit: usize) {
        let width = ui.available_width();
        let columns = ((width + 12.0) / 232.0).floor().max(1.0) as usize;
        let card_width = (width - 12.0 * (columns as f32 - 1.0)) / columns as f32;
        let mut clicked = None;
        let shown: Vec<usize> = indices.iter().take(limit).copied().collect();
        for row in shown.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for &index in row {
                    if self.card(ui, index, card_width).clicked() {
                        clicked = Some(index);
                    }
                }
            });
            ui.add_space(12.0);
        }
        if indices.len() > limit {
            ui.label(muted(trf(
                "{n} more — refine the search to see them",
                &[("n", (indices.len() - limit).to_string())],
            )));
        }
        if let Some(index) = clicked {
            let id = self.studio.catalog.entries[index].id.clone();
            self.studio.select(&id);
        }
    }

    fn card(&mut self, ui: &mut Ui, index: usize, width: f32) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(vec2(width, 176.0), Sense::click());
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let entry = self.studio.catalog.entries[index].clone();
        let selected = self.studio.selected.as_deref() == Some(entry.id.as_str());
        let hover = super::motion::spring(
            ui.ctx(),
            response.id.with("lift"),
            if response.hovered() { 1.0 } else { 0.0 },
            super::motion::Spring::SNAPPY,
        );
        let rect = rect.translate(vec2(0.0, -2.0 * hover));
        ui.painter().rect(
            rect,
            12,
            super::motion::blend(colors().surface, colors().elevated, hover),
            Stroke::new(
                1.0_f32,
                if selected {
                    colors().accent
                } else {
                    super::motion::blend(colors().border, colors().control_border, hover)
                },
            ),
            egui::StrokeKind::Inside,
        );
        let preview =
            Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(rect.width() - 12.0, 96.0));
        self.preview(ui, &entry, preview, false);
        let title_pos = pos2(rect.left() + 14.0, preview.bottom() + 12.0);
        let title = ui.fonts_mut(|f| {
            f.layout_job(egui::text::LayoutJob {
                wrap: egui::text::TextWrapping {
                    max_width: rect.width() - 52.0,
                    max_rows: 1,
                    break_anywhere: true,
                    overflow_character: Some('…'),
                },
                ..egui::text::LayoutJob::simple_singleline(
                    entry.title.clone(),
                    egui::FontId::proportional(BODY),
                    colors().text,
                )
            })
        });
        let painter = ui.painter();
        painter.galley(title_pos, title, colors().text);
        let source = entry.source();
        painter.text(
            pos2(rect.left() + 14.0, title_pos.y + 22.0),
            egui::Align2::LEFT_TOP,
            format!("{} · {}", source.name, source.license),
            egui::FontId::proportional(11.5),
            colors().muted,
        );
        let compat = self
            .studio
            .project
            .as_ref()
            .map(|p| lib::compatibility(&entry, p));
        if let Some(compat) = &compat {
            painter.circle_filled(
                pos2(rect.right() - 16.0, title_pos.y + 8.0),
                3.5,
                compat_color(compat),
            );
        }
        if self.studio.state.favorites.contains(&entry.id) {
            draw_icon_sized(
                painter,
                pos2(rect.right() - 16.0, title_pos.y + 29.0),
                Icon::Heart,
                colors().accent,
                12.0,
            );
        }
        let mut tip = format!("{}\n{}", entry.title, entry.description);
        if let Some(c) = &compat {
            tip.push('\n');
            tip.push_str(tr(compat_label(c)));
        }
        #[cfg(test)]
        super::motion::record(ui, format!("Card {}", entry.id), rect);
        response.on_hover_text(tip)
    }

    /// Live native preview, or an honest schematic for web components.
    fn preview(&mut self, ui: &mut Ui, entry: &lib::Entry, rect: Rect, large: bool) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 9, colors().inset);
        let time = ui.input(|i| i.time) as f32;
        match entry.native {
            Some(Native::DitherField | Native::DitherBloom) if super::dither::available() => {
                let p = &self.studio.play;
                let frame = super::dither::Frame {
                    variant: if entry.native == Some(Native::DitherBloom) {
                        super::dither::Variant::Bloom
                    } else {
                        super::dither::Variant::Field
                    },
                    time: time * 1.4,
                    ink: gamma(colors().text),
                    accent: gamma(colors().accent),
                    strength: if large { p.strength } else { 1.1 },
                    density: if large { p.density } else { 0.55 },
                    pixel: if large { p.pixel } else { 3.0 },
                    light: !active().is_dark(),
                    trail: [[0.0, 0.0, 1.0]; super::dither::TRAIL],
                    ripples: [[0.0, 0.0, -1.0]; super::dither::RIPPLES],
                };
                super::dither::paint(&painter, rect.shrink(1.0), frame);
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(42));
            }
            Some(native @ (Native::SpringSnappy | Native::SpringSmooth | Native::SpringGentle)) => {
                let preset = if large {
                    super::motion::Spring {
                        stiffness: self.studio.play.stiffness,
                        damping: self.studio.play.damping,
                    }
                } else {
                    match native {
                        Native::SpringSnappy => super::motion::Spring::SNAPPY,
                        Native::SpringSmooth => super::motion::Spring::SMOOTH,
                        _ => super::motion::Spring::GENTLE,
                    }
                };
                let left = rect.left() + 28.0;
                let right = rect.right() - 28.0;
                let flip = (time / 1.3).floor() as i64 % 2 == 0;
                let x = super::motion::spring(
                    ui.ctx(),
                    Id::new(("library-spring", &entry.id, large)),
                    if flip { right } else { left },
                    preset,
                );
                let y = rect.center().y;
                painter.hline(left..=right, y, Stroke::new(2.0_f32, colors().border));
                painter.circle_filled(pos2(left, y), 3.0, colors().control_border);
                painter.circle_filled(pos2(right, y), 3.0, colors().control_border);
                painter.circle_filled(pos2(x, y), 10.0, colors().accent);
                painter.circle_stroke(
                    pos2(x, y),
                    15.0,
                    Stroke::new(1.0_f32, colors().accent.gamma_multiply(0.35)),
                );
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(16));
            }
            Some(Native::LucideIcons) => {
                let icons = [
                    Icon::Search,
                    Icon::Git,
                    Icon::Settings,
                    Icon::Terminal,
                    Icon::Bell,
                    Icon::Save,
                    Icon::Refresh,
                    Icon::Play,
                    Icon::Folder,
                    Icon::Sparkles,
                    Icon::Package,
                    Icon::Heart,
                ];
                let size = if large {
                    self.studio.play.icon_size
                } else {
                    16.0
                };
                let color = if large && self.studio.play.accent {
                    colors().accent
                } else {
                    colors().secondary
                };
                grid(rect, icons.len(), |i, center| {
                    draw_icon_sized(&painter, center, icons[i], color, size)
                });
            }
            Some(Native::MaterialIcons) => {
                let files = [
                    "main.rs",
                    "app.tsx",
                    "index.ts",
                    "style.css",
                    "README.md",
                    "Cargo.toml",
                    "package.json",
                    "Dockerfile",
                    "main.py",
                    "data.json",
                    "logo.svg",
                    ".gitignore",
                ];
                let size = if large {
                    self.studio.play.icon_size
                } else {
                    16.0
                };
                grid(rect, files.len(), |i, center| {
                    super::assets::paint(
                        ui,
                        Rect::from_center_size(center, vec2(size, size)),
                        super::assets::file_name(std::path::Path::new(files[i])),
                    );
                });
            }
            _ => {
                // Schematic: category glyph, source and stack. No fake screenshot.
                let category = entry
                    .categories
                    .first()
                    .copied()
                    .unwrap_or(Category::Components);
                painter.rect_filled(rect, 9, colors().accent.gamma_multiply(0.035));
                for i in 0..6 {
                    let y = rect.top() + 14.0 + i as f32 * (rect.height() - 28.0) / 5.0;
                    painter.hline(
                        (rect.left() + 12.0)..=(rect.right() - 12.0),
                        y,
                        Stroke::new(1.0_f32, colors().border.gamma_multiply(0.45)),
                    );
                }
                draw_icon_sized(
                    &painter,
                    rect.center() - vec2(0.0, 8.0),
                    category_icon(category),
                    colors().accent.gamma_multiply(0.85),
                    if large { 34.0 } else { 26.0 },
                );
                painter.text(
                    rect.center() + vec2(0.0, if large { 26.0 } else { 20.0 }),
                    egui::Align2::CENTER_CENTER,
                    format!("{} · {}", entry.framework.label(), entry.language),
                    egui::FontId::proportional(10.5),
                    colors().muted,
                );
                painter.text(
                    rect.left_top() + vec2(10.0, 8.0),
                    egui::Align2::LEFT_TOP,
                    entry.source().name,
                    egui::FontId::proportional(10.5),
                    colors().muted,
                );
            }
        }
    }

    fn source_cards(&mut self, ui: &mut Ui) {
        let width = ui.available_width();
        let columns = ((width + 12.0) / 300.0).floor().max(1.0) as usize;
        let card_width = (width - 12.0 * (columns as f32 - 1.0)) / columns as f32;
        let mut open = None;
        let sources: Vec<&lib::Source> =
            lib::SOURCES.iter().filter(|s| s.id != "covalos").collect();
        for row in sources.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for source in row {
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(card_width, 92.0), Sense::click());
                    ui.painter().rect(
                        rect,
                        12,
                        if response.hovered() {
                            colors().elevated
                        } else {
                            colors().surface
                        },
                        Stroke::new(1.0_f32, colors().border),
                        egui::StrokeKind::Inside,
                    );
                    let count = self.studio.catalog.count(source.id);
                    let painter = ui.painter_at(rect);
                    painter.text(
                        rect.left_top() + vec2(14.0, 12.0),
                        egui::Align2::LEFT_TOP,
                        source.name,
                        egui::FontId::proportional(BODY),
                        colors().text,
                    );
                    painter.text(
                        rect.left_top() + vec2(14.0, 32.0),
                        egui::Align2::LEFT_TOP,
                        format!("{} · {}", source.license, tr(source.terms.label())),
                        egui::FontId::proportional(11.5),
                        terms_color(source.terms),
                    );
                    let synced = self.studio.catalog.sync.get(source.id);
                    let status = match (source.index, synced) {
                        (None, _) => tr("Link only").to_owned(),
                        (Some(_), Some(info)) if info.error.is_some() && count == 0 => {
                            tr("Unreachable on last sync").to_owned()
                        }
                        (Some(_), Some(_)) => {
                            trf("{n} entries cached", &[("n", count.to_string())])
                        }
                        (Some(_), None) => tr("Not synced yet").to_owned(),
                    };
                    painter.text(
                        rect.left_bottom() + vec2(14.0, -14.0),
                        egui::Align2::LEFT_BOTTOM,
                        status,
                        egui::FontId::proportional(11.5),
                        colors().muted,
                    );
                    painter.text(
                        rect.right_bottom() - vec2(14.0, 14.0),
                        egui::Align2::RIGHT_BOTTOM,
                        match source.pricing {
                            lib::Pricing::Free => tr("Free"),
                            lib::Pricing::Freemium => tr("Free + Pro"),
                            lib::Pricing::Paid => tr("Paid"),
                        },
                        egui::FontId::proportional(11.5),
                        colors().secondary,
                    );
                    if response.clicked() && !source.homepage.is_empty() {
                        open = Some(source.homepage);
                    }
                    response.on_hover_text(source.notes);
                }
            });
            ui.add_space(12.0);
        }
        if let Some(url) = open {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    }

    fn studio_sources(&mut self, ui: &mut Ui) {
        ScrollArea::vertical()
            .id_salt("library-sources")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                section_title(
                    ui,
                    "Sources",
                    "Official registries reviewed for Library Studio. Only metadata is stored for sources that forbid redistribution.",
                );
                self.source_cards(ui);
                for error in &self.studio.sync_errors {
                    ui.label(RichText::new(error).small().color(colors().warning));
                }
            });
    }

    fn studio_detail(&mut self, ui: &mut Ui) {
        let Some(id) = self.studio.selected.clone() else {
            return;
        };
        let Some(entry) = self.studio.catalog.get(&id).cloned() else {
            self.studio.selected = None;
            return;
        };
        let source = entry.source();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(ui.available_width() - 80.0);
                ui.add(egui::Label::new(RichText::new(&entry.title).size(18.0).strong()).wrap());
                ui.label(muted(format!(
                    "{} · {}",
                    source.name,
                    entry.kind.trim_start_matches("registry:")
                )));
            });
            ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                if icon_button(ui, Icon::Close, "Close details", false).clicked() {
                    self.studio.selected = None;
                }
                let favorite = self.studio.state.favorites.contains(&id);
                if icon_button(
                    ui,
                    if favorite { Icon::Heart } else { Icon::Star },
                    if favorite {
                        "Remove from favorites"
                    } else {
                        "Add to favorites"
                    },
                    favorite,
                )
                .clicked()
                {
                    self.studio.state.toggle_favorite(&id);
                    self.studio.save_state();
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            chip(ui, tr(source.terms.label()), terms_color(source.terms));
            chip(ui, source.license, colors().secondary);
            chip(ui, entry.framework.label(), colors().secondary);
            chip(ui, &entry.language, colors().muted);
            if entry.native.is_some() {
                chip(ui, tr("Available offline"), colors().success);
            }
        });
        ui.add_space(10.0);
        let (preview, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), 150.0), Sense::hover());
        self.preview(ui, &entry, preview, true);
        ui.add_space(10.0);
        let compat = self
            .studio
            .project
            .as_ref()
            .map(|p| lib::compatibility(&entry, p));
        ui.horizontal(|ui| {
            for (tab, label) in [
                (Tab::Overview, "Overview"),
                (Tab::Code, "Code"),
                (Tab::Playground, "Playground"),
                (Tab::Install, "Install"),
            ] {
                let r = ui.selectable_label(self.studio.tab == tab, tr(label));
                #[cfg(test)]
                super::motion::record(ui, format!("Library tab {label}"), r.rect);
                if r.clicked() {
                    self.studio.tab = tab;
                }
            }
        });
        ui.separator();
        ScrollArea::vertical()
            .id_salt(("library-detail", &id))
            .auto_shrink([false, false])
            .show(ui, |ui| match self.studio.tab {
                Tab::Overview => self.detail_overview(ui, &entry, compat.as_ref()),
                Tab::Code => self.detail_code(ui, &entry),
                Tab::Playground => self.detail_playground(ui, &entry),
                Tab::Install => self.detail_install(ui, &entry, compat.as_ref()),
            });
    }

    fn detail_overview(&mut self, ui: &mut Ui, entry: &lib::Entry, compat: Option<&Compatibility>) {
        let source = entry.source();
        if !entry.description.is_empty() {
            ui.add(
                egui::Label::new(RichText::new(&entry.description).color(colors().secondary))
                    .wrap(),
            );
            ui.add_space(10.0);
        }
        if let Some(compat) = compat {
            egui::Frame::new()
                .fill(compat_color(compat).gamma_multiply(0.08))
                .stroke(Stroke::new(
                    1.0_f32,
                    compat_color(compat).gamma_multiply(0.4),
                ))
                .corner_radius(8)
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(tr(compat_label(compat)))
                            .strong()
                            .color(compat_color(compat)),
                    );
                    match compat {
                        Compatibility::NeedsSetup(steps) => {
                            for step in steps {
                                ui.add(egui::Label::new(muted(step.clone())).wrap());
                            }
                        }
                        Compatibility::Incompatible(reason) | Compatibility::External(reason) => {
                            ui.add(egui::Label::new(muted(reason.clone())).wrap());
                        }
                        _ => {}
                    }
                });
            ui.add_space(10.0);
        }
        let row = |ui: &mut Ui, key: &str, value: String| {
            ui.horizontal(|ui| {
                ui.add_sized([104.0, 18.0], egui::Label::new(muted(key)));
                ui.add(egui::Label::new(RichText::new(value).size(12.5)).wrap());
            });
        };
        row(ui, "Source", source.name.to_owned());
        row(
            ui,
            "License",
            format!("{} — {}", source.license, tr(source.terms.label())),
        );
        row(ui, "Pricing", format!("{:?}", source.pricing));
        row(
            ui,
            "Framework",
            format!("{} · {}", entry.framework.label(), entry.language),
        );
        row(
            ui,
            "Categories",
            entry
                .categories
                .iter()
                .map(|c| tr(c.name()))
                .collect::<Vec<_>>()
                .join(", "),
        );
        if !entry.dependencies.is_empty() {
            row(ui, "Dependencies", entry.dependencies.join(", "));
        }
        if !entry.registry_dependencies.is_empty() {
            row(ui, "Requires", entry.registry_dependencies.join(", "));
        }
        if !entry.variants.is_empty() {
            row(ui, "Variants", entry.variants.join(", "));
        }
        if let Some(url) = entry.item_url(self.studio.project.as_ref()) {
            row(ui, "Registry item", url);
        }
        ui.add_space(8.0);
        ui.add(egui::Label::new(muted(source.notes.to_owned())).wrap());
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            let docs = entry.docs_url();
            if !docs.is_empty() && ui.button(tr("Open documentation")).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(docs));
            }
            if !source.repository.is_empty() && ui.button(tr("Repository")).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(source.repository));
            }
            if let Some(url) = entry.item_url(self.studio.project.as_ref())
                && ui.button(tr("Copy install command")).clicked()
            {
                let pm = self
                    .studio
                    .project
                    .as_ref()
                    .and_then(|p| p.package_manager)
                    .unwrap_or(lib::PackageManager::Npm);
                ui.ctx()
                    .copy_text(format!("{} shadcn@latest add {url}", pm.runner().join(" ")));
            }
        });
    }

    fn detail_code(&mut self, ui: &mut Ui, entry: &lib::Entry) {
        let terms = entry.source().terms;
        if entry.native.is_some() {
            ui.label(muted(
                "Native COVALOS resource. Its implementation lives in the COVALOS source tree:",
            ));
            ui.add_space(6.0);
            let snippet = native_snippet(entry.native);
            code_block(ui, snippet, "rs");
            if ui.button(tr("Copy")).clicked() {
                ui.ctx().copy_text(snippet.to_owned());
            }
            return;
        }
        if entry.item_url(None).is_none() || matches!(terms, Terms::Proprietary | Terms::Unverified)
        {
            empty_state(
                ui,
                "Source is not shown: the license is commercial or could not be verified. Open the official site instead.",
            );
            return;
        }
        let pending = self
            .studio
            .item_job
            .as_ref()
            .is_some_and(|(id, _)| *id == entry.id);
        let loaded = self
            .studio
            .item
            .as_ref()
            .filter(|(id, _)| *id == entry.id)
            .map(|(_, r)| r.clone());
        match loaded {
            None if pending => {
                ui.horizontal(|ui| {
                    busy_indicator(ui);
                    ui.label(muted("Fetching from the official registry…"));
                });
            }
            None => {
                ui.add(
                    egui::Label::new(muted(if terms == Terms::Open {
                        "Loads the component source from its official registry and caches it locally (permissive license)."
                    } else {
                        "Loads the component source from its official registry for viewing. COVALOS does not store it: the license forbids redistribution."
                    }))
                    .wrap(),
                );
                ui.add_space(6.0);
                let r = ui.add(primary(tr("Load source")));
                #[cfg(test)]
                super::motion::record(ui, "Load source", r.rect);
                if r.clicked() {
                    self.fetch_item(ui.ctx(), entry);
                }
            }
            Some(Err(error)) => {
                ui.label(RichText::new(error).color(colors().error));
                if ui.button(tr("Retry")).clicked() {
                    self.fetch_item(ui.ctx(), entry);
                }
            }
            Some(Ok(item)) => {
                ui.add(
                    egui::Label::new(muted(format!(
                        "© {} — {}. {}",
                        entry.source().name,
                        entry.source().license,
                        if terms == Terms::Open {
                            tr("Cached locally.")
                        } else {
                            tr("Not stored by COVALOS.")
                        }
                    )))
                    .wrap(),
                );
                for file in &item.files {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&file.path).monospace().size(12.0));
                        if let Some(content) = &file.content
                            && ui.small_button(tr("Copy")).clicked()
                        {
                            ui.ctx().copy_text(content.clone());
                        }
                    });
                    if let Some(content) = &file.content {
                        let ext = std::path::Path::new(&file.path)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("tsx");
                        let shown: String =
                            content.lines().take(400).collect::<Vec<_>>().join("\n");
                        code_block(ui, &shown, ext);
                    }
                }
            }
        }
    }

    fn fetch_item(&mut self, ctx: &egui::Context, entry: &lib::Entry) {
        let (send, receive) = mpsc::channel();
        let dir = self.studio.dir.clone();
        let target = entry.clone();
        let project = self.studio.project.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = send.send(lib::fetch_item(dir.as_deref(), &target, project.as_ref()));
            ctx.request_repaint();
        });
        self.studio.item_job = Some((entry.id.clone(), receive));
        self.studio.item = None;
    }

    fn detail_playground(&mut self, ui: &mut Ui, entry: &lib::Entry) {
        let p = &mut self.studio.play;
        match entry.native {
            Some(Native::DitherField | Native::DitherBloom) => {
                ui.add(egui::Slider::new(&mut p.density, 0.1..=1.0).text(tr("Density")));
                ui.add(egui::Slider::new(&mut p.strength, 0.3..=2.4).text(tr("Intensity")));
                ui.add(
                    egui::Slider::new(&mut p.pixel, 2.0..=8.0)
                        .step_by(1.0)
                        .text(tr("Pixel size")),
                );
                ui.label(muted("The preview above updates live."));
                if ui.button(tr("Use as Home background")).clicked() {
                    let prefs = &mut self.preferences.identity;
                    prefs.background = if entry.native == Some(Native::DitherBloom) {
                        super::identity::Background::Bloom
                    } else {
                        super::identity::Background::Dither
                    };
                    prefs.density = p.density;
                    prefs.intensity = (p.strength * 0.16).clamp(0.02, 0.40);
                }
            }
            Some(Native::SpringSnappy | Native::SpringSmooth | Native::SpringGentle) => {
                ui.add(egui::Slider::new(&mut p.stiffness, 60.0..=1200.0).text(tr("Stiffness")));
                ui.add(egui::Slider::new(&mut p.damping, 0.3..=1.2).text(tr("Damping ratio")));
                let k = p.stiffness.max(1.0);
                let settle = 4.0 / (p.damping.max(0.05) * k.sqrt());
                let overshoot = if p.damping < 1.0 {
                    (-std::f32::consts::PI * p.damping / (1.0 - p.damping * p.damping).sqrt()).exp()
                        * 100.0
                } else {
                    0.0
                };
                ui.label(muted(format!(
                    "≈ {:.0} ms · {:.1} %",
                    settle * 1000.0,
                    overshoot
                )));
                let code = format!(
                    "Spring {{ stiffness: {:.0}.0, damping: {:.2} }}",
                    p.stiffness, p.damping
                );
                code_block(ui, &code, "rs");
            }
            Some(Native::LucideIcons | Native::MaterialIcons) => {
                ui.add(
                    egui::Slider::new(&mut p.icon_size, 12.0..=32.0)
                        .step_by(1.0)
                        .text(tr("Size")),
                );
                if entry.native == Some(Native::LucideIcons) {
                    checkbox(ui, &mut p.accent, tr("Accent color"));
                }
            }
            None => {
                empty_state(
                    ui,
                    "Web components run in a browser runtime. COVALOS keeps its renderer native, so a sandboxed web preview is not part of this build. Open the official demo, or install the component and run your project's dev server.",
                );
                let docs = entry.docs_url();
                if !docs.is_empty() && ui.button(tr("Open live demo")).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(docs));
                }
            }
        }
    }

    fn detail_install(&mut self, ui: &mut Ui, entry: &lib::Entry, compat: Option<&Compatibility>) {
        let Some(project) = self.studio.project.clone() else {
            empty_state(ui, "Open a workspace to install components into it.");
            return;
        };
        let state = self
            .studio
            .install
            .as_ref()
            .filter(|(id, _)| *id == entry.id)
            .map(|(_, s)| s);
        enum Next {
            None,
            Plan(bool),
            Run(lib::Plan),
            Cancel,
            Revert(Vec<PathBuf>, PathBuf, String),
            Open(PathBuf),
        }
        let mut next = Next::None;
        match (compat, state) {
            (Some(Compatibility::Bundled), _) => {
                empty_state(
                    ui,
                    "Already part of COVALOS. Use the Playground tab to tune and apply it.",
                );
            }
            (Some(Compatibility::Incompatible(reason) | Compatibility::External(reason)), _) => {
                empty_state(ui, reason);
            }
            (_, None) => {
                ui.add(
                    egui::Label::new(muted(trf(
                        "Target: {path}",
                        &[("path", project.root.display().to_string())],
                    )))
                    .wrap(),
                );
                ui.add_space(6.0);
                let r = ui.add(primary(tr("Add to project")));
                #[cfg(test)]
                super::motion::record(ui, "Add to project", r.rect);
                if r.clicked() {
                    next = Next::Plan(false);
                }
            }
            (_, Some(Install::Planning(_))) => {
                ui.horizontal(|ui| {
                    busy_indicator(ui);
                    ui.label(muted(
                        "Reading the registry item and checking the workspace…",
                    ));
                });
            }
            (_, Some(Install::Review { plan, overwrite })) => {
                let mut overwrite = *overwrite;
                review(ui, plan);
                if !plan.conflicts.is_empty() {
                    ui.add_space(6.0);
                    if checkbox(ui, &mut overwrite, tr("Overwrite the conflicting files")).changed()
                    {
                        next = Next::Plan(overwrite);
                    }
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let allowed = plan.conflicts.is_empty() || overwrite;
                    let r = ui.add_enabled(allowed, primary(tr("Confirm and install")));
                    #[cfg(test)]
                    super::motion::record(ui, "Confirm and install", r.rect);
                    if r.clicked() {
                        next = Next::Run(plan.clone());
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        next = Next::Cancel;
                    }
                });
            }
            (_, Some(Install::Running { plan, log, .. })) => {
                ui.horizontal(|ui| {
                    busy_indicator(ui);
                    ui.label(
                        RichText::new(trf("Installing {id}…", &[("id", plan.entry.clone())]))
                            .strong(),
                    );
                });
                log_view(ui, log);
            }
            (
                _,
                Some(Install::Done {
                    ok,
                    log,
                    created,
                    modified,
                    backup,
                    plan,
                }),
            ) => {
                ui.label(
                    RichText::new(if *ok {
                        tr("Installed")
                    } else {
                        tr("Installation failed")
                    })
                    .strong()
                    .color(if *ok {
                        colors().success
                    } else {
                        colors().error
                    }),
                );
                ui.add_space(6.0);
                for (label, files) in [("Created", created), ("Modified", modified)] {
                    if files.is_empty() {
                        continue;
                    }
                    ui.label(muted(format!("{} ({})", tr(label), files.len())));
                    for file in files.iter().take(40) {
                        let shown = file
                            .strip_prefix(&project.root)
                            .unwrap_or(file)
                            .display()
                            .to_string();
                        if ui
                            .link(RichText::new(shown).monospace().size(12.0))
                            .clicked()
                        {
                            next = Next::Open(file.clone());
                        }
                    }
                }
                log_view(ui, log);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("Revert changes"))
                        .on_hover_text(tr(
                            "Restores package.json, lockfiles and config from the backup and deletes the created files.",
                        ))
                        .clicked()
                    {
                        next = Next::Revert(created.clone(), backup.clone(), plan.entry.clone());
                    }
                    if ui.button(tr("Done")).clicked() {
                        next = Next::Cancel;
                    }
                });
            }
            (_, Some(Install::Reverted(message))) => {
                ui.add(egui::Label::new(RichText::new(message).color(colors().secondary)).wrap());
                if ui.button(tr("Done")).clicked() {
                    next = Next::Cancel;
                }
            }
            (_, Some(Install::Failed(error))) => {
                ui.add(egui::Label::new(RichText::new(error).color(colors().error)).wrap());
                if ui.button(tr("Back")).clicked() {
                    next = Next::Cancel;
                }
            }
        }
        match next {
            Next::None => {}
            Next::Plan(overwrite) => self.plan_install(ui.ctx(), entry, overwrite),
            Next::Run(plan) => self.run_install(ui.ctx(), &project, plan),
            Next::Cancel => self.studio.install = None,
            Next::Revert(created, backup, id) => {
                let message = match lib::revert(&project.root, &backup, &created) {
                    Ok(()) => {
                        if let Some(record) = self
                            .studio
                            .state
                            .installed
                            .iter_mut()
                            .rev()
                            .find(|i| i.id == id)
                        {
                            record.reverted = true;
                        }
                        self.studio.save_state();
                        self.directories.clear();
                        tr("Reverted. Packages already in node_modules are pruned by your package manager on the next install.").to_owned()
                    }
                    Err(error) => format!("{}: {error}", tr("Revert failed")),
                };
                self.studio.install = Some((entry.id.clone(), Install::Reverted(message)));
            }
            Next::Open(file) => {
                self.studio_open = false;
                self.open_file(file);
            }
        }
    }

    fn plan_install(&mut self, ctx: &egui::Context, entry: &lib::Entry, overwrite: bool) {
        let Some(project) = self.studio.project.clone() else {
            return;
        };
        let (send, receive) = mpsc::channel();
        let dir = self.studio.dir.clone();
        let target = entry.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = match lib::fetch_item(dir.as_deref(), &target, Some(&project)) {
                Ok(item) => lib::plan(&target, &project, Some(&item), overwrite),
                Err(error) => Err(format!("Could not read the registry item. {error}")),
            };
            let _ = send.send(result);
            ctx.request_repaint();
        });
        self.studio.install = Some((entry.id.clone(), Install::Planning(receive)));
    }

    fn run_install(&mut self, ctx: &egui::Context, project: &lib::Project, plan: lib::Plan) {
        let Some(dir) = self.studio.dir.clone() else {
            return;
        };
        let backup =
            dir.join("backups")
                .join(format!("{}-{}", lib::now(), plan.entry.replace('/', "-")));
        if let Err(error) = lib::backup(&project.root, &backup, &plan.conflicts) {
            self.studio.install = Some((
                plan.entry.clone(),
                Install::Failed(format!("Backup failed: {error}")),
            ));
            return;
        }
        let before = lib::snapshot(&project.root);
        let (send, events) = mpsc::channel();
        let root = project.root.clone();
        let commands = plan.commands.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            for command in commands {
                let _ = send.send(Progress::Line(format!("> {}\n", command.join(" "))));
                if let Err(error) = run(&root, &command, &send, &ctx) {
                    let _ = send.send(Progress::Finished(Err(error)));
                    ctx.request_repaint();
                    return;
                }
            }
            let _ = send.send(Progress::Finished(Ok(())));
            ctx.request_repaint();
        });
        self.studio.install = Some((
            plan.entry.clone(),
            Install::Running {
                plan,
                events,
                log: String::new(),
                before,
                backup,
            },
        ));
    }

    fn poll_install(&mut self) {
        let root = self.studio.project.as_ref().map(|p| p.root.clone());
        let Some((id, install)) = &mut self.studio.install else {
            return;
        };
        let mut record = None;
        match install {
            Install::Planning(job) => {
                if let Ok(result) = job.try_recv() {
                    *install = match result {
                        Ok(plan) => Install::Review {
                            plan,
                            overwrite: false,
                        },
                        Err(error) => Install::Failed(error),
                    };
                }
            }
            Install::Running {
                plan,
                events,
                log,
                before,
                backup,
            } => {
                let mut finished = None;
                for event in events.try_iter().take(512) {
                    match event {
                        Progress::Line(line) => {
                            log.push_str(&line);
                            if log.len() > 60_000 {
                                let cut = log.ceil_char_boundary(log.len() - 60_000);
                                log.drain(..cut);
                            }
                        }
                        Progress::Finished(result) => finished = Some(result),
                    }
                }
                if let Some(result) = finished {
                    let root = root.unwrap_or_default();
                    let (created, modified) = lib::changes(before, &lib::snapshot(&root));
                    if let Err(error) = &result {
                        log.push_str(&format!("\n{error}\n"));
                    }
                    record = Some(lib::Installed {
                        id: id.clone(),
                        project: root,
                        at: lib::now(),
                        created: created.clone(),
                        modified: modified.clone(),
                        backup: Some(backup.clone()),
                        reverted: false,
                    });
                    *install = Install::Done {
                        plan: plan.clone(),
                        ok: result.is_ok(),
                        log: std::mem::take(log),
                        created,
                        modified,
                        backup: backup.clone(),
                    };
                }
            }
            _ => {}
        }
        if let Some(record) = record {
            self.studio.state.installed.push(record);
            self.studio.save_state();
            self.directories.clear();
        }
    }

    #[cfg(test)]
    pub(super) fn library_item_loaded(&self) -> bool {
        self.studio.item.is_some()
    }

    #[cfg(test)]
    pub(super) fn library_open_playground_tab(&mut self) {
        self.studio.tab = Tab::Playground;
    }

    #[cfg(test)]
    pub(super) fn library_review_ready(&self) -> bool {
        matches!(
            self.studio.install,
            Some((_, Install::Review { .. } | Install::Failed(_)))
        )
    }

    #[cfg(test)]
    pub(super) fn library_open_install_tab(&mut self) {
        self.studio.tab = Tab::Install;
    }

    #[cfg(test)]
    pub(super) fn library_open_code_tab(&mut self) {
        self.studio.tab = Tab::Code;
    }
}

/// Run one approved command in the project, streaming output. On Windows the package
/// runners are batch shims (npx.cmd, pnpm.cmd, yarn.cmd), which std spawns safely.
fn run(
    root: &std::path::Path,
    command: &[String],
    send: &mpsc::Sender<Progress>,
    ctx: &egui::Context,
) -> Result<(), String> {
    use std::io::{BufRead, BufReader};
    let program =
        if cfg!(windows) && matches!(command[0].as_str(), "npx" | "pnpm" | "yarn" | "bunx") {
            format!("{}.cmd", command[0])
        } else {
            command[0].clone()
        };
    let mut cmd = Command::new(&program);
    cmd.args(&command[1..])
        .current_dir(root)
        .env("CI", "1")
        .env("FORCE_COLOR", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::process::configure(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start {program}: {e}"))?;
    let streams: Vec<Box<dyn std::io::Read + Send>> = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    let readers: Vec<_> = streams
        .into_iter()
        .map(|stream| {
            let send = send.clone();
            let ctx = ctx.clone();
            thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let _ = send.send(Progress::Line(format!("{line}\n")));
                    ctx.request_repaint();
                }
            })
        })
        .collect();
    let status = child.wait().map_err(|e| e.to_string())?;
    for reader in readers {
        let _ = reader.join();
    }
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with {status}", command.join(" ")))
    }
}

fn review(ui: &mut Ui, plan: &lib::Plan) {
    ui.label(RichText::new(tr("Review before installing")).strong());
    ui.add_space(4.0);
    ui.label(muted("Commands"));
    for command in &plan.commands {
        command_block(ui, &command.join(" "));
    }
    let list = |ui: &mut Ui, title: &str, items: Vec<String>, color: Color32| {
        if items.is_empty() {
            return;
        }
        ui.add_space(6.0);
        ui.label(muted(format!("{} ({})", tr(title), items.len())));
        for item in items {
            ui.add(
                egui::Label::new(RichText::new(item).monospace().size(12.0).color(color)).wrap(),
            );
        }
    };
    let rel = |p: &PathBuf| {
        p.display()
            .to_string()
            .trim_start_matches(r"\\?\")
            .to_owned()
    };
    list(
        ui,
        "Files",
        plan.files.iter().map(rel).collect(),
        colors().text,
    );
    list(
        ui,
        "Conflicts (existing files)",
        plan.conflicts.iter().map(rel).collect(),
        colors().warning,
    );
    list(
        ui,
        "npm dependencies",
        plan.dependencies.clone(),
        colors().secondary,
    );
    list(
        ui,
        "Dev dependencies",
        plan.dev_dependencies.clone(),
        colors().secondary,
    );
    list(
        ui,
        "Registry dependencies",
        plan.registry_dependencies.clone(),
        colors().secondary,
    );
    ui.add_space(6.0);
    for note in &plan.notes {
        ui.add(egui::Label::new(muted(note.clone())).wrap());
    }
    ui.add(
        egui::Label::new(muted(trf(
            "License: {license}. A backup of package.json, lockfiles and config is taken first.",
            &[("license", plan.license.clone())],
        )))
        .wrap(),
    );
}

fn log_view(ui: &mut Ui, log: &str) {
    egui::Frame::new()
        .fill(colors().inset)
        .corner_radius(8)
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("library-install-log")
                .max_height(220.0)
                .stick_to_bottom(true)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(log).monospace().size(11.5)).wrap());
                });
        });
}

/// Render a shell command as clean wrapped selectable monospace text with a copy button,
/// avoiding both syntax-highlighter error underlines and horizontal scrollbar overlap.
fn command_block(ui: &mut Ui, command: &str) {
    egui::Frame::new()
        .fill(colors().inset)
        .stroke(Stroke::new(1.0_f32, colors().border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(
                    RichText::new(command)
                        .monospace()
                        .size(11.5)
                        .color(colors().text),
                )
                .wrap()
                .selectable(true),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(tr("Copy")).clicked() {
                        ui.ctx().copy_text(command.to_owned());
                    }
                });
            });
        });
}

fn code_block(ui: &mut Ui, code: &str, language: &str) {
    egui::Frame::new()
        .fill(colors().inset)
        .stroke(Stroke::new(1.0_f32, colors().border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let job =
                super::syntax::highlight(ui.ctx(), code, language, 12.0, 12.0 * CODE_LINE_HEIGHT);
            ScrollArea::horizontal()
                .id_salt(("code", code.len(), language))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(job).extend().selectable(true));
                });
        });
}

fn section_title(ui: &mut Ui, title: &str, detail: &str) {
    ui.label(RichText::new(tr(title)).size(LEAD).strong());
    if !detail.is_empty() {
        ui.add(egui::Label::new(muted(detail.to_owned())).wrap());
    }
    ui.add_space(8.0);
}

fn empty_state(ui: &mut Ui, text: &str) {
    ui.add_space(18.0);
    ui.vertical_centered(|ui| {
        let (_, mark) = ui.allocate_space(vec2(30.0, 30.0));
        draw_icon_sized(
            ui.painter(),
            mark.center(),
            Icon::Library,
            colors().muted,
            24.0,
        );
        ui.add(egui::Label::new(RichText::new(tr(text)).color(colors().secondary)).wrap());
    });
    ui.add_space(10.0);
}

fn terms_color(terms: Terms) -> Color32 {
    match terms {
        Terms::Open => colors().success,
        Terms::NoRedistribution => colors().warning,
        Terms::Proprietary => colors().error,
        Terms::Unverified => colors().muted,
    }
}

fn gamma(c: Color32) -> egui::Rgba {
    egui::Rgba::from_rgba_premultiplied(
        f32::from(c.r()) / 255.0,
        f32::from(c.g()) / 255.0,
        f32::from(c.b()) / 255.0,
        1.0,
    )
}

fn grid(rect: Rect, count: usize, mut draw: impl FnMut(usize, egui::Pos2)) {
    let columns = 6.min(count).max(1);
    let rows = count.div_ceil(columns);
    let cell = vec2(rect.width() / columns as f32, rect.height() / rows as f32);
    for i in 0..count {
        let (c, r) = (i % columns, i / columns);
        draw(
            i,
            rect.min + vec2((c as f32 + 0.5) * cell.x, (r as f32 + 0.5) * cell.y),
        );
    }
}

fn age(at: u64) -> String {
    let seconds = lib::now().saturating_sub(at);
    match seconds {
        0..=89 => tr("just now").to_owned(),
        90..=5399 => trf("{n} min ago", &[("n", (seconds / 60).to_string())]),
        5400..=172_799 => trf("{n} h ago", &[("n", (seconds / 3600).to_string())]),
        _ => trf("{n} days ago", &[("n", (seconds / 86_400).to_string())]),
    }
}

fn native_snippet(native: Option<Native>) -> &'static str {
    match native {
        Some(Native::DitherField | Native::DitherBloom) => {
            "// src/editor/dither.rs\nsuper::dither::paint(&painter, rect, super::dither::Frame {\n    variant: Variant::Field,\n    time, ink, accent,\n    strength: 1.0, density: 0.45, pixel: 4.0,\n    light: false, trail, ripples,\n});"
        }
        Some(Native::SpringSnappy | Native::SpringSmooth | Native::SpringGentle) => {
            "// src/editor/motion.rs\nlet x = motion::spring(ctx, id, target, motion::Spring::SMOOTH);\nlet rect = motion::spring_rect(ctx, id, target_rect, motion::Spring::GENTLE);"
        }
        Some(Native::LucideIcons) => {
            "// src/editor/theme.rs\ndraw_icon_sized(&painter, center, Icon::Search, colors().muted, 16.0);"
        }
        Some(Native::MaterialIcons) => "// src/editor/assets.rs\nassets::file(ui, center, &path);",
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::super::preferences::Preferences;
    use super::*;

    #[test]
    fn studio_renders_views_and_filters_without_network() -> std::io::Result<()> {
        let folder = tempfile::tempdir()?;
        let ctx = egui::Context::default();
        let mut editor = Editor::new(&ctx, Some(folder.path().to_owned()), Preferences::default());
        editor.studio.load_from(folder.path().join("library"));
        editor.open_library();
        let frame = |editor: &mut Editor| {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1400.0, 900.0))),
                    ..Default::default()
                },
                |ctx| editor.render(ctx),
            );
            assert!(!output.shapes.is_empty());
        };
        frame(&mut editor);
        for view in [
            View::Discover,
            View::Category(Category::Backgrounds),
            View::Favorites,
            View::Installed,
            View::Sources,
        ] {
            editor.studio.view = view;
            frame(&mut editor);
        }
        editor.studio.view = View::Discover;
        editor.studio.query.text = "spring".into();
        frame(&mut editor);
        assert!(!editor.studio.results().is_empty());
        let results = editor.studio.results().to_vec();
        assert!(results.iter().all(|&i| {
            editor.studio.catalog.entries[i]
                .id
                .starts_with("covalos/spring")
        }));
        editor.studio.select("covalos/dither-field");
        for tab in [Tab::Overview, Tab::Code, Tab::Playground, Tab::Install] {
            editor.studio.tab = tab;
            frame(&mut editor);
        }
        editor.studio.state.toggle_favorite("covalos/dither-field");
        editor.studio.save_state();
        let reloaded = lib::State::load(Some(&folder.path().join("library")));
        assert_eq!(reloaded.favorites, ["covalos/dither-field"]);
        assert_eq!(
            reloaded.recent.first().map(String::as_str),
            Some("covalos/dither-field")
        );
        Ok(())
    }
}
