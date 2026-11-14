"""Build a self-contained static site. Requires Rust wasm target and matching wasm-bindgen."""
import argparse
from pathlib import Path
import shutil
import subprocess
import sys
from runtime_cache import version_runtime

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('--wasm-bindgen', default='wasm-bindgen')
parser.add_argument('--out', type=Path, default=ROOT/'web/dist')
args = parser.parse_args()
subprocess.run(['cargo','build','--locked','--release','--target','wasm32-unknown-unknown','--manifest-path',str(ROOT/'web/dsp/Cargo.toml')],check=True)
destination = args.out.resolve()
destination.mkdir(parents=True, exist_ok=True)
shutil.copytree(ROOT/'web/site',destination,dirs_exist_ok=True)
subprocess.run([args.wasm_bindgen,str(ROOT/'web/dsp/target/wasm32-unknown-unknown/release/slimeybeats_web_dsp.wasm'),'--target','web','--out-dir',str(destination/'pkg'),'--out-name','dsp'],check=True)
art = destination/'assets'; art.mkdir(exist_ok=True)
shutil.copy2(ROOT/'rust/assets/learning-guide.json',art/'learning-guide.json')
for name in ['slimeybeats-small-icon.png','slimeybeats-dragon-portrait.png','sovereign-panel-frame.png','sovereign-amethyst-crest.png','sovereign-ambient-ribbons.png','slimeybeats-slime-frame.png']:
    shutil.copy2(ROOT/'rust/assets'/name,art/name)
shutil.copy2(ROOT/'examples/audio/demo-input.wav',art/'demo-input.wav')
shutil.copy2(ROOT/'examples/audio/CREDITS.md',destination/'DEMO-CREDITS.txt')
shutil.copy2(ROOT/'LICENSE',destination/'LICENSE.txt')
sys.path.insert(0,str(ROOT/'rust/tools'))
from package_release import dependency_notices
dependency_notices(ROOT/'web/dsp',destination)
notice=destination/'THIRD-PARTY-NOTICES.txt'
notice.write_text(notice.read_text(encoding='utf-8').replace('not used on Windows','not used in the browser').replace('FFmpeg is an optional external installation and is not bundled.','The browser build does not use or bundle FFmpeg.'),encoding='utf-8')
shutil.copy2(ROOT/'OUTPUT_OWNERSHIP.md',destination/'OUTPUT-OWNERSHIP.txt')
print(f'Runtime cache version: {version_runtime(destination)}')
print(f'Static site ready: {destination}')
