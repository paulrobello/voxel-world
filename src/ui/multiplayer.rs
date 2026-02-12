//! Multiplayer UI panels for hosting and joining games.
//!
//! Provides a tabbed interface with:
//! - Host tab: Configure and start a server
//! - Join tab: Direct connect or discover LAN servers
//! - Connection status overlay when connected
//! - Player list panel (Tab key when connected)

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use egui_winit_vulkano::egui::{self, Color32, RichText};

use crate::config::GameMode;
use crate::net::DiscoveredServer;

/// Multiplayer panel tab selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MultiplayerTab {
    /// Host a server.
    #[default]
    Host,
    /// Join a server.
    Join,
}

/// State for the host panel tab.
#[derive(Debug, Clone)]
pub struct HostPanelState {
    /// Server name displayed in LAN discovery.
    pub server_name: String,
    /// Port text input.
    pub port: String,
    /// Hosting status message.
    pub status_message: Option<String>,
    /// Whether currently hosting.
    pub is_hosting: bool,
}

impl Default for HostPanelState {
    fn default() -> Self {
        Self {
            server_name: "Voxel World Server".to_string(),
            port: "5000".to_string(),
            status_message: None,
            is_hosting: false,
        }
    }
}

/// State for the join panel tab.
#[derive(Debug, Clone, Default)]
pub struct JoinPanelState {
    /// Direct connect address input.
    pub address_input: String,
    /// List of discovered servers from LAN discovery.
    pub discovered_servers: Vec<DiscoveredServer>,
    /// Selected server index in the discovered list.
    pub selected_server: Option<usize>,
    /// Whether LAN discovery is active.
    pub discovery_active: bool,
    /// Connection status message.
    pub status_message: Option<String>,
}

/// Main multiplayer panel state.
#[derive(Debug, Clone, Default)]
pub struct MultiplayerPanelState {
    /// Whether the panel is open.
    pub open: bool,
    /// Currently selected tab.
    pub tab: MultiplayerTab,
    /// Whether we were focused before opening.
    pub previously_focused: bool,
    /// Host panel state.
    pub host: HostPanelState,
    /// Join panel state.
    pub join: JoinPanelState,
    /// Show player list overlay (Tab key).
    pub show_player_list: bool,
}

/// Result of multiplayer panel interactions.
#[derive(Debug, Clone, Default)]
pub struct MultiplayerAction {
    /// Request to start hosting with (server_name, port).
    pub start_hosting: Option<(String, u16)>,
    /// Request to stop hosting.
    pub stop_hosting: bool,
    /// Request to connect to address.
    pub connect: Option<SocketAddr>,
    /// Request to disconnect.
    pub disconnect: bool,
    /// Request to start LAN discovery.
    pub start_discovery: bool,
    /// Request to stop LAN discovery.
    pub stop_discovery: bool,
}

impl MultiplayerAction {
    #[allow(dead_code)]
    pub fn none() -> Self {
        Self::default()
    }
}

/// Multiplayer UI renderer.
pub struct MultiplayerUI;

