// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// DeepSynth Preset — native egui/eframe desktop app.
//
// A "bring your own key" tool: type a text prompt, pick a synth and an LLM
// provider (Claude or Gemini), and generate a preset file (Surge XT `.fxp`,
// Dexed `.syx`, Vital `.vital`). The generation runs on a background thread and
// posts its result back through a channel, so the UI never blocks. Generated
// parameters are shown in a table; a save dialog (rfd) writes the file. A
// settings panel stores each provider's API key in the OS keychain via
// `keyring`.

mod theme;

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender};

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

use preset_core::claude::key::KeySource;
use preset_core::param::ParamValue;
use preset_core::{PresetMeta, Provider, Synth};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([920.0, 700.0])
            .with_min_inner_size([640.0, 480.0])
            .with_title("DeepSynth Preset"),
        ..Default::default()
    };
    eframe::run_native(
        "DeepSynth Preset",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}

/// The result of a background generation job.
enum GenResult {
    Ok {
        bytes: Vec<u8>,
        mapped: BTreeMap<String, ParamValue>,
    },
    Err(String),
}

/// The app state.
struct App {
    prompt: String,
    name: String,
    synth: Synth,
    provider: Provider,
    /// Optional model-id override; empty = use the provider's default model.
    model: String,

    // Generation state.
    generating: bool,
    rx: Option<Receiver<GenResult>>,
    status: String,
    last_bytes: Option<Vec<u8>>,
    last_mapped: BTreeMap<String, ParamValue>,
    // Provider used for the last successful generation (for the meta line).
    last_provider: Provider,

    // API key settings — one editable field per provider.
    show_settings: bool,
    claude_key_input: String,
    gemini_key_input: String,
    openai_key_input: String,
    openrouter_key_input: String,
    key_status: String,

    // Self-capture hook for docs/demo screenshots (no OS screen-recording
    // permission needed). Set DEEPSYNTH_APP_CAPTURE=<out.png> to enable;
    // DEEPSYNTH_DEMO_PROMPT/_SYNTH/_PROVIDER optionally run one generation
    // first and capture after it completes. The app exits after saving.
    capture_path: Option<std::path::PathBuf>,
    capture_demo: bool,
    capture_requested: bool,
    frames: u32,
}

impl App {
    fn new() -> Self {
        let provider = Provider::Claude;
        let key_status = key_status_for(provider);
        Self {
            prompt: String::new(),
            name: "DeepSynth Patch".to_string(),
            synth: Synth::Surge,
            provider,
            model: String::new(),
            generating: false,
            rx: None,
            status: "Enter a prompt and click Generate.".to_string(),
            last_bytes: None,
            last_mapped: BTreeMap::new(),
            last_provider: provider,
            show_settings: false,
            claude_key_input: String::new(),
            gemini_key_input: String::new(),
            openai_key_input: String::new(),
            openrouter_key_input: String::new(),
            key_status,
            capture_path: std::env::var_os("DEEPSYNTH_APP_CAPTURE").map(Into::into),
            capture_demo: false,
            capture_requested: false,
            frames: 0,
        }
    }

