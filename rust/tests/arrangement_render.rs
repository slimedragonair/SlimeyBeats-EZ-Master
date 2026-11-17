use mastercraft::{audio::Audio, config::default_eq, studio::arrangement::*};
use std::{cell::Cell, f64::consts::TAU, sync::Arc};

fn options() -> RenderOptions {
    RenderOptions {
        protect_peaks: false,
        ..Default::default()
    }
}
fn clip(id: &str, source: &str, start: u64, source_start: u64, length: u64) -> Clip {
    Clip {
        id: id.into(),
        name: None,
        source_id: source.into(),
        start_frame: start,
        source_start_frame: source_start,
        end_frame: start + length,
        gain_db: 0.,
        fade_in_frames: 0,
        fade_out_frames: 0,
    }
}
fn fixture(samples: Vec<f32>, rate: u32, channels: usize) -> (Arrangement, AssetLibrary) {
    let frames = samples.len() / channels;
    let assets = AssetLibrary::from([(
        "source".into(),
        Arc::new(Audio::new(samples, rate, channels).unwrap()),
    )]);
    let mut document = Arrangement::new(rate, channels);
    let mut lane = Lane::new("lane", "Part");
    lane.clips.push(clip("clip", "source", 0, 0, frames as u64));
    document.lanes.push(lane);
    (document, assets)
}
fn sine(rate: u32, frames: usize, frequency: f64, gain: f64) -> Vec<f32> {
    (0..frames)
        .map(|i| (gain * (TAU * frequency * i as f64 / rate as f64).sin()) as f32)
        .collect()
}
fn bits(samples: &[f32]) -> Vec<u32> {
    samples.iter().map(|sample| sample.to_bits()).collect()
}
fn output_gain(id: &str, scope: EqScope, range: Option<FrameRange>, gain: f64) -> ScopedEq {
    let mut effect = ScopedEq::new(id, scope);
    effect.range = range;
    effect.output_gain_db = gain;
    for band in &mut effect.bands {
        band.on = false;
    }
    effect
}

#[test]
fn neutral_pcm_including_negative_zero_is_bit_exact_and_original_is_immutable() {
    for channels in [1, 2] {
        let samples = vec![-0., 0., 0.125, -0.25, 0.75, -0.5, 0.03125, -0.0625];
        let (document, assets) = fixture(samples.clone(), 8000, channels);
        let before = bits(&assets["source"].samples);
        let rendered = render(&document, &assets, &options(), &|| false).unwrap();
        assert_eq!(bits(&rendered.samples), bits(&samples));
        assert_eq!(before, bits(&assets["source"].samples));
        assert_eq!(rendered.frames(), samples.len() / channels);
    }
}

#[test]
fn differing_source_lengths_offsets_overlap_and_mono_upmix_have_known_samples() {
    let assets = AssetLibrary::from([
        (
            "a".into(),
            Arc::new(Audio::new(vec![0.125, 0.25, 0.5, 0.75], 8000, 1).unwrap()),
        ),
        (
            "b".into(),
            Arc::new(Audio::new(vec![0.0625, 0.125], 8000, 1).unwrap()),
        ),
    ]);
    let mut document = Arrangement::new(8000, 2);
    let mut lane = Lane::new("lane", "Part");
    lane.clips = vec![clip("a", "a", 1, 0, 4), clip("b", "b", 3, 0, 2)];
    document.lanes.push(lane);
    let rendered = render(&document, &assets, &options(), &|| false).unwrap();
    assert_eq!(
        rendered.samples,
        [
            0., 0., 0.125, 0.125, 0.25, 0.25, 0.5625, 0.5625, 0.875, 0.875
        ]
    );
}

#[test]
fn live_arbitrary_blocks_and_offline_render_match_with_and_without_final_peak_protection() {
    for rate in [8000, 44_100, 48_000, 96_000] {
        let (document, assets) = fixture(sine(rate, rate as usize / 8, 1300., 1.3), rate, 1);
        for protect in [false, true] {
            let opts = RenderOptions {
                protect_peaks: protect,
                ..Default::default()
            };
            let expected = render(&document, &assets, &opts, &|| false).unwrap();
            let mut player = ArrangementPlayer::new(&document, &assets, &opts).unwrap();
            let mut actual = vec![];
            for block_frames in [1, 7, 63, 128].into_iter().cycle() {
                let mut block = vec![123.; block_frames];
                let frames = player.process_block(&mut block).unwrap();
                actual.extend_from_slice(&block[..frames]);
                assert!(block[frames..].iter().all(|value| *value == 0.));
                if frames < block_frames {
                    break;
                }
            }
            assert_eq!(bits(&actual), bits(&expected.samples), "{rate}/{protect}");
            assert_eq!(player.position(), expected.frames() as u64);
            if protect {
                assert!(actual.iter().all(|value| value.abs() <= 0.9), "{rate}");
            }
        }
    }
}

