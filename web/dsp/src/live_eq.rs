use crate::{failure, settings};
use mastercraft::{
    analysis::Analysis,
    config::Settings,
    dsp::{biquad, eq_chain::EqParameters, eq_loudness::EqLoudnessModel, realtime_eq::RealtimeEq},
};
use wasm_bindgen::prelude::*;

const BLOCK_CAPACITY: usize = 2048;

#[wasm_bindgen]
pub struct LiveEq {
    processor: RealtimeEq,
    model: EqLoudnessModel,
    rate: u32,
    channels: usize,
    input: Vec<f32>,
    output: Vec<f32>,
}

#[wasm_bindgen]
impl LiveEq {
    #[wasm_bindgen(constructor)]
    pub fn new(
        rate: u32,
        channels: u32,
        analysis_json: &str,
        json: &str,
    ) -> Result<LiveEq, JsValue> {
        if !(8000..=192000).contains(&rate) || !(1..=2).contains(&channels) {
            return Err(failure("Invalid playback layout"));
        }
        let a: Analysis = serde_json::from_str(analysis_json).map_err(failure)?;
        let s = settings(json).map_err(failure)?;
        let model = EqLoudnessModel::new(&a.spectrum, rate, a.metrics.integrated_lufs);
        let parameters = EqParameters::new(&s, rate, model.hold_gain(&s));
        Ok(Self {
            processor: RealtimeEq::new(rate, channels as usize, parameters),
            model,
            rate,
            channels: channels as usize,
            input: vec![0.; BLOCK_CAPACITY * 2],
            output: vec![0.; BLOCK_CAPACITY * 2],
        })
    }

    pub fn set_settings(&mut self, json: &str) -> Result<f64, JsValue> {
        let s = settings(json).map_err(failure)?;
        let hold = self.model.hold_gain(&s);
        self.processor
            .set_parameters(EqParameters::new(&s, self.rate, hold));
        Ok(self.model.delta_db(&s) + hold + s.eq_output_db)
    }

    pub fn delay(&self) -> u32 {
        self.processor.delay() as u32
    }
    pub fn input_ptr(&self) -> *const f32 {
        self.input.as_ptr()
    }
    pub fn output_ptr(&self) -> *const f32 {
        self.output.as_ptr()
    }

    // Pointers refer to preallocated buffers: the audio callback does not allocate per block.
    pub fn process_block(&mut self, frames: usize) -> bool {
        if frames > BLOCK_CAPACITY {
            return false;
        }
        for i in 0..frames {
            let mut frame = [0.; 2];
            for (ch, value) in frame.iter_mut().enumerate().take(self.channels) {
                *value = self.input[i * self.channels + ch] as f64;
            }
            let output = self.processor.process(frame);
            if !output.iter().all(|v| v.is_finite()) {
                self.output.fill(0.);
                return false;
            }
            for (ch, value) in output.iter().enumerate().take(self.channels) {
                self.output[i * self.channels + ch] = *value as f32;
            }
        }
        true
    }
}

#[wasm_bindgen]
pub fn response(json: &str, rate: u32) -> Result<Vec<f32>, JsValue> {
    let s: Settings = settings(json).map_err(failure)?;
    let filters: Vec<_> = s
        .manual_eq
        .iter()
        .filter(|b| b.on)
        .map(|b| biquad::from_band(b, rate))
        .collect();
    Ok((0..256)
        .map(|i| {
            let f = 20. * (rate as f64 * 0.49 / 20.).powf(i as f64 / 255.);
            (s.eq_output_db + filters.iter().map(|c| c.response_db(f, rate)).sum::<f64>()) as f32
        })
        .collect())
}