    /// Drive the self-capture hook: optionally run one demo generation, then
    /// request a viewport screenshot, save it as PNG, and exit.
    fn drive_capture(&mut self, ctx: &egui::Context) {
        let Some(path) = self.capture_path.clone() else {
            return;
        };
        ctx.request_repaint(); // keep frames flowing while we wait
        self.frames += 1;

        let demo_prompt = std::env::var("DEEPSYNTH_DEMO_PROMPT").ok();
        if !self.capture_demo && self.frames == 3 {
            self.capture_demo = true;
            if std::env::var("DEEPSYNTH_DEMO_VIEW").as_deref() == Ok("settings") {
                self.show_settings = true;
            }
            if let Some(p) = &demo_prompt {
                self.prompt = p.clone();
                match std::env::var("DEEPSYNTH_DEMO_SYNTH").as_deref() {
                    Ok("dexed") => self.synth = Synth::Dexed,
                    Ok("vital") => self.synth = Synth::Vital,
                    _ => self.synth = Synth::Surge,
                }
                if std::env::var("DEEPSYNTH_DEMO_PROVIDER").as_deref() == Ok("gemini") {
                    self.provider = Provider::Gemini;
                    self.key_status = key_status_for(self.provider);
                }
                if let Ok(name) = std::env::var("DEEPSYNTH_DEMO_NAME") {
                    self.name = name;
                }
                self.start_generate(ctx);
            }
        }

        let ready = if demo_prompt.is_some() {
            self.capture_demo && !self.generating && self.last_bytes.is_some()
        } else {
            self.frames >= 8
        };
        if ready && !self.capture_requested {
            self.capture_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }

        let shot: Option<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let [w, h] = image.size;
            let rgba: Vec<u8> = image
                .pixels
                .iter()
                .flat_map(|c| c.to_srgba_unmultiplied())
                .collect();
            match image::save_buffer(&path, &rgba, w as u32, h as u32, image::ColorType::Rgba8) {
                Ok(()) => eprintln!("capture saved: {}", path.display()),
                Err(e) => eprintln!("capture save failed: {e}"),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Kick off a background generation job.
    fn start_generate(&mut self, ctx: &egui::Context) {
        let prompt = self.prompt.trim().to_string();
        if prompt.is_empty() {
            self.status = "Please enter a prompt.".to_string();
            return;
        }
        let provider = self.provider;
        // Local backends (Ollama, LM Studio) need no key; hosted ones do.
        let key = match provider.resolve_key(None) {
            Some((k, _src)) => k,
            None if !provider.requires_key() => String::new(),
            None => {
                self.status = format!(
                    "No {} API key. Open Settings to add one (or set {}).",
                    provider.label(),
                    provider.env_var()
                );
                self.show_settings = true;
                return;
            }
        };

        let (tx, rx): (Sender<GenResult>, Receiver<GenResult>) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        self.last_provider = provider;
        self.generating = true;
        self.status = "Generating… (this can take up to a couple of minutes)".to_string();

        let synth = self.synth;
        let name = self.name.clone();
        let model = self.model.trim().to_string();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let meta = PresetMeta {
                name,
                ..Default::default()
            };
            let model_opt = if model.is_empty() { None } else { Some(model.as_str()) };
            let result = match preset_core::generate_preset_with(
                provider, synth, &key, model_opt, &prompt, &meta,
            ) {
                Ok(gen) => GenResult::Ok {
                    bytes: gen.bytes,
                    mapped: gen.mapped,
                },
                Err(e) => GenResult::Err(format!("{e}")),
            };
            let _ = tx.send(result);
            ctx.request_repaint(); // wake the UI to pick up the result
        });
    }

    /// Poll the channel for a finished job.
    fn poll_result(&mut self) {
        if let Some(rx) = &self.rx {
            if let Ok(result) = rx.try_recv() {
                self.generating = false;
                self.rx = None;
                match result {
                    GenResult::Ok { bytes, mapped } => {
                        self.status = format!(
                            "Generated {} ({} bytes, {} parameters). Use Save to write the file.",
                            self.synth.id(),
                            bytes.len(),
                            mapped.len()
                        );
                        self.last_bytes = Some(bytes);
                        self.last_mapped = mapped;
                    }
                    GenResult::Err(e) => {
                        self.status = format!("Generation failed: {e}");
                    }
                }
            }
        }
    }

