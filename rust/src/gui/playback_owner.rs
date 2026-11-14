//! Workspace ownership gates both players independently of transport/job state.
use super::app::MasterCraftApp;

impl MasterCraftApp {
    pub(super) fn enforce_playback_owner(&self) {
        // Close the outgoing gate first, including any fade or late job resume.
        // Pausing through the gate preserves the inactive timeline and seek.
        if self.tab == 3 {
            self.player.set_audible(false);
            self.studio.player.set_audible(true);
        } else {
            self.studio.player.set_audible(false);
            self.player.set_audible(true);
        }
    }

    pub(super) fn activate_workspace(&mut self, tab: usize) {
        self.tab = tab;
        self.enforce_playback_owner();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audio::Audio,
        gui::{
            guide_content::guide,
            player::Player,
            studio::state::Revision,
            test_fixture, theme,
            ui_test_input::{click, frame, settled},
        },
        studio::stem_mix::MixSettings,
    };
    use eframe::egui;
    use std::{
        path::Path,
        sync::{Arc, atomic::Ordering},
    };

    const VIEWPORT: egui::Vec2 = egui::vec2(1100., 720.);

    fn assert_owner(app: &MasterCraftApp, studio: bool) {
        assert_eq!(app.player.controls.audible.load(Ordering::Relaxed), !studio);
        assert_eq!(
            app.studio.player.controls.audible.load(Ordering::Relaxed),
            studio
        );
        let inactive = if studio {
            &app.player
        } else {
            &app.studio.player
        };
        assert!(!inactive.controls.playing.load(Ordering::Relaxed));
    }

    fn park(player: &Player, frame: u64) {
        player.controls.position.store(frame, Ordering::Relaxed);
        player.controls.seek.store(u64::MAX, Ordering::Relaxed);
    }

    fn timeline(player: &Player) -> (u64, u64) {
        (
            player.controls.position.load(Ordering::Relaxed),
            player.controls.seek.load(Ordering::Relaxed),
        )
    }

    fn load_studio(app: &mut MasterCraftApp) {
        let audio = Arc::new(Audio::new(vec![0.02; 24000], 48000, 1).unwrap());
        app.studio.accept(
            Revision {
                before: audio.clone(),
                audio,
                label: "Ownership fixture".into(),
                notes: vec![],
                before_lufs: -20.,
                after_lufs: -20.,
                bank: None,
                mix: MixSettings::default(),
            },
            false,
        );
    }

    #[test]
    fn activation_pauses_the_loser_without_seeking_or_automatically_resuming() {
        let ctx = egui::Context::default();
        let mut app = test_fixture::app(&ctx);
        park(&app.player, 1200);
        park(&app.studio.player, 2400);
        app.player.controls.seek.store(1800, Ordering::Relaxed);
        app.player.toggle();
        app.activate_workspace(3);
        assert_owner(&app, true);
        assert_eq!(timeline(&app.player), (1200, 1800));
        assert_eq!(timeline(&app.studio.player), (2400, u64::MAX));
        assert!(!app.studio.player.controls.playing.load(Ordering::Relaxed));

        app.studio.player.toggle();
        app.activate_workspace(0);
        assert_owner(&app, false);
        assert!(!app.player.controls.playing.load(Ordering::Relaxed));
        assert_eq!(timeline(&app.player), (1200, 1800));
        assert_eq!(timeline(&app.studio.player), (2400, u64::MAX));
    }

    #[test]
    fn moving_between_main_views_does_not_interrupt_the_main_transport() {
        let ctx = egui::Context::default();
        let mut app = test_fixture::app(&ctx);
        park(&app.player, 1200);
        app.player.toggle();
        for tab in [0, 1, 2, 0] {
            app.activate_workspace(tab);
            assert_owner(&app, false);
            assert!(app.player.controls.playing.load(Ordering::Relaxed));
            assert_eq!(timeline(&app.player), (1200, u64::MAX));
        }
    }