#[test]
fn mix_lane_and_source_anchored_clip_eq_touch_only_the_selected_scope_and_range() {
    let (mut document, mut assets) = fixture(vec![0.2; 1000], 8000, 1);
    document.lanes[0].clips[0] = clip("clip", "source", 50, 100, 800);
    assets.insert(
        "other".into(),
        Arc::new(Audio::new(vec![0.1; 900], 8000, 1).unwrap()),
    );
    let mut other = Lane::new("other_lane", "Other");
    other.clips.push(clip("other_clip", "other", 0, 0, 900));
    document.lanes.push(other);
    let dry = render(&document, &assets, &options(), &|| false).unwrap();
    for scope in [
        EqScope::Mix {},
        EqScope::Lane {
            lane_id: "lane".into(),
        },
        EqScope::Clip {
            clip_id: "clip".into(),
        },
    ] {
        let is_mix = matches!(scope, EqScope::Mix {});
        let range = if matches!(scope, EqScope::Clip { .. }) {
            FrameRange {
                start_frame: 200,
                end_frame: 600,
            }
        } else {
            FrameRange {
                start_frame: 150,
                end_frame: 550,
            }
        };
        let mut edited = document.clone();
        edited.eq.push(output_gain("eq", scope, Some(range), -6.));
        let output = render(&edited, &assets, &options(), &|| false).unwrap();
        assert_eq!(bits(&output.samples[..150]), bits(&dry.samples[..150]));
        assert_eq!(bits(&output.samples[550..]), bits(&dry.samples[550..]));
        let gain = 10_f64.powf(-6. / 20.);
        let expected = if is_mix {
            (0.2_f32 as f64 + 0.1_f32 as f64) * gain
        } else {
            0.2_f32 as f64 * gain + 0.1_f32 as f64
        };
        assert!((output.samples[350] as f64 - expected).abs() < 1e-7);
    }
}

#[test]
fn range_eq_is_warmed_before_entry_and_split_trim_retain_canonical_source_state() {
    let (mut document, assets) = fixture(sine(8000, 3000, 500., 0.1), 8000, 1);
    let mut whole = ScopedEq::new(
        "eq",
        EqScope::Clip {
            clip_id: "clip".into(),
        },
    );
    whole.bands = default_eq();
    for band in &mut whole.bands {
        band.on = false;
    }
    whole.bands[3].on = true;
    whole.bands[3].freq = 500.;
    whole.bands[3].gain = 6.;
    document.eq.push(whole.clone());
    let reference = render(&document, &assets, &options(), &|| false).unwrap();
    document.eq[0].range = Some(FrameRange {
        start_frame: 800,
        end_frame: 1800,
    });
    let ranged = render(&document, &assets, &options(), &|| false).unwrap();
    assert_eq!(
        bits(&ranged.samples[840..1760]),
        bits(&reference.samples[840..1760])
    );
    assert_eq!(
        bits(&ranged.samples[..800]),
        bits(&assets["source"].samples[..800])
    );
    assert_eq!(
        bits(&ranged.samples[1800..]),
        bits(&assets["source"].samples[1800..])
    );
    let split = apply_edit(
        &document,
        &Edit::SplitClip {
            clip_id: "clip".into(),
            at_frame: 1200,
            new_clip_id: "right".into(),
        },
        &assets,
    )
    .unwrap();
    assert_eq!(
        bits(&ranged.samples),
        bits(
            &render(&split, &assets, &options(), &|| false)
                .unwrap()
                .samples
        )
    );
    let trimmed = apply_edit(
        &document,
        &Edit::TrimClip {
            clip_id: "clip".into(),
            start_frame: 0,
            source_start_frame: 1000,
            end_frame: 1500,
        },
        &assets,
    )
    .unwrap();
    assert_eq!(
        bits(
            &render(&trimmed, &assets, &options(), &|| false)
                .unwrap()
                .samples
        ),
        bits(&ranged.samples[1000..2500])
    );
}