    /// Open a save dialog and write the last-generated preset.
    fn save_dialog(&mut self) {
        let Some(bytes) = &self.last_bytes else {
            self.status = "Nothing to save yet — generate a preset first.".to_string();
            return;
        };
        let default_name = format!("{}.{}", sanitize(&self.name), self.synth.extension());
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter(self.synth.extension(), &[self.synth.extension()])
            .save_file()
        {
            match std::fs::write(&path, bytes) {
                Ok(()) => self.status = format!("Saved {}.", path.display()),
                Err(e) => self.status = format!("Could not save: {e}"),
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_result();
        self.drive_capture(ctx);

        // --- top bar --------------------------------------------------------
        let header_frame = egui::Frame::new()
            .fill(theme::BG)
            .inner_margin(egui::Margin {
                left: 16,
                right: 16,
                top: 12,
                bottom: 12,
            })
            .stroke(Stroke::new(1.0, theme::LINE));
        egui::TopBottomPanel::top("top")
            .frame(header_frame)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::wordmark(ui, 22.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Ghost Settings button: transparent fill, accent hover border.
                        let btn = egui::Button::new(
                            RichText::new("⚙  Settings").color(theme::MUTED),
                        )
                        .fill(Color32::TRANSPARENT);
                        if ui.add(btn).clicked() {
                            self.show_settings = true;
                        }
                    });
                });
            });

        // --- settings window (API key) -------------------------------------
        if self.show_settings {
            self.settings_window(ctx);
        }

        // --- main panel -----------------------------------------------------
        let central_frame = egui::Frame::new()
            .fill(theme::BG)
            .inner_margin(egui::Margin::same(18));
        egui::CentralPanel::default()
            .frame(central_frame)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        theme::field_label(ui, "Synth");
                        egui::ComboBox::from_id_salt("synth")
                            .selected_text(self.synth.id())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.synth,
                                    Synth::Surge,
                                    "Surge XT (.fxp)",
                                );
                                ui.selectable_value(
                                    &mut self.synth,
                                    Synth::Dexed,
                                    "Dexed / DX7 (.syx)",
                                );
                                ui.selectable_value(
                                    &mut self.synth,
                                    Synth::Vital,
                                    "Vital (.vital)",
                                );
                            });
                    });
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        theme::field_label(ui, "Provider");
                        let prev_provider = self.provider;
                        egui::ComboBox::from_id_salt("provider")
                            .selected_text(self.provider.label())
                            .show_ui(ui, |ui| {
                                for p in Provider::ALL {
                                    ui.selectable_value(&mut self.provider, *p, p.label());
                                }
                            });
                        if self.provider != prev_provider {
                            // Reflect the newly-selected provider's key status.
                            self.key_status = key_status_for(self.provider);
                        }
                    });
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        theme::field_label(ui, "Preset name");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.name).desired_width(220.0),
                        );
                    });
                });

                ui.add_space(12.0);
                theme::field_label(ui, "Describe the sound");
                ui.add_space(2.0);
                ui.add(
                    egui::TextEdit::multiline(&mut self.prompt)
                        .hint_text("e.g. warm analog pad with slow attack and gentle movement")
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );

                ui.add_space(12.0);
                theme::field_label(ui, "Model");
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.model)
                            .hint_text(self.provider.default_model())
                            .desired_width(240.0),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("blank = {}", self.provider.default_model()))
                            .color(theme::MUTED),
                    );
                    if !self.provider.requires_key() {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("local backend — no API key needed")
                                .color(theme::MUTED),
                        );
                    }
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let gen_enabled = !self.generating;
                    // Primary accent-pink Generate button.
                    let gen = egui::Button::new(
                        RichText::new("Generate").color(Color32::WHITE).strong(),
                    )
                    .fill(theme::ACCENT)
                    .stroke(Stroke::new(1.0, theme::ACCENT))
                    .min_size(egui::vec2(120.0, 0.0));
                    let gen_resp = ui.add_enabled(gen_enabled, gen);
                    if gen_enabled && gen_resp.hovered() {
                        // Brighten on hover.
                        ui.painter().rect_filled(
                            gen_resp.rect,
                            CornerRadius::same(theme::RADIUS),
                            Color32::from_white_alpha(26),
                        );
                    }
                    if gen_resp.clicked() {
                        self.start_generate(ctx);
                    }

                    let save_enabled = self.last_bytes.is_some() && !self.generating;
                    // Ghost Save button with teal hover border.
                    let save = egui::Button::new(
                        RichText::new("Save…")
                            .color(if save_enabled { theme::TEXT } else { theme::MUTED }),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, theme::LINE));
                    let save_resp = ui.add_enabled(save_enabled, save);
                    if save_enabled && save_resp.hovered() {
                        ui.painter().rect_stroke(
                            save_resp.rect,
                            CornerRadius::same(theme::RADIUS),
                            Stroke::new(1.0, theme::ACCENT2),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if save_resp.clicked() {
                        self.save_dialog();
                    }

                    if self.generating {
                        ui.add_space(4.0);
                        ui.add(egui::Spinner::new().color(theme::ACCENT));
                    }
                });

                ui.add_space(10.0);
                ui.colored_label(status_color(&self.status), &self.status);
                ui.add_space(10.0);

                // --- parameter table --------------------------------------
                if !self.last_mapped.is_empty() {
                    let bytes = self.last_bytes.as_ref().map(|b| b.len()).unwrap_or(0);
                    let meta = format!(
                        "{} parameters  ·  .{}  ·  {} bytes  ·  {}",
                        self.last_mapped.len(),
                        self.synth.extension(),
                        bytes,
                        self.last_provider.id(),
                    );
                    ui.label(RichText::new(meta).size(12.5).color(theme::MUTED));
                    ui.add_space(6.0);

                    let card = egui::Frame::new()
                        .fill(theme::PANEL)
                        .stroke(Stroke::new(1.0, theme::LINE))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(egui::Margin::same(12));
                    card.show(ui, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            egui::Grid::new("params")
                                .striped(true)
                                .num_columns(2)
                                .min_col_width(240.0)
                                .spacing(egui::vec2(16.0, 4.0))
                                .show(ui, |ui| {
                                    for (k, v) in &self.last_mapped {
                                        ui.monospace(RichText::new(k).color(theme::MONO));
                                        let (val, color) = match v {
                                            ParamValue::Num(_) => (format_value(v), theme::ACCENT2),
                                            _ => (format_value(v), theme::ACCENT),
                                        };
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.monospace(RichText::new(val).color(color));
                                            },
                                        );
                                        ui.end_row();
                                    }
                                });
                        });
                    });
                } else {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "No preset generated yet. Enter a prompt above and click Generate. \
                             You'll need an API key for the selected provider (Settings).",
                        )
                        .color(theme::MUTED),
                    );
                }
            });
    }
}

