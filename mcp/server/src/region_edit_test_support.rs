//! Deterministic stereo bank shared by the region adapter and project regressions.
use crate::{
    arguments::Arguments,
    jobs::{Change, ContextData, Product},
    state::Engine,
    studio_state::{AudioTarget, EditedAudio, StemBank, StudioState},
};
use mastercraft::{
    audio::Audio,
    studio::stem_mix::{self, Stem, StemKind, StemSet},
};
use serde_json::json;
use std::sync::{Arc, atomic::AtomicBool};

pub(crate) struct Workspace {
    pub root: tempfile::TempDir,
    pub engine: Arc<Engine>,
    pub bank: Arc<StemBank>,
}

impl Workspace {
    pub fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::new(&Arguments {
            input_root: vec![root.path().into()],
            output_root: root.path().join("exports"),
            max_decoded_mib: 8,
            decode_timeout_seconds: 5,
            desktop_exe: None,
            profiles_root: None,
            studio_data: Some(root.path().join("unused-runtime")),
            plugin_root: vec![],
            tool_profile: crate::tool_catalog::ToolProfile::Compact,
            print_config: false,
        })
        .unwrap();
        let mut stems = StemSet::new(
            [
                StemKind::Vocals,
                StemKind::part(64).unwrap(),
                StemKind::Drums,
            ]
            .into_iter()
            .enumerate()
            .map(|(index, kind)| Stem {
                kind,
                audio: Arc::new(
                    Audio::new(
                        (0..3200)
                            .map(|sample| {
                                if sample == 0 {
                                    -0.0
                                } else {
                                    ((sample % (17 + index)) as f32 - 8.)
                                        / (512. * (index + 1) as f32)
                                }
                            })
                            .collect(),
                        8000,
                        2,
                    )
                    .unwrap(),
                ),
            })
            .collect(),
        )
        .unwrap();
        stems
            .set_display_name(StemKind::part(64).unwrap(), "Backing vocal take")
            .unwrap();
        let mut neutral = stems.defaults();
        neutral.protect_peaks = false;
        let original_mix = Arc::new(stem_mix::render(&stems, &neutral, &|| false).unwrap());
        let mut controls = neutral;
        controls.tracks[1].gain_db = -6.;
        controls.tracks[1].balance = 0.25;
        controls.tracks[2].mute = true;
        controls.output_gain_db = -2.;
        let mixed = Arc::new(stem_mix::render(&stems, &controls, &|| false).unwrap());
        let bank = Arc::new(StemBank {
            id: "stems-baseline".into(),
            source_name: "Original imported song.wav".into(),
            stems,
            controls,
            model: json!({"origin":"fixture", "source_sha256":"unchanged-provenance"}),
            original_mix,
        });
        let origin = AudioTarget::bank_mix(&bank, "Fixture mix").origin;
        engine.lock().unwrap().session.studio = StudioState {
            origin: Some(origin.clone()),
            bank: Some(bank.clone()),
            current: Some(Arc::new(EditedAudio {
                id: "studio-baseline".into(),
                label: "Fixture mix".into(),
                audio: mixed,
                before: bank.original_mix.clone(),
                file: None,
                steps: vec![json!({"operation":"import_stems","fixture_provenance":true})],
                notes: vec![],
                origin_snapshot: origin,
                bank_snapshot: Some(bank.clone()),
            })),
            ..Default::default()
        };
        Self { root, engine, bank }
    }

    pub fn context(&self, id: &str) -> ContextData {
        ContextData {
            engine: self.engine.clone(),
            id: id.into(),
            snapshot: self.engine.lock().unwrap().session.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn commit(&self, product: Product) {
        let Change::Studio(studio) = product.change else {
            panic!("Expected a Studio change")
        };
        self.engine.lock().unwrap().session.studio = studio;
    }
}
