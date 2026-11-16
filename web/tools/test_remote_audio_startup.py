"""Deterministic deadline/PCM guards used by the real streaming browser probe."""
from pathlib import Path
import shutil
import subprocess
import unittest


@unittest.skipUnless(shutil.which("node"), "Node is required for the browser probe helper")
class RemoteAudioStartupTests(unittest.TestCase):
    def test_delayed_pcm_dead_clocks_silence_and_errors(self):
        helper = Path(__file__).with_name("remote_audio_startup.mjs").resolve().as_uri()
        script = r'''
import assert from 'node:assert/strict';
const {waitForMeasuredAudio,waitForAudioSilence,AUDIO_STARTUP_TIMEOUT_MS,AUDIO_MIN_RMS,AUDIO_SILENCE_RMS}=await import(process.argv[1]);
function fixture(measure,ready=()=>true,error=()=>null){
    let time=0;const trace={};
    return {trace,options:{measure:()=>measure(time),ready:()=>ready(time),error,snapshot:()=>({
        context:'running',ready_state:4,context_time:time<800?.0116:time/1000,current_time:time<800?0:time/1000}),
        trace,label:'Synthetic captured audio',now:()=>time,wait:async ms=>{time+=ms;}}};
}
const delayed=fixture(time=>time<960?0:.05,time=>time>=800);
const passed=await waitForMeasuredAudio(delayed.options);
assert.equal(passed.elapsed_ms,960);assert.equal(passed.rms,.05);
assert.ok(delayed.trace.samples.some(sample=>sample.context_time===.0116 && sample.rms===0));
for(const missing of [fixture(()=>0),fixture(()=>AUDIO_MIN_RMS),fixture(()=>.05,()=>false)]){
    await assert.rejects(waitForMeasuredAudio(missing.options),/no measurable samples before the startup deadline/);
    assert.equal(missing.trace.elapsed_ms,AUDIO_STARTUP_TIMEOUT_MS);
    assert.equal(missing.trace.samples.length,64);
    assert.equal(missing.trace.samples[0].elapsed_ms,0);
}
await assert.rejects(waitForMeasuredAudio(fixture(()=>NaN).options),/Nonfinite captured PCM/);
await assert.rejects(waitForMeasuredAudio(fixture(()=>.05,()=>true,()=> 'CORS denied').options),/CORS denied/);
let lateTime=0;const late=fixture(()=>lateTime===0?0:.05);
late.options.now=()=>lateTime;late.options.wait=async()=>{lateTime=AUDIO_STARTUP_TIMEOUT_MS+1;};
await assert.rejects(waitForMeasuredAudio(late.options),/startup deadline/);
const drain=fixture(time=>time<960?.05:0,time=>time>=800);
assert.equal((await waitForAudioSilence(drain.options)).elapsed_ms,960);
for(const audible of [fixture(()=>.05),fixture(()=>AUDIO_SILENCE_RMS),fixture(()=>0,()=>false)]){
    await assert.rejects(waitForAudioSilence(audible.options),/did not reach silence/);
    assert.equal(audible.trace.elapsed_ms,AUDIO_STARTUP_TIMEOUT_MS);
}
'''
        subprocess.run([shutil.which("node"), "--input-type=module", "-e", script, helper], check=True)
