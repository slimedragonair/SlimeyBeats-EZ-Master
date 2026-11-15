mod app;
mod audio_source;
mod brand_art;
mod bundled_resources;
mod eq_band_controls;
mod eq_curve;
mod eq_jobs;
mod eq_live_control;
mod eq_panel;
mod guide_content;
mod help;
mod help_center;
mod interface_preferences;
mod job_events;
#[cfg(test)]
mod keyboard_interaction_tests;
#[cfg(test)]
mod learning_interaction_tests;
mod learning_ui;
mod live_spectrum;
mod live_stem_path;
#[cfg(test)]
mod live_stem_tests;
mod mastering_controls;
mod mastering_preview_state;
mod mcp_setup;
mod overview;
mod playback_owner;
mod playback_tap;
mod player;
mod plots;
mod recovery_capture;
mod recovery_control;
#[cfg(test)]
mod recovery_controller_tests;
mod recovery_dialog;
mod recovery_document;
mod recovery_eq;
mod recovery_key;
mod recovery_preferences;
mod recovery_restore;
#[cfg(test)]
mod recovery_snapshot_tests;
mod recovery_state;
mod recovery_view;
mod recovery_worker;
mod settings_panel;
pub mod studio;
mod support_report_state;
#[cfg(test)]
mod support_report_tests;
mod support_report_view;
#[cfg(test)]
mod test_fixture;
mod theme;
mod tool_setup;
mod update_launch;
mod update_state;
mod update_view;
#[cfg(test)]
mod update_view_tests;
mod worker;
pub use app::MasterCraftApp;
pub use brand_art::window_icon;
mod control_layout;
mod crystal_button;
mod crystal_slider;
mod desktop_preferences;
mod preference_store;
mod queue_panel;
mod shell;
mod skin_frame;
mod skin_icons;
mod slime_border;
mod timeline_panel;
mod transport;
mod tutorial_history;
mod ui_capture;
#[cfg(test)]
mod ui_test_input;
mod user_experience;
mod welcome_panel;
mod workspace_tabs;
