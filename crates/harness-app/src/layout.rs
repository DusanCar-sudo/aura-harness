//! Top-level egui layout — the `eframe::App::update` render pass,
//! moved out of `main.rs` to keep both under the size ceiling.

use std::time::Duration;

use eframe::egui;

use harness_core::agent::Approval;
use harness_core::api::Message;

use crate::theme;
use crate::ui;
use crate::ui::chat::{ChatAction, ChatState};
use crate::ui::input::{ComposerAction, ComposerState};
use crate::ui::plugin_browser::{BrowserAction, BrowserState};
use crate::ui::settings::SettingsState;
use crate::ui::sidebar::SidebarState;
use crate::ui::RightTab;
use crate::{load_img_texture, App, View};

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events(ctx);
        self.absorb_mcp();
        self.pump_online_search();
        self.pump_directory();
        self.pump_analyze(ctx);

        // Repaint immediately when the theme picker moves, so it can be tried
        // before it is saved.
        if self.config.theme != self.applied_theme {
            self.applied_theme = self.config.theme;
            theme::apply(ctx, self.applied_theme);
        }
        // ---- background image ------------------------------------------
        let bg_path = self.config.background_image.trim().to_string();
        if bg_path != self.bg_loaded_path {
            self.bg_loaded_path = bg_path.clone();
            self.bg_texture = if bg_path.is_empty() {
                None
            } else {
                match load_img_texture(ctx, &bg_path) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        self.error = Some(format!("background image not loaded: {e}"));
                        None
                    }
                }
            };
        }
        if self.bg_texture.is_some() {
            if let Some(tex) = &self.bg_texture {
                // Full-window wallpaper on the background layer; panels sit on
                // top with alpha, so the image reads through every section.
                // Panels paint on egui's shared background layer; painting
                // there FIRST (this runs before any panel is created) puts the
                // image underneath everything. A private layer sorted after
                // the panels' layer would cover the UI instead.
                let layer = ctx.layer_painter(egui::LayerId::background());
                layer.image(
                    tex.id(),
                    ctx.screen_rect(),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        }
        // Panels go translucent every frame while a background is live, so the
        // slider works immediately instead of only on the next restart.
        let bg_on = self.bg_texture.is_some();
        if bg_on != self.bg_blend_active {
            self.bg_blend_active = bg_on;
            theme::apply(ctx, self.applied_theme);
        }
        // Persist background edits immediately (settings feels broken if a
        // restart silently drops the choice).
        let tab_now = match self.right_tab {
            RightTab::Files => "files",
            RightTab::Preview => "preview",
            RightTab::Plugins => "plugins",
        };
        if tab_now != self.config.right_panel_tab.trim() {
            self.config.right_panel_tab = tab_now.into();
            let _ = self.config.save();
        }
        if self.file_tree.show_hidden != self.config.show_hidden_files {
            self.config.show_hidden_files = self.file_tree.show_hidden;
            let _ = self.config.save();
        }
        if self.bg_saved_image != self.config.background_image.trim()
            || (self.bg_saved_opacity - self.config.background_opacity).abs() > 0.001
            || self.bg_saved_auto_approve != self.config.auto_approve
        {
            self.bg_saved_image = self.config.background_image.trim().to_string();
            self.bg_saved_opacity = self.config.background_opacity;
            self.bg_saved_auto_approve = self.config.auto_approve;
            let _ = self.config.save();
        }
        if bg_on {
            let p = theme::palette_for(self.applied_theme);
            // blend = how much of the image shows through; panels keep the rest.
            let blend = self.config.background_opacity.clamp(0.0, 1.0);
            let alpha = ((1.0 - blend) * 255.0).round() as u8;
            ctx.style_mut(|s| {
                let a = |c: egui::Color32| egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha);
                s.visuals.panel_fill = a(p.panel);
                s.visuals.window_fill = a(p.card);
                s.visuals.faint_bg_color = a(p.card);
                s.visuals.extreme_bg_color = a(p.field);
                s.visuals.code_bg_color = a(p.field);
                s.visuals.widgets.noninteractive.bg_fill = a(p.panel);
                s.visuals.widgets.noninteractive.weak_bg_fill = a(p.panel);
                s.visuals.widgets.inactive.bg_fill = a(p.card);
                s.visuals.widgets.inactive.weak_bg_fill = a(p.card);
                s.visuals.widgets.hovered.bg_fill = a(p.bubble);
                s.visuals.widgets.hovered.weak_bg_fill = a(p.bubble);
            });
        }

        let current_id = self.session.as_ref().map(|s| s.header.id.clone());
        let plugin_line = self.plugin_status_line();

        let mut sidebar_action = None;
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(230.0)
            .width_range(180.0..=340.0)
            .show(ctx, |ui| {
                sidebar_action = ui::sidebar::show(
                    ui,
                    &SidebarState {
                        sessions: &self.sessions,
                        current_id: current_id.as_deref(),
                        plugin_status: &plugin_line,
                        plugin_ok: self.mcp_failures.is_empty(),
                        mark: self.logo.as_ref(),
                        theme: self.config.theme,
                    },
                );
            });
        if let Some(action) = sidebar_action {
            self.handle_sidebar(action);
        }

        // Right-hand panel: one box, three tabs — the file tree, the artifact
        // preview of whatever is being built, and the curated plugin browser.
        if self.right_panel {
            let mut close_right = false;
            let mut clicked_add: Option<(String, String)> = None;
            let mut clicked_file: Option<std::path::PathBuf> = None;
            let configured: Vec<String> = self.plugins.mcp.keys().cloned().collect();
            let mut query_edit = self.catalog_query.clone();
            let mut tab = self.right_tab;
            let project_root = self.file_tree.root.clone();
            egui::SidePanel::right("right-panel")
                .resizable(true)
                .default_width(340.0)
                .width_range(260.0..=640.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut tab, RightTab::Files, "📁 Files");
                        ui.selectable_value(&mut tab, RightTab::Preview, "🖼 Preview");
                        ui.selectable_value(&mut tab, RightTab::Plugins, "🔌 Plugins");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").on_hover_text("Hide the panel").clicked() {
                                close_right = true;
                            }
                        });
                    });
                    ui.separator();
                    match tab {
                        RightTab::Files => {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                egui::CollapsingHeader::new("Project files")
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        if let Some(p) = self.file_tree.show(ui) {
                                            clicked_file = Some(p);
                                        }
                                    });
                            });
                        }
                        RightTab::Preview => {
                            ui::preview::show(ui, ctx, &mut self.preview, &project_root);
                        }
                        RightTab::Plugins => {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                ui.label("Plugin search");
                                ui.text_edit_singleline(&mut query_edit);
                                if query_edit.trim().len() < 2 {
                                    ui.small(
                                        "Type to search (e.g. gaming, social media, 3d game, summertime)",
                                    );
                                } else {
                                    let hits = harness_core::catalog::search(&query_edit);
                                    if hits.is_empty() {
                                        ui.small("No matches.");
                                    }
                                    for e in hits {
                                        if configured.contains(&harness_core::catalog::plugin_key(e)) {
                                            continue;
                                        }
                                        ui.horizontal(|ui| {
                                            ui.add(egui::Label::new(e.name).truncate());
                                            if ui.small_button("Add").clicked() {
                                                clicked_add = Some((
                                                    e.name.to_string(),
                                                    e.launch_line.to_string(),
                                                ));
                                            }
                                        });
                                    }
                                }
                            });
                        }
                    }
                });
            self.right_tab = tab;
            self.catalog_query = query_edit;
            if close_right {
                self.right_panel = false;
            }
            if self.file_tree.root_changed {
                self.file_tree.root_changed = false;
                self.config.project_root = self.file_tree.root.display().to_string();
                let _ = self.config.save();
                self.preview.force_rescan(&self.file_tree.root.clone());
            }
            if let Some(p) = clicked_file {
                let hint = format!("@{}", p.display());
                if self.draft.is_empty() {
                    self.draft.push_str(&hint);
                } else {
                    self.draft.push_str(&format!(" {}", hint));
                }
            }
            if let Some((n, l)) = clicked_add {
                self.add_mcp_server(&n, &l);
            }
        }

        // Composer (hidden in settings so the form gets the room).
        if self.view == View::Chat {
            let mut composer_action = None;
            egui::TopBottomPanel::bottom("composer").show(ctx, |ui| {
                ui.add_space(4.0);
                composer_action = ui::input::show(
                    ui,
                    &mut ComposerState {
                        draft: &mut self.draft,
                        attachments: &self.attachments,
                        model: &mut self.model,
                        effort: &mut self.effort,
                        selected_model: if self.config.model_id.trim().is_empty() { None } else { Some(self.config.model_id.trim().to_string()) },
                        busy: self.busy,
                        tool_count: self.registry.len(),
                    },
                );
                if let Some(err) = &self.error {
                    ui.colored_label(theme::danger(), err);
                }
                ui.add_space(4.0);
            });

            match composer_action {
                Some(ComposerAction::Send) => self.send(ctx),
                Some(ComposerAction::Stop) => self.stop(),
                Some(ComposerAction::Attach) => self.attach_files(),
                Some(ComposerAction::RemoveAttachment(i)) => self.remove_attachment(i),
                None => {}
            }
        }

        let mut settings_action = None;
        let mut chat_action = None;
        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            View::Chat => {
                let messages: &[Message] = self
                    .session
                    .as_ref()
                    .map(|s| s.messages.as_slice())
                    .unwrap_or(&[]);
                chat_action = ui::chat::show(
                    ui,
                    &mut ChatState {
                        messages,
                        live: &self.live,
                        cards: &self.cards,
                        busy: self.busy,
                        cache: &mut self.md_cache,
                    },
                );
            }
            View::Settings => {
                let cmd_tools: Vec<String> = self
                    .plugins
                    .cmd
                    .iter()
                    .map(|(name, tool)| format!("{name} — {}", tool.command))
                    .collect();
                settings_action = ui::settings::show(
                    ui,
                    &mut SettingsState {
                        config: &mut self.config,
                        plugins: &mut self.plugins,
                        key_input: &mut self.key_input,
                        key_present: self.api_key.is_some(),
                        mcp_status: &self.mcp_status,
                        cmd_tools: &cmd_tools,
                        tool_count: self.registry.len(),
                    },
                );
            }
        });

        match chat_action {
            Some(ChatAction::Approve(id)) => self.decide(&id, Approval::Approve),
            Some(ChatAction::Reject(id)) => self.decide(&id, Approval::Reject),
            None => {}
        }
        if let Some(action) = settings_action {
            self.apply_settings(action, ctx);
        }

        // Plugin browser: a searchable window over catalogue + machine.
        if self.show_browser {
            let mut browser_action = None;
            let configured: Vec<String> = self.plugins.mcp.keys().cloned().collect();
            egui::Window::new("Add a plugin")
                .id(egui::Id::new("plugin-browser"))
                .default_width(520.0)
                .resizable(true)
                .open(&mut self.show_browser)
                .show(ctx, |ui| {
                    if let Some(note) = &self.status {
                        ui.label(egui::RichText::new(note).color(theme::ok()).small());
                    }
                    if let Some(err) = &self.error {
                        ui.label(egui::RichText::new(err).color(theme::danger()).small());
                    }
                    browser_action = ui::plugin_browser::show(
                        ui,
                        &mut BrowserState {
                            query: &mut self.catalog_query,
                            line: &mut self.browser_line,
                            available: &self.available,
                            configured: &configured,
                            online: &self.online,
                            directory: &self.directory,
                        },
                    );
                });
            match browser_action {
                Some(BrowserAction::Add { name, line }) => {
                    self.add_mcp_server(&name, &line);
                    // Keep the window open so more can be chosen.
                    self.show_browser = true;
                }
                Some(BrowserAction::Analyze(url)) => {
                    self.start_analyze(&url);
                    self.show_browser = true;
                }
                Some(BrowserAction::Open(url)) => {
                    ctx.open_url(egui::OpenUrl::new_tab(url));
                }
                Some(BrowserAction::Close) => self.show_browser = false,
                None => {}
            }
        }

        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}