#[test]
fn range_eq_requires_twenty_ms_but_whole_clip_eq_accepts_one_frame() {
    let (mut document, assets) = fixture(vec![0.25; 1000], 8000, 1);
    let mut effect = output_gain(
        "eq",
        EqScope::Mix {},
        Some(FrameRange {
            start_frame: 10,
            end_frame: 169,
        }),
        -6.,
    );
    document.eq.push(effect.clone());
    assert!(document.validate(&assets).is_err());
    effect.range = Some(FrameRange {
        start_frame: 10,
        end_frame: 170,
    });
    document.eq[0] = effect;
    document.validate(&assets).unwrap();
    document.lanes[0].clips[0].end_frame = 1;
    document.eq[0].range = None;
    document.eq[0].scope = EqScope::Clip {
        clip_id: "clip".into(),
    };
    assert_eq!(
        render(&document, &assets, &options(), &|| false)
            .unwrap()
            .frames(),
        1
    );
}

#[test]
fn prepared_seek_reconstructs_exact_state_and_adoption_returns_the_retired_player() {
    let (mut document, assets) = fixture(sine(8000, 3000, 700., 0.1), 8000, 1);
    document
        .eq
        .push(output_gain("eq", EqScope::Mix {}, None, -6.));
    let expected = render(&document, &assets, &options(), &|| false).unwrap();
    let mut player = ArrangementPlayer::new(&document, &assets, &options()).unwrap();
    let prepared = player.prepare_seek(1234, &|| false).unwrap();
    let retired = player.apply_prepared_seek(prepared).unwrap();
    assert_eq!(retired.position(), 0);
    let mut block = vec![0.; 100];
    player.process_block(&mut block).unwrap();
    assert_eq!(bits(&block), bits(&expected.samples[1234..1334]));
    assert_eq!(
        player.frame_at(2999, &|| false).unwrap().unwrap()[0].to_bits(),
        expected.samples[2999].to_bits()
    );
    assert!(player.next_frame().unwrap().is_none());
}

#[test]
fn live_gains_mute_solo_and_eq_changes_settle_without_accumulating_processing() {
    let (mut document, assets) = fixture(vec![0.2; 8000], 8000, 1);
    document
        .eq
        .push(output_gain("eq", EqScope::Mix {}, None, 0.));
    let mut player = ArrangementPlayer::new(&document, &assets, &options()).unwrap();
    player.process_block(&mut vec![0.; 100]).unwrap();
    player.set_clip_gain("clip", -6.).unwrap();
    let mut transition = vec![0.; 400];
    player.process_block(&mut transition).unwrap();
    assert!((transition[399] - (0.2 * 10_f64.powf(-6. / 20.)) as f32).abs() < 1e-7);
    assert!(
        transition
            .windows(2)
            .all(|pair| (pair[1] - pair[0]).abs() < 0.002)
    );
    player.set_clip_gain("clip", 0.).unwrap();
    let effect = output_gain("eq", EqScope::Mix {}, None, -6.);
    player.set_eq(&effect).unwrap();
    player.process_block(&mut transition).unwrap();
    assert!((transition[399] - (0.2 * 10_f64.powf(-6. / 20.)) as f32).abs() < 1e-7);
    player.set_eq(&effect).unwrap();
    player.process_block(&mut transition).unwrap();
    assert!((transition[399] - (0.2 * 10_f64.powf(-6. / 20.)) as f32).abs() < 1e-7);
    player
        .set_lane_controls("lane", 0., 0., true, true)
        .unwrap();
    player.process_block(&mut transition).unwrap();
    assert_eq!(transition[399], 0.);
}