impl App {
    /// The API-key settings window: one key row per provider.
    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        // Opaque card frame (same style as the parameters table) so the window
        // reads as a solid panel instead of letting the rack bleed through.
        let win_frame = egui::Frame::new()
            .fill(theme::PANEL)
            .stroke(Stroke::new(1.0, theme::LINE))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(16))
            .shadow(egui::epaint::Shadow {
                offset: [0, 8],
                blur: 24,
                spread: 0,
                color: Color32::from_black_alpha(140),
            });
        egui::Window::new(RichText::new("Settings — API keys").color(theme::TEXT))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .frame(win_frame)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Bring your own API keys. Each is stored in your OS keychain \
                         (service \"deepsynth-preset\") and never written to disk in plaintext.",
                    )
                    .color(theme::MUTED),
                );
                ui.add_space(6.0);
                ui.colored_label(status_color(&self.key_status), &self.key_status);
                ui.separator();

                Self::provider_key_row(
                    ui,
                    Provider::Claude,
                    "sk-ant-…",
                    &mut self.claude_key_input,
                    &mut self.key_status,
                );
                ui.add_space(8.0);
                Self::provider_key_row(
                    ui,
                    Provider::Gemini,
                    "AIza…",
                    &mut self.gemini_key_input,
                    &mut self.key_status,
                );
                ui.add_space(8.0);
                Self::provider_key_row(
                    ui,
                    Provider::OPENAI,
                    "sk-…",
                    &mut self.openai_key_input,
                    &mut self.key_status,
                );
                ui.add_space(8.0);
                Self::provider_key_row(
                    ui,
                    Provider::OPENROUTER,
                    "sk-or-…",
                    &mut self.openrouter_key_input,
                    &mut self.key_status,
                );

                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "A provider's environment variable (ANTHROPIC_API_KEY / GEMINI_API_KEY / \
                         OPENAI_API_KEY / OPENROUTER_API_KEY) takes priority over the keychain if set.",
                    )
                    .size(11.5)
                    .color(theme::MUTED),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Local backends — Ollama (localhost:11434) and LM Studio (localhost:1234) — \
                         need no key. Override their host with OLLAMA_BASE_URL / LMSTUDIO_BASE_URL, \
                         and set the model above (e.g. llama3.1).",
                    )
                    .size(11.5)
                    .color(theme::MUTED),
                );
            });
        self.show_settings = open;
    }

    /// Render one provider's labeled key field + save/clear buttons.
    fn provider_key_row(
        ui: &mut egui::Ui,
        provider: Provider,
        hint: &str,
        input: &mut String,
        key_status: &mut String,
    ) {
        ui.label(egui::RichText::new(provider.label()).strong());
        ui.horizontal(|ui| {
            ui.label("Key:");
            ui.add(
                egui::TextEdit::singleline(input)
                    .password(true)
                    .hint_text(hint)
                    .desired_width(320.0),
            );
        });
        ui.horizontal(|ui| {
            if ui.button("Save to keychain").clicked() {
                let k = input.trim().to_string();
                if k.is_empty() {
                    *key_status = "Enter a key first.".to_string();
                } else {
                    match provider.store_key(&k) {
                        Ok(()) => {
                            input.clear();
                            *key_status =
                                format!("{} key saved to the OS keychain.", provider.label());
                        }
                        Err(e) => *key_status = format!("Could not save: {e}"),
                    }
                }
            }
            if ui.button("Clear stored key").clicked() {
                match provider.clear_key() {
                    Ok(()) => *key_status = format!("{} key cleared.", provider.label()),
                    Err(e) => *key_status = format!("Could not clear: {e}"),
                }
            }
        });
    }
}

