"""Prepare the documented CC0 music example; requires FFmpeg and the release CLI."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import urllib.request

SOURCE_URL='https://opengameart.org/sites/default/files/tecnological_messup_v2_0.ogg'
SOURCE_PAGE='https://opengameart.org/content/technological-messup'
SOURCE_SHA256='d782c36a82cfdb02772001fb5830a8d4e0e82392dd157fe4410d07d11f01e45e'

def run(*args):
    return subprocess.run(list(map(str,args)),capture_output=True,text=True,encoding='utf-8',errors='replace',check=True)

def loudness(path):
    summary=run('ffmpeg','-hide_banner','-nostdin','-i',path,'-af','ebur128=peak=true:framelog=verbose','-f','null','-').stderr.rsplit('Summary:',1)[1]
    return float(re.search(r'\bI:\s+([-\d.]+) LUFS',summary)[1])

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source',type=Path)
    args=parser.parse_args()
    project=Path(__file__).resolve().parents[1]
    target=project.parent/'examples/audio'; target.mkdir(parents=True,exist_ok=True)
    if (target/'demo-input.wav').exists(): raise FileExistsError('Demo audio already exists')
    with tempfile.TemporaryDirectory(prefix='slimeybeats-demo-') as temp:
        work=Path(temp)
        original=args.source
        if original is None:
            original=work/'source.ogg'
            with urllib.request.urlopen(SOURCE_URL,timeout=40) as response: original.write_bytes(response.read())
        if hashlib.sha256(original.read_bytes()).hexdigest()!=SOURCE_SHA256: raise ValueError('Demo source checksum mismatch')
        raw=work/'excerpt.wav'
        run('ffmpeg','-v','error','-nostdin','-ss','16','-i',original,'-t','45','-af','afade=t=in:d=0.03,afade=t=out:st=44.7:d=0.3','-c:a','pcm_f32le',raw)
        input_gain=-20.-loudness(raw)
        run('ffmpeg','-v','error','-nostdin','-i',raw,'-af',f'volume={input_gain:.6f}dB','-c:a','pcm_s24le',target/'demo-input.wav')
        binary=project/'target/release/slimeybeats-ez-master-cli.exe'
        run(binary,target/'demo-input.wav','--profile','neutral','--mode','streaming_safe','--format','wav24','--out',work/'master','--threads','4')
        report_path=next((work/'master').glob('*.report.json'))
        report=json.loads(report_path.read_text())
        shutil.copy2(Path(report['exports'][0]['path']),target/'demo-master.wav')
        levels={}
        for name in ('input','master'):
            wave=target/f'demo-{name}.wav'
            gain=-20.-loudness(wave)
            run('ffmpeg','-v','error','-nostdin','-i',wave,'-af',f'volume={gain:.6f}dB','-map_metadata','-1','-c:a','libmp3lame','-b:a','192k',target/f'listen-{name}.mp3')
            levels[name]={'wav_lufs':loudness(wave),'preview_lufs':loudness(target/f'listen-{name}.mp3')}
        report['file']='examples/audio/demo-input.wav'
        report['configuration']['out_dir']='examples/audio'
        report['exports'][0]['path']='examples/audio/demo-master.wav'
        (target/'demo-master.report.json').write_text(json.dumps(report,indent=2)+'\n')
        provenance={'title':'Technological Messup','artist':'Bärghest','license':'CC0 1.0 Universal',
                    'license_url':'https://creativecommons.org/publicdomain/zero/1.0/','source_page':SOURCE_PAGE,
                    'download_url':SOURCE_URL,'original_sha256':SOURCE_SHA256,'excerpt_start_s':16,'duration_s':45,
                    'input_modification':'Decoded from Ogg, short fades, linear headroom adjustment to approximately -20 LUFS, WAV24.',
                    'master_modification':'SlimeyBeats Neutral/Gentler defaults; WAV24.',
                    'preview_modification':'Both previews linearly matched to approximately -20 LUFS, MP3 192 kbps.',
                    'input_gain_db':input_gain,'levels':levels,'app_version':report['version'],
                    'files':{path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in target.iterdir() if path.suffix in ('.wav','.mp3')}}
        (target/'source.json').write_text(json.dumps(provenance,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
        print(json.dumps(levels,indent=2))

if __name__=='__main__': main()
