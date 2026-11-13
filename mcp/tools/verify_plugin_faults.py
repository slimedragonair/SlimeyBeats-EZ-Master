"""Real native plugin-worker termination/stall containment through MCP."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import wave

from protocol_client import Client

ROOT = Path(__file__).resolve().parents[2]


def verify(server):
    result = []
    with tempfile.TemporaryDirectory(prefix='SlimeyBeats plugin fault QA ') as temporary:
        root = Path(temporary)
        plugins = root / 'Plugins'
        plugins.mkdir()
        fault = plugins / 'FaultFixture.vst3'
        subprocess.run(['rustc', '--edition', '2024', '--crate-type', 'cdylib',
                        str(ROOT / 'plugins/host/tools/fixtures/fault_plugin.rs'), '-o', str(fault)], check=True, timeout=60)
        source = root / 'source.wav'
        with wave.open(str(source), 'wb') as wav:
            wav.setparams((1, 2, 8000, 0, 'NONE', ''))
            wav.writeframes(b'\0\0' * 8000)
        for mode in ['null', 'exit', 'stall']:
            previous = os.environ.get('SLIMEYBEATS_VST_TEST_FAULT')
            os.environ['SLIMEYBEATS_VST_TEST_FAULT'] = mode
            client = None
            try:
                client = Client([str(server), '--input-root', str(root), '--output-root', str(root / 'Exports'), '--plugin-root', str(plugins)])
                loaded = client.run('load_audio', {'library': 0, 'path': source.name})
                start = time.monotonic()
                submitted = client.call('probe_plugin', {'library': 0, 'path': fault.name})
                if mode == 'stall':
                    time.sleep(.1)
                    client.call('cancel_job', {'job_id': submitted['job_id']})
                job = client.finish(submitted)
                assert job['status'] == ('cancelled' if mode == 'stall' else 'failed'), job
                assert time.monotonic() - start < 5, job
                assert client.call('get_session')['source']['source_id'] == loaded['source_id']
                assert client.call('get_capabilities')['companion_version']
                # A new real DSP job must work in the surviving parent process.
                assert client.run('analyze_audio', {'source_id': loaded['source_id']})
                result.append({'fault': mode, 'status': job['status'], 'parent_usable': True, 'source_retained': True})
            finally:
                if client:
                    client.close()
                if previous is None:
                    os.environ.pop('SLIMEYBEATS_VST_TEST_FAULT', None)
                else:
                    os.environ['SLIMEYBEATS_VST_TEST_FAULT'] = previous
    return {'passed': result, 'scope': 'Invalid/null factory, plugin-initiated worker exit and cancelled stalled factory in a real native module. Not universal plugin certification.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--server', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve()), indent=2))