    #[test]
    fn real_tabs_and_play_buttons_transfer_ownership_without_resetting_positions() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = test_fixture::mastered(&ctx);
        app.activate_workspace(0);
        load_studio(&mut app);
        let output = settled(&ctx, &mut app, VIEWPORT);
        park(&app.player, 1200);
        park(&app.studio.player, 2400);
        app.studio.player.toggle();
        let output = click(&ctx, &mut app, VIEWPORT, &output, "Play");
        assert_owner(&app, false);
        assert!(app.player.controls.playing.load(Ordering::Relaxed));
        let output = click(&ctx, &mut app, VIEWPORT, &output, "Studio");
        assert_owner(&app, true);
        assert_eq!(timeline(&app.player), (1200, u64::MAX));
        let output = click(&ctx, &mut app, VIEWPORT, &output, "Play");
        assert!(app.studio.player.controls.playing.load(Ordering::Relaxed));
        click(&ctx, &mut app, VIEWPORT, &output, "Overview");
        assert_owner(&app, false);
        assert_eq!(timeline(&app.studio.player), (2400, u64::MAX));
        assert!(!app.player.controls.playing.load(Ordering::Relaxed));
    }

    #[derive(Debug)]
    struct DroppedArchive(&'static str);

    impl egui::DroppedFile for DroppedArchive {
        fn path(&self) -> &Path {
            Path::new(self.0)
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            panic!("Ownership regression must not read archive contents")
        }
    }

    #[test]
    fn zip_drop_transfers_ownership_before_space_starts_studio_in_the_same_frame() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = test_fixture::app(&ctx);
        app.activate_workspace(0);
        load_studio(&mut app);
        settled(&ctx, &mut app, VIEWPORT);
        park(&app.player, 1200);
        app.player.toggle();
        // Multiple archives take the real activation path without starting import I/O.
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT)),
                dropped_files: vec![
                    Arc::new(DroppedArchive("first.zip")),
                    Arc::new(DroppedArchive("second.zip")),
                ],
                events: vec![egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| app.render_frame(ui),
        );
        output.textures_delta.clear();
        assert_eq!(app.tab, 3);
        assert_owner(&app, true);
        assert!(app.studio.player.controls.playing.load(Ordering::Relaxed));
        assert_eq!(timeline(&app.player), (1200, u64::MAX));
        assert!(!app.studio.busy());
    }

    #[test]
    fn tutorial_screen_navigation_gates_the_previous_owner_in_both_directions() {
        for (target, previous_tab, next_tab) in [("stems", 0, 3), ("eq", 3, 1), ("overview", 3, 0)]
        {
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            let mut app = test_fixture::app(&ctx);
            app.activate_workspace(previous_tab);
            park(&app.player, 1200);
            park(&app.studio.player, 2400);
            if previous_tab == 3 {
                app.studio.player.toggle();
            } else {
                app.player.toggle();
            }
            app.experience.start_tour();
            app.experience.preferences.tutorial_step = guide()
                .steps
                .iter()
                .position(|step| step.desktop_target == target)
                .unwrap();
            let output = settled(&ctx, &mut app, VIEWPORT);
            click(&ctx, &mut app, VIEWPORT, &output, "See this screen");
            assert_eq!(app.tab, next_tab);
            assert_owner(&app, next_tab == 3);
            assert_eq!(timeline(&app.player), (1200, u64::MAX));
            assert_eq!(timeline(&app.studio.player), (2400, u64::MAX));
        }
    }

    #[test]
    fn hidden_player_reload_and_resume_keeps_its_gate_closed_and_is_paused_next_frame() {
        for tab in [0, 3] {
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            let mut app = test_fixture::app(&ctx);
            app.activate_workspace(tab);
            let hidden = if tab == 3 {
                &mut app.player
            } else {
                &mut app.studio.player
            };
            let audio = Arc::new(Audio::new(vec![0.02; 24000], 48000, 1).unwrap());
            hidden
                .load(audio.clone(), Some(audio), -20., -20., None)
                .unwrap();
            park(hidden, 2400);
            hidden.toggle();
            assert!(
                !hidden.controls.audible.load(Ordering::Relaxed),
                "Reload must retain the ownership gate"
            );
            frame(&ctx, &mut app, VIEWPORT, vec![]);
            assert_owner(&app, tab == 3);
            let hidden = if tab == 3 {
                &app.player
            } else {
                &app.studio.player
            };
            assert_eq!(timeline(hidden), (2400, u64::MAX));
        }
    }

    #[test]
    fn returning_to_a_late_resumed_hidden_player_requires_explicit_play() {
        for previous_tab in [0, 3] {
            let ctx = egui::Context::default();
            let mut app = test_fixture::app(&ctx);
            app.activate_workspace(previous_tab);
            park(&app.player, 1200);
            park(&app.studio.player, 2400);
            app.player.controls.playing.store(true, Ordering::Relaxed);
            app.studio
                .player
                .controls
                .playing
                .store(true, Ordering::Relaxed);
            let next_tab = if previous_tab == 3 { 0 } else { 3 };
            let hidden = if next_tab == 3 {
                &app.studio.player
            } else {
                &app.player
            };
            assert!(!hidden.controls.audible.load(Ordering::Relaxed));

            // Return immediately, before a frame can clear the hidden late-job flag.
            app.activate_workspace(next_tab);
            assert_owner(&app, next_tab == 3);
            assert_eq!(timeline(&app.player), (1200, u64::MAX));
            assert_eq!(timeline(&app.studio.player), (2400, u64::MAX));
            let active = if next_tab == 3 {
                &app.studio.player
            } else {
                &app.player
            };
            assert!(
                !active.controls.playing.load(Ordering::Relaxed),
                "Opening a gate must not resume a late job"
            );
            active.toggle();
            app.enforce_playback_owner();
            assert!(
                active.controls.playing.load(Ordering::Relaxed),
                "Subsequent ownership checks must preserve explicit Play"
            );
        }
    }

    #[test]
    fn a_late_mastering_result_cannot_reclaim_playback_from_studio() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = test_fixture::mastered(&ctx);
        load_studio(&mut app);
        app.activate_workspace(3);
        app.studio.player.toggle();
        // The mastered fixture still has its real Ready/Finished events queued.
        app.player.toggle();
        frame(&ctx, &mut app, VIEWPORT, vec![]);
        assert_eq!(app.loaded, Some(0));
        assert!(app.tracks[0].result.is_some());
        assert_owner(&app, true);
        assert!(app.studio.player.controls.playing.load(Ordering::Relaxed));
    }
}
