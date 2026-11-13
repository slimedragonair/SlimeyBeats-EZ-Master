"""Derive a fixed-delay QA effect from a pinned, locally supplied upstream example.

Obtain examples/gain.rs from coupler-rs/vst3-rs commit
b2a45f2d40e602eff5a224b41613735388868590 (MIT OR Apache-2.0).
This script downloads nothing and never overwrites an existing fixture directory.
"""
import argparse
import hashlib
from pathlib import Path
import shutil

SOURCE_SHA = '22bb7b9cc2ce3645a1d23c09e1f5d0f3399f65c520bc439ac63faf8e389a4c1c'


def prepare(source, destination):
    data = source.read_bytes().replace(b'\r\n', b'\n')
    if hashlib.sha256(data).hexdigest() != SOURCE_SHA:
        raise ValueError('Use the documented pinned gain example; source hash differs')
    text = data.decode('utf-8')
    replacements = [
        ('extern "system" fn GetPluginFactory() -> *mut IPluginFactory {',
         'extern "system" fn GetPluginFactory() -> *mut IPluginFactory {\n    println!("SlimeyBeats QA fixture startup log");\n    extern "C" { fn puts(text: *const std::ffi::c_char) -> std::ffi::c_int; }\n    unsafe { puts(b"SlimeyBeats QA native CRT startup log\\0".as_ptr().cast()); }'),
        ('struct GainProcessor {\n    gain: AtomicU64,\n}',
         'struct GainProcessor {\n    gain: AtomicU64,\n    delay: std::sync::Mutex<(Vec<[f32; 2]>, usize)>,\n}'),
        ('gain: AtomicU64::new(1.0f64.to_bits()),',
         'gain: AtomicU64::new(1.0f64.to_bits()),\n            delay: std::sync::Mutex::new((vec![[0.; 2]; 137], 0)),'),
        ('unsafe fn getLatencySamples(&self) -> u32 {\n        0\n    }',
         'unsafe fn getLatencySamples(&self) -> u32 {\n        137\n    }'),
        ('for i in 0..num_samples {\n            output_l[i] = gain * input_l[i];\n            output_r[i] = gain * input_r[i];\n        }',
         'let mut state = self.delay.lock().unwrap();\n        for i in 0..num_samples {\n            let position = state.1;\n            let delayed = state.0[position];\n            state.0[position] = [input_l[i], input_r[i]];\n            state.1 = (position + 1) % 137;\n            output_l[i] = gain * delayed[0];\n            output_r[i] = gain * delayed[1];\n        }'),
    ]
    for old, new in replacements:
        if text.count(old) != 1:
            raise ValueError('Pinned source layout changed; inspect before generating')
        text = text.replace(old, new)
    destination.mkdir(parents=True, exist_ok=False)
    (destination / 'src').mkdir()
    (destination / 'src/lib.rs').write_text('// QA derivative of coupler-rs/vst3-rs gain example at the pinned commit.\n// Original license: MIT OR Apache-2.0; see copied license files.\n' + text, encoding='utf-8')
    (destination / 'Cargo.toml').write_text('[package]\nname="slimey-latency-fixture"\nversion="0.1.0"\nedition="2021"\nlicense="MIT OR Apache-2.0"\n[lib]\ncrate-type=["cdylib"]\n[dependencies]\nvst3="=0.3.0"\n', encoding='utf-8')
    for name in ['LICENSE-MIT', 'LICENSE-APACHE']:
        shutil.copy2(source.parent.parent / name, destination / name)
    (destination / 'PROVENANCE.txt').write_text(__doc__ + '\nLF-normalized source SHA-256: ' + SOURCE_SHA + '\nChanges: fixed 137-frame stereo delay and Rust/C startup logs. This is a QA fixture, not a distributed audio effect.\n', encoding='utf-8')
    print(destination)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    prepare(args.source.resolve(), args.out.resolve())