#[test]
fn prepared_replacements_crossfade_before_one_guard_and_keep_latest_queued_revision() {
    let (document, assets) = fixture(vec![0.2; 3000], 8000, 1);
    let mut player = ArrangementPlayer::new(&document, &assets, &options()).unwrap();
    player.process_block(&mut vec![0.; 100]).unwrap();
    let mut quieter = document.clone();
    quieter.lanes[0].gain_db = -6.;
    let mut next = ArrangementPlayer::new(&quieter, &assets, &options()).unwrap();
    next.seek(player.position(), &|| false).unwrap();
    assert!(
        player
            .replace_prepared_boxed(Box::new(next))
            .unwrap()
            .is_none()
    );
    player.process_block(&mut [0.; 50]).unwrap();
    let mut quietest = document.clone();
    quietest.lanes[0].gain_db = -12.;
    let mut pending = ArrangementPlayer::new(&quietest, &assets, &options()).unwrap();
    pending.seek(player.position(), &|| false).unwrap();
    assert!(
        player
            .replace_prepared_boxed(Box::new(pending))
            .unwrap()
            .is_none()
    );
    let mut output = vec![];
    for _ in 0..600 {
        output.push(player.next_frame().unwrap().unwrap()[0]);
        drop(player.take_retired().unwrap());
    }
    assert!(
        output
            .windows(2)
            .all(|pair| (pair[1] - pair[0]).abs() < 0.004)
    );
    assert!((output[599] - (0.2 * 10_f64.powf(-12. / 20.)) as f32).abs() < 1e-7);
}

#[test]
fn cancellation_budget_malformed_pcm_and_empty_arrangements_fail_without_source_changes() {
    let (document, mut assets) = fixture(vec![0.2; 50_000], 8000, 1);
    let before = bits(&assets["source"].samples);
    assert!(render(&document, &assets, &options(), &|| true).is_err());
    let calls = Cell::new(0);
    assert!(
        render(&document, &assets, &options(), &|| {
            calls.set(calls.get() + 1);
            calls.get() > 8
        })
        .is_err()
    );
    let tiny = RenderOptions {
        max_output_bytes: 4,
        ..options()
    };
    assert!(render(&document, &assets, &tiny, &|| false).is_err());
    assert_eq!(before, bits(&assets["source"].samples));
    assets.insert(
        "source".into(),
        Arc::new(Audio {
            samples: vec![f32::NAN; 50_000],
            sample_rate: 8000,
            channels: 1,
        }),
    );
    assert!(ArrangementPlayer::new(&document, &assets, &options()).is_err());
    let empty = Arrangement::new(8000, 1);
    assert!(
        ArrangementPlayer::new(&empty, &AssetLibrary::new(), &options())
            .unwrap()
            .next_frame()
            .unwrap()
            .is_none()
    );
    assert!(render(&empty, &AssetLibrary::new(), &options(), &|| false).is_err());
}

#[test]
fn bounded_preparation_skips_whole_prefix_and_matches_a_settled_eq_control() {
    let (mut document, assets) = fixture(sine(8000, 160_000, 500., 0.1), 8000, 1);
    let mut effect = ScopedEq::new("eq", EqScope::Mix {});
    for band in &mut effect.bands {
        band.on = false;
    }
    effect.bands[3].on = true;
    effect.bands[3].freq = 500.;
    effect.bands[3].gain = 6.;
    document.eq.push(effect);
    let expected = render(&document, &assets, &options(), &|| false).unwrap();
    let original = ArrangementPlayer::new(&document, &assets, &options()).unwrap();
    let mut cache = original.validation_cache();
    let calls = Cell::new(0);
    let mut prepared = ArrangementPlayer::prepare_bounded(
        &document,
        &assets,
        &options(),
        &mut cache,
        120_000,
        12_000,
        &|| {
            calls.set(calls.get() + 1);
            false
        },
    )
    .unwrap();
    assert!(
        calls.get() < 20,
        "Bounded preparation checked {} chunks",
        calls.get()
    );
    let mut actual = [0.; 200];
    prepared.process_block(&mut actual).unwrap();
    assert_eq!(bits(&actual), bits(&expected.samples[120_000..120_200]));
}

#[test]
fn prepared_eq_rejects_wrong_sample_rate_without_changing_the_document() {
    let (mut document, assets) = fixture(vec![0.2; 1000], 8000, 1);
    document
        .eq
        .push(output_gain("eq", EqScope::Mix {}, None, 0.));
    let mut player = ArrangementPlayer::new(&document, &assets, &options()).unwrap();
    let prepared =
        PreparedEqUpdate::new(&output_gain("eq", EqScope::Mix {}, None, -6.), 48_000).unwrap();
    assert!(player.apply_prepared_eq(prepared).is_err());
    assert_eq!(player.arrangement(), &document);
}