/// Describe where the selected provider's key currently resolves from.
fn key_status_for(provider: Provider) -> String {
    match provider.resolve_key(None) {
        Some((_, KeySource::Env)) => {
            format!("{} key found via {}.", provider.label(), provider.env_var())
        }
        Some((_, KeySource::Keychain)) => {
            format!("{} key found in the OS keychain.", provider.label())
        }
        Some((_, KeySource::Explicit)) => format!("{} key set.", provider.label()),
        None if !provider.requires_key() => {
            format!("{} is a local backend — no API key needed.", provider.label())
        }
        None => format!(
            "No {} API key found. Open Settings to add one.",
            provider.label()
        ),
    }
}

/// Pick a status-line color: teal for success / engine output, pink for
/// warnings and errors, muted for the neutral idle prompt.
fn status_color(status: &str) -> Color32 {
    let s = status.to_ascii_lowercase();
    if s.contains("fail") || s.contains("could not") || s.starts_with("no ") || s.contains("please")
        || s.contains("nothing")
    {
        theme::ACCENT // pink warning / error
    } else if s.contains("generated") || s.contains("saved") {
        theme::ACCENT2 // teal success
    } else {
        theme::MUTED // neutral / in-progress
    }
}

/// Format a mapped value for the table.
fn format_value(v: &ParamValue) -> String {
    match v {
        ParamValue::Num(n) => {
            if (n.fract()).abs() < 1e-9 {
                format!("{}", *n as i64)
            } else {
                format!("{:.4}", n)
            }
        }
        ParamValue::Enum(s) => s.clone(),
        ParamValue::Bool(b) => b.to_string(),
    }
}

/// Sanitize a preset name into a safe filename stem.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "preset".to_string()
    } else {
        trimmed.to_string()
    }
}
