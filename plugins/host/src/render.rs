use crate::{host_context::Changes, instance::Instance};
use anyhow::{Result, ensure};
use mastercraft::audio::Audio;
use serde::{Deserialize, Serialize};
use vst3::{
    ComWrapper,
    Steinberg::{Vst::*, *},
};

const BLOCK: usize = 1024;
#[derive(Clone, Deserialize, Serialize)]
pub struct ParameterValue {
    pub id: u32,
    pub normalized: f64,
}

pub fn process(
    plugin: &Instance,
    input: &Audio,
    parameters: &[ParameterValue],
) -> Result<(Audio, u32)> {
    let known = plugin.parameters()?;
    for p in parameters {
        ensure!(
            p.normalized.is_finite() && (0. ..=1.).contains(&p.normalized),
            "Parameter values must be normalized 0–1"
        );
        ensure!(
            known.iter().any(|k| k.id == p.id && !k.read_only),
            "Unknown/read-only plugin parameter {}",
            p.id
        );
    }
    unsafe {
        ensure!(
            plugin
                .component
                .getBusCount(MediaTypes_::kAudio as _, BusDirections_::kInput as _)
                > 0,
            "Instrument/generator plugins without audio input are unsupported"
        );
        ensure!(
            plugin
                .processor
                .canProcessSampleSize(SymbolicSampleSizes_::kSample32 as _)
                == kResultOk,
            "Plugin does not support 32-bit float audio"
        );
        let mut channels = input.channels;
        let mut arrangement = if channels == 1 {
            SpeakerArr::kMono
        } else {
            SpeakerArr::kStereo
        };
        if plugin
            .processor
            .setBusArrangements(&mut arrangement, 1, &mut arrangement, 1)
            != kResultOk
        {
            channels = 2;
            arrangement = SpeakerArr::kStereo;
            ensure!(
                plugin
                    .processor
                    .setBusArrangements(&mut arrangement, 1, &mut arrangement, 1)
                    == kResultOk,
                "Plugin does not accept a mono/stereo main input/output"
            );
        }
        for direction in [BusDirections_::kInput, BusDirections_::kOutput] {
            let count = plugin
                .component
                .getBusCount(MediaTypes_::kAudio as _, direction as _);
            ensure!((1..=16).contains(&count), "Unsupported plugin bus count");
            for bus in 0..count {
                plugin.component.activateBus(
                    MediaTypes_::kAudio as _,
                    direction as _,
                    bus,
                    if bus == 0 { 1 } else { 0 },
                );
            }
        }
        let mut setup = ProcessSetup {
            processMode: ProcessModes_::kOffline as _,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as _,
            maxSamplesPerBlock: BLOCK as i32,
            sampleRate: input.sample_rate as f64,
        };
        if plugin.processor.setupProcessing(&mut setup) != kResultOk {
            setup.processMode = ProcessModes_::kRealtime as _;
            ensure!(
                plugin.processor.setupProcessing(&mut setup) == kResultOk,
                "Plugin rejected the processing setup"
            );
        }
        ensure!(
            plugin.component.setActive(1) == kResultOk,
            "Plugin could not be activated"
        );
        let _ = plugin.processor.setProcessing(1);
        let latency = plugin.processor.getLatencySamples();
        ensure!(
            latency <= input.sample_rate * 10,
            "Plugin latency exceeds the supported 10-second bound"
        );
        if let Some(controller) = &plugin.controller {
            for p in parameters {
                controller.setParamNormalized(p.id, p.normalized);
            }
        }
        let changes = ComWrapper::new(Changes::new(
            &parameters
                .iter()
                .map(|p| (p.id, p.normalized))
                .collect::<Vec<_>>(),
        ))
        .to_com_ptr::<IParameterChanges>()
        .unwrap();
        let mut before = vec![vec![0_f32; BLOCK]; channels];
        let mut after = vec![vec![0_f32; BLOCK]; channels];
        let mut input_pointers = before
            .iter_mut()
            .map(|ch| ch.as_mut_ptr())
            .collect::<Vec<_>>();
        let mut output_pointers = after
            .iter_mut()
            .map(|ch| ch.as_mut_ptr())
            .collect::<Vec<_>>();
        let mut in_bus = AudioBusBuffers {
            numChannels: channels as i32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: input_pointers.as_mut_ptr(),
            },
        };
        let mut out_bus = AudioBusBuffers {
            numChannels: channels as i32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: output_pointers.as_mut_ptr(),
            },
        };
        let mut context: ProcessContext = std::mem::zeroed();
        context.sampleRate = input.sample_rate as f64;
        let mut result = Audio::new(
            vec![0.; input.samples.len()],
            input.sample_rate,
            input.channels,
        )?;
        let total = input.frames() + latency as usize;
        for start in (0..total).step_by(BLOCK) {
            let count = (total - start).min(BLOCK);
            for ch in 0..channels {
                before[ch].fill(0.);
                after[ch].fill(0.);
                for (i, sample) in before[ch].iter_mut().take(count).enumerate() {
                    if start + i < input.frames() {
                        *sample = input.samples
                            [(start + i) * input.channels + ch.min(input.channels - 1)];
                    }
                }
            }
            context.projectTimeSamples = start as i64;
            let mut data = ProcessData {
                processMode: setup.processMode,
                symbolicSampleSize: setup.symbolicSampleSize,
                numSamples: count as i32,
                numInputs: 1,
                numOutputs: 1,
                inputs: &mut in_bus,
                outputs: &mut out_bus,
                inputParameterChanges: if start == 0 {
                    changes.as_ptr()
                } else {
                    std::ptr::null_mut()
                },
                outputParameterChanges: std::ptr::null_mut(),
                inputEvents: std::ptr::null_mut(),
                outputEvents: std::ptr::null_mut(),
                processContext: &mut context,
            };
            ensure!(
                plugin.processor.process(&mut data) == kResultOk,
                "Plugin processing failed"
            );
            for (i, _) in after[0].iter().take(count).enumerate() {
                let index = start + i;
                if index < latency as usize {
                    continue;
                }
                let frame = index - latency as usize;
                if frame >= input.frames() {
                    continue;
                }
                for ch in 0..input.channels {
                    let value = if input.channels == 1 && channels == 2 {
                        (after[0][i] + after[1][i]) * 0.5
                    } else {
                        after[ch][i]
                    };
                    ensure!(value.is_finite(), "Plugin returned non-finite audio");
                    result.samples[frame * input.channels + ch] = value;
                }
            }
        }
        Ok((result, latency))
    }
}