impl MultiplayerUI {
    /// Draws the multiplayer panel.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        ctx: &egui::Context,
        state: &mut MultiplayerPanelState,
        game_mode: GameMode,
        is_connected: bool,
        player_count: u8,
        max_players: u8,
        server_address: Option<SocketAddr>,
        ping_ms: Option<u32>,
        player_names: &[String],
    ) -> MultiplayerAction {
        let mut action = MultiplayerAction::default();

        if !state.open {
            return action;
        }

        // Update internal hosting state from game mode
        state.host.is_hosting = game_mode == GameMode::Host;

        egui::Window::new("Multiplayer")
            .collapsible(false)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                // Tab selector
                ui.horizontal(|ui| {
                    let host_text = if state.host.is_hosting {
                        "Hosting..."
                    } else {
                        "Host"
                    };
                    if ui
                        .selectable_label(state.tab == MultiplayerTab::Host, host_text)
                        .clicked()
                    {
                        state.tab = MultiplayerTab::Host;
                    }
                    if ui
                        .selectable_label(state.tab == MultiplayerTab::Join, "Join")
                        .clicked()
                    {
                        state.tab = MultiplayerTab::Join;
                    }
                });

                ui.separator();

                match state.tab {
                    MultiplayerTab::Host => {
                        Self::draw_host_tab(ui, state, game_mode, &mut action);
                    }
                    MultiplayerTab::Join => {
                        Self::draw_join_tab(ui, state, is_connected, &mut action);
                    }
                }
            });

        // Draw connection status overlay if connected
        if is_connected || game_mode == GameMode::Host {
            Self::draw_connection_status(
                ctx,
                state,
                game_mode,
                player_count,
                max_players,
                server_address,
                ping_ms,
            );
        }

        // Draw player list overlay (Tab key toggles this separately)
        if state.show_player_list && (is_connected || game_mode == GameMode::Host) {
            Self::draw_player_list(ctx, player_names, player_count, max_players);
        }

        action
    }

    /// Draws the host tab.
    fn draw_host_tab(
        ui: &mut egui::Ui,
        state: &mut MultiplayerPanelState,
        game_mode: GameMode,
        action: &mut MultiplayerAction,
    ) {
        let host = &mut state.host;

        if host.is_hosting {
            // Currently hosting - show status and stop button
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Server Running")
                        .color(Color32::from_rgb(100, 255, 100))
                        .size(16.0),
                );

                if let Some(ref msg) = host.status_message {
                    ui.label(RichText::new(msg).color(Color32::from_gray(200)).size(12.0));
                }

                ui.add_space(8.0);

                if ui.button("Stop Hosting").clicked() {
                    action.stop_hosting = true;
                    host.is_hosting = false;
                    host.status_message = None;
                }
            });
        } else {
            // Configuration UI
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Server Name:");
                    ui.text_edit_singleline(&mut host.server_name);
                });

                ui.horizontal(|ui| {
                    ui.label("Port:");
                    ui.add(egui::TextEdit::singleline(&mut host.port).desired_width(80.0));
                });

                ui.add_space(8.0);

                // Validate port
                let port_valid = host.port.parse::<u16>().is_ok();

                if !port_valid {
                    ui.label(
                        RichText::new("Invalid port number")
                            .color(Color32::from_rgb(255, 100, 100)),
                    );
                }

                if ui
                    .add_enabled(port_valid, egui::Button::new("Start Hosting"))
                    .clicked()
                {
                    if let Ok(port) = host.port.parse::<u16>() {
                        action.start_hosting = Some((host.server_name.clone(), port));
                        host.is_hosting = true;
                        host.status_message = Some(format!("Hosting on port {}", port));
                    }
                }

                if game_mode == GameMode::Client {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Note: Cannot host while connected to another server")
                            .color(Color32::from_rgb(255, 200, 100))
                            .size(11.0),
                    );
                }
            });
        }
    }

    /// Draws the join tab.
    fn draw_join_tab(
        ui: &mut egui::Ui,
        state: &mut MultiplayerPanelState,
        is_connected: bool,
        action: &mut MultiplayerAction,
    ) {
        let join = &mut state.join;

        ui.vertical(|ui| {
            // Direct connect section
            ui.label(RichText::new("Direct Connect").strong());
            ui.horizontal(|ui| {
                ui.label("Address:");
                ui.add(
                    egui::TextEdit::singleline(&mut join.address_input)
                        .desired_width(200.0)
                        .hint_text("127.0.0.1:5000"),
                );
            });

            if is_connected {
                if ui.button("Disconnect").clicked() {
                    action.disconnect = true;
                }
            } else {
                let address_valid = join.address_input.parse::<SocketAddr>().is_ok()
                    || Self::parse_simple_address(&join.address_input).is_some();

                if ui
                    .add_enabled(address_valid, egui::Button::new("Connect"))
                    .clicked()
                {
                    if let Ok(addr) = join.address_input.parse::<SocketAddr>() {
                        action.connect = Some(addr);
                    } else if let Some(addr) = Self::parse_simple_address(&join.address_input) {
                        action.connect = Some(addr);
                    }
                }
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // LAN Discovery section
            ui.label(RichText::new("LAN Discovery").strong());

            if join.discovery_active {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Scanning for servers...");
                });

                if ui.button("Stop Scanning").clicked() {
                    action.stop_discovery = true;
                    join.discovery_active = false;
                }
            } else if ui.button("Scan for Servers").clicked() {
                action.start_discovery = true;
                join.discovery_active = true;
                join.discovered_servers.clear();
            }

            // Discovered servers list
            if !join.discovered_servers.is_empty() {
                ui.add_space(8.0);
                ui.label(format!(
                    "Found {} server(s):",
                    join.discovered_servers.len()
                ));

                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for (i, server) in join.discovered_servers.iter().enumerate() {
                            let selected = join.selected_server == Some(i);
                            let response = ui.selectable_label(
                                selected,
                                format!(
                                    "{} ({} players) - {}:{}",
                                    server.server_name,
                                    server.player_count,
                                    server.address.ip(),
                                    server.game_port
                                ),
                            );

                            if response.clicked() {
                                join.selected_server = Some(i);
                            }

                            if response.double_clicked() && !is_connected {
                                action.connect = Some(server.address);
                            }
                        }
                    });

                if let Some(idx) = join.selected_server {
                    if ui.button("Join Selected").clicked() && !is_connected {
                        action.connect = Some(join.discovered_servers[idx].address);
                    }
                }
            } else if join.discovery_active {
                ui.label(
                    RichText::new("No servers found yet...")
                        .color(Color32::from_gray(150))
                        .italics(),
                );
            }

            if let Some(ref msg) = join.status_message {
                ui.add_space(8.0);
                ui.label(RichText::new(msg).color(Color32::from_rgb(255, 200, 100)));
            }
        });
    }

    /// Draws the connection status overlay (top-right corner).
    fn draw_connection_status(
        ctx: &egui::Context,
        _state: &MultiplayerPanelState,
        game_mode: GameMode,
        player_count: u8,
        max_players: u8,
        server_address: Option<SocketAddr>,
        ping_ms: Option<u32>,
    ) {
        let _screen_rect = ctx.screen_rect();

        egui::Area::new(egui::Id::new("connection_status"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(20, 20, 30, 220))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(100, 200, 100)))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Connection type icon
                            let (icon, color) = match game_mode {
                                GameMode::Host => ("🖥 Hosting", Color32::from_rgb(100, 200, 255)),
                                GameMode::Client => {
                                    ("🔗 Connected", Color32::from_rgb(100, 255, 100))
                                }
                                GameMode::SinglePlayer => ("", Color32::GRAY),
                            };

                            if !icon.is_empty() {
                                ui.label(RichText::new(icon).color(color).size(12.0));
                            }

                            // Server address
                            if let Some(addr) = server_address {
                                ui.label(
                                    RichText::new(format!("{}:{}", addr.ip(), addr.port()))
                                        .color(Color32::from_gray(200))
                                        .size(11.0),
                                );
                            }

                            // Player count
                            ui.label(
                                RichText::new(format!("👥 {}/{}", player_count, max_players))
                                    .color(Color32::from_gray(200))
                                    .size(11.0),
                            );

                            // Ping
                            if let Some(ping) = ping_ms {
                                let ping_color = if ping < 50 {
                                    Color32::from_rgb(100, 255, 100)
                                } else if ping < 150 {
                                    Color32::from_rgb(255, 200, 100)
                                } else {
                                    Color32::from_rgb(255, 100, 100)
                                };
                                ui.label(
                                    RichText::new(format!("⏱ {}ms", ping))
                                        .color(ping_color)
                                        .size(11.0),
                                );
                            }

                            // Hint for player list
                            ui.label(
                                RichText::new("(Tab for players)")
                                    .color(Color32::from_gray(120))
                                    .size(10.0),
                            );
                        });
                    });
            });
    }

    /// Draws the player list overlay.
    fn draw_player_list(
        ctx: &egui::Context,
        player_names: &[String],
        player_count: u8,
        max_players: u8,
    ) {
        let _screen_rect = ctx.screen_rect();

        egui::Area::new(egui::Id::new("player_list"))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(20, 20, 30, 240))
                    .stroke(egui::Stroke::new(2.0, Color32::from_rgb(100, 150, 200)))
                    .inner_margin(12.0)
                    .corner_radius(6.0)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Players ({}/{})",
                                    player_count, max_players
                                ))
                                .color(Color32::from_rgb(100, 200, 255))
                                .size(16.0)
                                .strong(),
                            );

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            if player_names.is_empty() {
                                ui.label(
                                    RichText::new("No players connected")
                                        .color(Color32::from_gray(150))
                                        .italics(),
                                );
                            } else {
                                for (i, name) in player_names.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!("{}.", i + 1))
                                                .color(Color32::from_gray(150)),
                                        );
                                        ui.label(RichText::new(name).color(Color32::WHITE));
                                    });
                                }
                            }

                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("Press Tab to close")
                                    .color(Color32::from_gray(120))
                                    .size(11.0),
                            );
                        });
                    });
            });
    }

    /// Parses a simple address like "127.0.0.1:5000" or "localhost:5000".
    fn parse_simple_address(input: &str) -> Option<SocketAddr> {
        let input = input.trim();

        // Handle localhost
        if let Some(stripped) = input.strip_prefix("localhost:") {
            let port: u16 = stripped.parse().ok()?;
            return Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port,
            ));
        }

        // Handle just port number (assume localhost)
        if let Ok(port) = input.parse::<u16>() {
            return Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port,
            ));
        }

        None
    }
}
