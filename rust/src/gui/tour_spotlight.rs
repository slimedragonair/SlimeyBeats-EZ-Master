//! Visual-only quick tour cues from real widget geometry, not screen coordinates.
use super::{app::MasterCraftApp, guide_content::guide};
use eframe::egui::{self, Align2, Color32, Id, Painter, Rect, Stroke, StrokeKind, pos2, vec2};
use std::{sync::atomic::Ordering, time::Duration};

#[derive(Clone, Copy, Debug, Hash)]
pub(super) enum Target {
    Add,
    Analyze,
    Master,
    Compare,
    Eq,
    Export,
    StemFiles,
    StemsTab,
    Help,
    OverviewTab,
    EqTab,
    StudioTab,
}

#[derive(Clone, Copy)]
pub(super) struct Cue {
    pub rect: Rect,
    pub label: &'static str,
    pub enabled: bool,
}

const TEAL: Color32 = Color32::from_rgb(103, 255, 221);
const PULSE_SECONDS: f64 = 1.25;
const EDGE: f32 = 18.;
const CLEARANCE: f32 = 14.;

pub(super) fn step_index(app: &MasterCraftApp) -> usize {
    app.experience
        .preferences
        .tutorial_step
        .min(guide().steps.len() - 1)
}

pub(super) fn workspace(target: &str) -> usize {
    match target {
        "eq" => 1,
        "stems" => 3,
        _ => 0,
    }
}

/// Called before laying out the actual controls. Navigate only once per step,
/// and never automatically change the audition workspace during playback/work.
pub(super) fn prepare(app: &mut MasterCraftApp, ctx: &egui::Context) {
    let key = Id::unique("quick-tour-entry");
    if !app.experience.tour_open {
        ctx.data_mut(|data| data.remove::<(usize, f64)>(key));
        return;
    }
    let index = step_index(app);
    if ctx
        .data_mut(|data| data.get_temp::<(usize, f64)>(key))
        .is_some_and(|(old, _)| old == index)
    {
        return;
    }
    let now = ctx.input(|input| input.time);
    ctx.data_mut(|data| data.insert_temp(key, (index, now)));
    let step = &guide().steps[index];
    if step.id != "help"
        && !app.busy
        && !app.studio.busy()
        && !app.recovery.offer
        && !app.player.controls.playing.load(Ordering::Relaxed)
        && !app.studio.player.controls.playing.load(Ordering::Relaxed)
    {
        let tab = workspace(&step.desktop_target);
        if tab != app.tab {
            app.activate_workspace(tab);
        }
    }
}

pub(super) fn record(
    ui: &egui::Ui,
    target: Target,
    label: &'static str,
    response: &egui::Response,
) {
    record_rect(ui, target, label, response.rect, response.enabled());
}

pub(super) fn record_rect(
    ui: &egui::Ui,
    target: Target,
    label: &'static str,
    rect: Rect,
    enabled: bool,
) {
    if !ui.is_visible()
        || !rect.is_finite()
        || !rect.is_positive()
        || !ui.clip_rect().expand(0.5).contains_rect(rect)
    {
        return;
    }
    let ctx = ui.ctx();
    let pass = ctx.cumulative_pass_nr();
    let cue = Cue {
        rect,
        label,
        enabled,
    };
    ctx.data_mut(|data| data.insert_temp(Id::unique(("quick-tour-target", target)), (pass, cue)));
}

pub(super) fn cue(app: &MasterCraftApp, ctx: &egui::Context, step: &str) -> Option<Cue> {
    use Target::*;
    let candidates: &[Target] = match step {
        "add" => &[Add, OverviewTab],
        "analyze" if app.tab != 3 => &[Analyze, OverviewTab],
        "analyze" => &[OverviewTab],
        "master" => &[Master, OverviewTab],
        "compare" => &[Compare, OverviewTab],
        "eq" => &[Eq, EqTab],
        "export" => &[Export, OverviewTab],
        "studio" => &[StemFiles, StemsTab, StudioTab],
        "help" => &[Help],
        _ => &[],
    };
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|data| {
        candidates.iter().find_map(|target| {
            let (seen, cue) =
                data.get_temp::<(u64, Cue)>(Id::unique(("quick-tour-target", target)))?;
            (seen == pass).then_some(cue)
        })
    })
}

/// Park the card against the viewport edge with more clear space. The bounded
/// body scroll keeps the navigation buttons outside the scroll area.
pub(super) fn card_position(screen: Rect, cue: Option<Cue>) -> (Align2, egui::Vec2, f32) {
    if let Some(cue) = cue {
        let above = cue.rect.top() - screen.top() - EDGE - CLEARANCE;
        let below = screen.bottom() - cue.rect.bottom() - EDGE - CLEARANCE;
        if above.max(below) >= 205. {
            return if above > below {
                (Align2::CENTER_TOP, vec2(0., EDGE), above)
            } else {
                (Align2::CENTER_BOTTOM, vec2(0., -EDGE), below)
            };
        }
    }
    (
        Align2::CENTER_CENTER,
        egui::Vec2::ZERO,
        screen.height() - 2. * EDGE,
    )
}

pub(super) fn paint(painter: &Painter, screen: Rect, cue: Option<Cue>, index: usize) {
    let shade = Color32::from_black_alpha(88);
    let Some(cue) = cue else {
        painter.rect_filled(screen, 0., shade);
        return;
    };
    let hole = cue.rect.expand(5.).intersect(screen);
    for rect in [
        Rect::from_min_max(screen.min, pos2(screen.right(), hole.top())),
        Rect::from_min_max(pos2(screen.left(), hole.bottom()), screen.max),
        Rect::from_min_max(pos2(screen.left(), hole.top()), hole.left_bottom()),
        Rect::from_min_max(hole.right_top(), pos2(screen.right(), hole.bottom())),
    ] {
        if rect.is_positive() {
            painter.rect_filled(rect, 0., shade);
        }
    }
    let ctx = painter.ctx();
    let entry = ctx.data_mut(|data| data.get_temp::<(usize, f64)>(Id::unique("quick-tour-entry")));
    let elapsed = entry
        .filter(|(step, _)| *step == index)
        .map(|(_, start)| ctx.input(|input| input.time) - start);
    let glow = match elapsed {
        Some(t)
            if (0.0..PULSE_SECONDS).contains(&t)
                && ctx.style_of(egui::Theme::Dark).animation_time > 0. =>
        {
            ctx.request_repaint_after(Duration::from_millis(16));
            // A single soft swell; never flash or move the control.
            (std::f64::consts::PI * t / PULSE_SECONDS).sin().powi(2) as f32
        }
        _ => 0.,
    };
    for (padding, opacity) in [(8., 0.10), (6., 0.21), (4., 0.36)] {
        painter.rect_stroke(
            cue.rect.expand(padding),
            8.,
            Stroke::new(2., TEAL.gamma_multiply(opacity * (1. + 0.8 * glow))),
            StrokeKind::Outside,
        );
    }
    painter.rect_stroke(
        cue.rect.expand(2.),
        7.,
        Stroke::new(2., TEAL),
        StrokeKind::Outside,
    );
}
