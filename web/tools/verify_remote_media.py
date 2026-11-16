"""Verify real v2 Service Worker media streams in an isolated browser fixture.

Starts only an owned loopback backend/site; never connects to the public service.
Copies candidate JS unchanged, adds a generated probe, and keeps all credentials
in browser memory. Requires Python Playwright and real native worker/CLI/FFmpeg.
"""
import argparse
from array import array
from contextlib import closing, contextmanager
import functools
import hashlib
import http.client
import http.server
import json
import math
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import struct
import subprocess
import sys
import threading
import time
import wave

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]
MODULES = ("remote-media-worker.js", "hybrid/remote-media-bridge.js",
           "hybrid/remote-media-policy.js", "hybrid/server-client.js",
           "hybrid/remote-project.js", "hybrid/main-controls.js", "hybrid/remote-mastering.js",
           "audio/streaming-player.js", "audio/live-eq-node.js", "audio/eq-processor.js",
           "pkg/dsp.js", "pkg/dsp_bg.wasm")
CHECKS = ("native_http_bank_root_export_parity", "service_worker_claim", "chunk_upload_native_import_metadata",
          "range_7_17", "401_and_cross_guest_404", "html_audio_nonzero", "stop_silence",
          "server_analyze_master_for_live_eq", "actual_streaming_player_live_eq", "live_eq_settings_change", "live_eq_stop_silence",
          "virtual_artifact_stream_download", "no_full_decode_or_blob_credentials_memory_only")
HTML = """<!doctype html><meta charset=utf-8><title>Remote media isolation probe</title>
<input id=file type=file><button id=play>Play</button><button id=stop>Stop</button>
<button id=eq-play>Play actual live EQ</button><button id=eq-stop>Stop actual live EQ</button>
<audio id=audio preload=none></audio><pre id=status>Loading probe</pre>
<script type=module src=probe.js></script>"""

PROBE = r"""
import {ServerClient} from './hybrid/server-client.js';
import {RemoteProject} from './hybrid/remote-project.js';
import {RemoteMastering} from './hybrid/remote-mastering.js';
import {StreamingPlayer} from './audio/streaming-player.js';
import init, {defaults, LiveEq} from './pkg/dsp.js';
import {RemoteMediaBridge} from './hybrid/remote-media-bridge.js';
import {serverProcessing} from './hybrid/main-controls.js';
import {waitForMeasuredAudio,waitForAudioSilence} from './remote_audio_startup.mjs';
const state = {requests:[], decodeCalls:0, blobReads:0, objectUrls:0, fullMediaReads:0,
    playError:null, started:false, stopped:false, progress:[], accessMessages:0,workerFetches:[],mediaEvents:[]};
const secrets = [];
const nativeFetch = globalThis.fetch.bind(globalThis);
globalThis.fetch = async (input, options) => {
    const url = new URL(typeof input==='string' || input instanceof URL ? input : input.url, location.href);
    const entry = {origin:url.origin,path:url.pathname,query:!!url.search,method:options?.method||'GET'};
    state.requests.push(entry);
    const response = await nativeFetch(input,options); entry.status=response.status; return response;
};
const NativeContext = globalThis.AudioContext || globalThis.webkitAudioContext;
if (NativeContext) NativeContext.prototype.decodeAudioData = function() {
    ++state.decodeCalls; throw new Error('Full PCM decode forbidden in streaming probe');
};
Response.prototype.blob = function() { ++state.blobReads; throw new Error('Full Blob response forbidden'); };
const arrayBuffer = Response.prototype.arrayBuffer;
Response.prototype.arrayBuffer = function() {
    const media = /\/remote-media\/|\/v2\/projects\/[^/]+\/(assets|artifacts)\//.test(this.url);
    if (media && !(this.status===206 && this.headers.get('Content-Length')==='11')) {
        ++state.fullMediaReads; throw new Error('Full media response buffering forbidden');
    }
    return arrayBuffer.call(this);
};
URL.createObjectURL = function() { ++state.objectUrls; throw new Error('Blob URL media path forbidden'); };
navigator.serviceWorker.addEventListener('message', event => {
    if(event.data?.type==='slimeybeats-media-access') ++state.accessMessages;
    if(event.data?.type==='remote-media-verifier-fetch-event') state.workerFetches.push(event.data.event);
});
let project,client,api,asset,ctx,analyser,source,audio,artifact,remote,eqPlayer,eqSettings,eqRawAnalysis,eqFlatRms;
const delay = ms => new Promise(resolve=>setTimeout(resolve,ms));
const EQ_STARTUP_SETTLE_SECONDS = 0.25;
const EQ_SETTINGS_SETTLE_SECONDS = 0.35;
const demand = (test,message) => {if(!test) throw new Error(message);};
async function guest() {
    const response = await fetch(api+'/v1/session',{method:'POST',headers:{'Content-Type':'application/json'},
        body:'{}',credentials:'omit',cache:'no-store'});
    demand(response.status===201,'Guest session mint failed');
    const value=await response.json(); secrets.push(value.token); return value.token;
}
function rms() {
    const samples=new Float32Array(analyser.fftSize); analyser.getFloatTimeDomainData(samples);
    return Math.sqrt(samples.reduce((sum,v)=>sum+v*v,0)/samples.length);
}
function audioState(context, element) {
    return {wall_ms:performance.now(),context:context?.state,context_time:context?.currentTime,
        current_time:element?.currentTime,paused:element?.paused,ended:element?.ended,
        ready_state:element?.readyState,network_state:element?.networkState,cross_origin:element?.crossOrigin,
        error:element?.error ? {code:element.error.code,message:element.error.message} : null,
        buffered:element ? Array.from({length:element.buffered.length},(_,index)=>[
            element.buffered.start(index),element.buffered.end(index)]) : []};
}
document.getElementById('play').onclick = () => {
    state.playError=null;
    void (async()=>{await ctx.resume(); await audio.play();state.started=true;})().catch(e=>state.playError=e.message);
};
document.getElementById('stop').onclick = () => {audio.pause();audio.currentTime=0;state.stopped=true;};
document.getElementById('eq-play').onclick = () => {
    state.eqPlayError=null;
    void eqPlayer.play().then(()=>state.eqStarted=eqPlayer.playing).catch(error=>state.eqPlayError=error.message);
};
document.getElementById('eq-stop').onclick = () => {eqPlayer.stop();state.eqStopped=true;};
function eqRms() {
    const values=new Float32Array(eqPlayer.analyser.fftSize);
    eqPlayer.analyser.getFloatTimeDomainData(values);
    return Math.sqrt(values.reduce((sum,v)=>sum+v*v,0)/values.length);
}
async function averageEqRms() {
    let sum=0;
    for(let index=0;index<12;index++){sum+=eqRms();await delay(40);}
    return sum/12;
}
function waitEqClock(seconds,trace,label,silent=false) {
    const clockStart=eqPlayer.ctx.currentTime;
    Object.assign(trace,{settle_audio_seconds:seconds,clock_start:clockStart});
    return (silent ? waitForAudioSilence : waitForMeasuredAudio)({measure:eqRms,
        ready:()=>eqPlayer.ctx.state==='running' && eqPlayer.ctx.currentTime-clockStart>=seconds
            && (silent ? !eqPlayer.playing : eqPlayer.playing && !!eqPlayer.worklet && eqPlayer.source==='master'),
        snapshot:()=>({...audioState(eqPlayer.ctx,eqPlayer.activeEntry?.audio),worklet:!!eqPlayer.worklet,
            position:eqPlayer.position(),source:eqPlayer.source}),trace,error:()=>state.eqPlayError,label});
}
window.probe = {
    async initialize(endpoint) {
        api=endpoint; client=new ServerClient(); await client.connect(api,await guest());
        // Only the controller/UI shell is replaced. The real RemoteProject,
        // ServerClient, RemoteMediaBridge and registered worker run unchanged.
        serverProcessing.attach({enabled:true,session:{connected:true,available:true,client},
            availability:{state:'online'},check:async()=>{}},()=>{},()=>{});
        const bootstrap=new RemoteMediaBridge(client);
        await bootstrap.ready();
        state.worker_claimed_without_reload=!!navigator.serviceWorker.controller;
        bootstrap.dispose();
        remote=new RemoteMastering(p=>state.progress.push(p));project=remote.project;
        const started=performance.now();
        try {await project.connect();} catch(error) {throw new Error(error.message+' '+JSON.stringify(state.requests));}
        demand(!!navigator.serviceWorker.controller,'Worker did not claim initial page');
        return {claimed:true,ready_ms:performance.now()-started,
            script:navigator.serviceWorker.controller.scriptURL,scope:(await navigator.serviceWorker.ready).scope};
    },
    async importFile() {
        const file=document.getElementById('file').files[0];
        demand(file?.size>1048576,'Fixture must cross one upload-chunk boundary');
        asset=await remote.import(file);const manifest=project.manifest;
        demand(asset.remote && !('samples' in asset) && !('buffer' in asset),'Remote asset contains client PCM');
        demand(asset.waveform.length>0 && asset.waveform.length<=1024,'Invalid waveform envelope');
        demand(asset.channelPeaks.length===asset.channels,'Missing per-channel peaks');
        audio=document.getElementById('audio');audio.crossOrigin='anonymous';audio.src=asset.mediaUrl;
        for(const type of ['loadedmetadata','canplay','playing','waiting','stalled','suspend','error','ended'])
            audio.addEventListener(type,()=>state.mediaEvents.push({type,...audioState(ctx,audio)}));
        demand(NativeContext,'Browser has no Web Audio context');
        ctx=new NativeContext(); analyser=ctx.createAnalyser();analyser.fftSize=2048;
        source=ctx.createMediaElementSource(audio);source.connect(analyser);analyser.connect(ctx.destination);
        return {id:manifest.id,asset_id:asset.id,original_id:manifest.original_id,preview_id:manifest.preview_id,
            frames:asset.frames,channels:asset.channels,sample_rate:asset.sample_rate,
            peaks:asset.waveform.length,channel_peaks:asset.channelPeaks,url:asset.mediaUrl,
            upload_chunks:state.requests.filter(r=>r.method==='PUT' && r.path.includes('/uploads/')).length};
    },
    async range() {
        const response=await fetch(asset.mediaUrl,{headers:{Range:'bytes=7-17'},cache:'no-store'});
        demand(response.status===206,'Virtual media Range did not return 206');
        const bytes=Array.from(new Uint8Array(await response.arrayBuffer()));
        demand(bytes.length===11,'Range did not contain exactly 11 bytes');
        return {status:response.status,content_range:response.headers.get('Content-Range'),
            accept_ranges:response.headers.get('Accept-Ranges'),content_length:response.headers.get('Content-Length'),bytes};
    },
    async isolation() {
        const missing=await fetch(api+project.path(),{credentials:'omit',cache:'no-store'});
        const other=await guest();
        const cross=await fetch(api+project.path(),{headers:{Authorization:'Bearer '+other},credentials:'omit',cache:'no-store'});
        const crossMedia=await fetch(api+project.path('/assets/'+asset.id+'/preview'),
            {headers:{Authorization:'Bearer '+other,Range:'bytes=7-17'},credentials:'omit',cache:'no-store'});
        demand(missing.status===401,'Unauthenticated project access was not 401');
        demand(cross.status===404,'Cross-guest project access was not 404');
        demand(crossMedia.status===404,'Cross-guest preview access was not 404');
        return {unauthenticated:missing.status,cross_guest:cross.status,cross_guest_preview:crossMedia.status};
    },
    async played() {
        state.rawAudioStartup={};
        const measured=await waitForMeasuredAudio({measure:rms,
            ready:()=>ctx.state==='running' && !audio.paused && audio.currentTime>0,
            snapshot:()=>audioState(ctx,audio),trace:state.rawAudioStartup,
            error:()=>state.playError || audio.error?.message,label:'Streamed HTML audio'});
        demand(!audio.paused && audio.currentTime>0 && measured.rms>0.0001,'Streamed HTML audio produced no measurable samples');
        return {...measured,current_time:audio.currentTime,paused:audio.paused,context:ctx.state};
    },
    async silence() {
        const clockStart=ctx.currentTime;
        state.rawStop={};
        await waitForAudioSilence({measure:rms,
            ready:()=>audio.paused && audio.currentTime===0 && ctx.state==='running' && ctx.currentTime-clockStart>=.35,
            snapshot:()=>audioState(ctx,audio),trace:state.rawStop,error:()=>state.playError,label:'Streamed HTML Stop'});
        let maximum=0;
        for(let i=0;i<5;i++){maximum=Math.max(maximum,rms());await delay(30);}
        demand(audio.paused && audio.currentTime===0 && maximum<1e-7,'Stop did not return captured samples to silence');
        return {rms:maximum,current_time:audio.currentTime,paused:audio.paused};
    },
    async prepareEq() {
        await init();
        eqSettings=JSON.parse(defaults()).settings;
        Object.assign(eqSettings,{safety:false,intensity:0,eq_hold_loudness:true,eq_output_db:0});
        const inputAnalysis=await remote.analyze(eqSettings);
        const result=await remote.masterTrack(eqSettings);
        eqRawAnalysis=result.raw_analysis;
        state.eqAnalysis={keys:Object.keys(eqRawAnalysis),suggested_profile:eqRawAnalysis.suggested_profile,
            first_issue_keys:Object.keys(eqRawAnalysis.issues?.[0]||{}),spectrum_bins:eqRawAnalysis.spectrum?.db?.length};
        // Diagnostic only: do not repair the server Analysis or replace the
        // actual worklet. This exposes the same serde boundary on failure.
        try {const diagnostic=new LiveEq(48000,2,JSON.stringify(eqRawAnalysis),JSON.stringify(eqSettings));
            diagnostic.free();state.direct_live_eq_constructor='passed';}
        catch(error){state.direct_live_eq_constructor=String(error);}
        eqPlayer=new StreamingPlayer(()=>{},error=>{state.eqPlayError=error.message;});
        eqPlayer.setOriginal(remote.original);
        eqPlayer.setMaster(result.buffer,eqRawAnalysis,eqSettings,
            result.report.input.metrics.integrated_lufs,result.report.output.metrics.integrated_lufs);
        eqPlayer.match=false;eqPlayer.volume=1;eqPlayer.select('master');
        return {input_analysis:!!inputAnalysis.spectrum,master_id:result.buffer.id,
            raw_analysis:state.eqAnalysis,settings:{eq_hold_loudness:true,eq_output_db:0},
            direct_live_eq_constructor:state.direct_live_eq_constructor};
    },
    async eqPlayed() {
        state.eqAudioStartup={};
        const measured=await waitEqClock(EQ_STARTUP_SETTLE_SECONDS,state.eqAudioStartup,'Actual StreamingPlayer LiveEq');
        eqFlatRms=await averageEqRms();
        demand(eqPlayer.playing && !!eqPlayer.worklet && eqPlayer.source==='master' && eqFlatRms>0.0001,
            'Actual StreamingPlayer LiveEq produced no measurable master output');
        return {rms:eqFlatRms,startup_ms:measured.elapsed_ms,worklet:!!eqPlayer.worklet,delay_seconds:eqPlayer.delay,
            wet_gain:eqPlayer.wetGain.gain.value,dry_gain:eqPlayer.dryGain.gain.value,
            context:eqPlayer.ctx.state,position:eqPlayer.position(),source:eqPlayer.source};
    },
    async eqChange() {
        const changed=structuredClone(eqSettings);changed.eq_output_db=-6;
        eqPlayer.updateSettings(changed,1);state.eqCutSettling={};
        await waitEqClock(EQ_SETTINGS_SETTLE_SECONDS,state.eqCutSettling,'Live EQ cut settling');
        const quieter=await averageEqRms(),ratio=quieter/eqFlatRms;
        demand(eqPlayer.playing && ratio>.43 && ratio<.59,'Live EQ -6 dB setting did not change captured worklet output');
        eqPlayer.updateSettings(eqSettings,2);state.eqRestoreSettling={};
        await waitEqClock(EQ_SETTINGS_SETTLE_SECONDS,state.eqRestoreSettling,'Live EQ restore settling');
        const restored=await averageEqRms();
        demand(restored/eqFlatRms>.9 && restored/eqFlatRms<1.1,'Live EQ restoration did not return to the same output level');
        return {flat_rms:eqFlatRms,cut_rms:quieter,cut_ratio:ratio,restored_rms:restored,
            decoding_calls:state.decodeCalls,context_unchanged:true};
    },
    async eqSilence() {
        state.eqStop={};
        await waitEqClock(.35,state.eqStop,'Actual StreamingPlayer Stop',true);
        const measured=eqRms();
        demand(!eqPlayer.playing && eqPlayer.offset===0 && measured<1e-7,'Actual StreamingPlayer Stop did not silence output');
        return {rms:measured,playing:eqPlayer.playing,offset:eqPlayer.offset};
    },
    async exportDownload() {
        artifact=await project.export('export',{audio_id:project.manifest.original_id,format:'wav24',settings:{}},'probe-export.wav');
        return {artifact_id:artifact.id,bytes:artifact.bytes,name:artifact.name,project_id:project.id};
    },
    audit(strict=true) {
        const persistent=JSON.stringify({local:{...localStorage},session:{...sessionStorage},cookie:document.cookie});
        const markup=document.documentElement.outerHTML;
        const urls=state.requests.map(r=>r.origin+r.path).concat(location.href,audio?.src||'');
        const clean=secrets.every(token=>!persistent.includes(token) && !markup.includes(token) && urls.every(u=>!u.includes(token)));
        if(strict) {
            demand(clean,'Credential was persisted or exposed in markup/URL');
            demand(state.decodeCalls===0 && state.blobReads===0 && state.objectUrls===0 && state.fullMediaReads===0,'A full-download/decode path was attempted');
        }
        return {...state,credentials_memory_only:clean,navigation_entries:performance.getEntriesByType('navigation').length};
    },
    async close() {audio?.pause();eqPlayer?.clear();await eqPlayer?.ctx?.close();await ctx?.close();await project?.close();project?.media?.dispose();client?.disconnect();},
    redact(text) {for(const secret of secrets)text=text.split(secret).join('[credential removed]');return text;}
};
document.getElementById('status').textContent='Probe ready';
"""


def free_port():
    with closing(socket.socket()) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def synthetic(path):
    rate, frames = 48000, 48000 * 8
    with wave.open(str(path), "wb") as output:
        output.setparams((2, 2, rate, 0, "NONE", "not compressed"))
        for offset in range(0, frames, rate):
            values = [round(6000 * math.sin(2 * math.pi * frequency * (offset+i) / rate))
                      for i in range(rate) for frequency in (280, 440)]
            output.writeframesraw(struct.pack("<" + "h" * len(values), *values))
    return frames


def start_site(candidate):
    class Handler(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *unused):
            pass

        def end_headers(self):
            self.send_header("Cache-Control", "no-store")
            super().end_headers()

        def send_response(self, code, message=None):
            self.server.request_log.append({"method":self.command,"path":self.path.split("?",1)[0],"status":code})
            super().send_response(code,message)

    site = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Handler, directory=str(candidate)))
    site.daemon_threads = True
    site.request_log = []
    thread = threading.Thread(target=site.serve_forever, daemon=True)
    thread.start()
    return site, thread


def await_backend(process, port):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("Owned backend exited before readiness; inspect private service.log")
        try:
            with closing(http.client.HTTPConnection("127.0.0.1", port, timeout=1)) as remote:
                remote.request("GET", "/health")
                if remote.getresponse().status == 200:
                    return
        except OSError:
            pass
        time.sleep(.05)
    raise TimeoutError("Owned backend readiness timed out")


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(65536):
            digest.update(block)
    return digest.hexdigest()


def native_bank_parity(port, token, source):
    """Independently decode actual v2 HTTP roots/exports outside the browser."""
    def request(method, path, value=None, raw=None):
        headers = {"Authorization":"Bearer " + token}
        if value is not None:
            raw = json.dumps(value).encode()
            headers["Content-Type"] = "application/json"
        if raw is not None:
            headers["Content-Length"] = str(len(raw))
        with closing(http.client.HTTPConnection("127.0.0.1",port,timeout=90)) as remote:
            remote.request(method,path,body=raw,headers=headers)
            response = remote.getresponse()
            data = response.read()
            if response.status >= 400:
                raise AssertionError("Native bank HTTP failure " + str(response.status))
            return json.loads(data) if response.getheader("Content-Type")=="application/json" else data

    def command(manifest, operation, **values):
        base = "/v2/projects/" + manifest["id"]
        manifest = request("POST",base+"/commands",{"operation":operation,"base_revision":manifest["revision"],**values})
        deadline=time.monotonic()+90
        while manifest["command"]["state"] in ("queued","running"):
            if time.monotonic()>deadline:
                raise TimeoutError("Native bank command timed out")
            time.sleep(.05)
            manifest=request("GET",base)
        assert manifest["command"]["state"]=="completed", manifest["command"]
        return manifest

    def pcm(blob):
        assert blob[:4]==b"RIFF" and blob[8:12]==b"WAVE"
        offset,encoding,bits=12,None,None
        while offset+8<=len(blob):
            tag,size=struct.unpack_from("<4sI",blob,offset)
            start=offset+8
            if tag==b"fmt ":
                encoding,channels,rate,_,_,bits=struct.unpack_from("<HHIIHH",blob,start)
                if encoding==0xFFFE:
                    encoding=struct.unpack_from("<I",blob,start+24)[0]
            elif tag==b"data":
                assert encoding==3 and bits==32
                samples=array("f");samples.frombytes(blob[start:start+size])
                if sys.byteorder!="little":
                    samples.byteswap()
                return samples,rate,channels
            offset=start+size+size%2
        raise AssertionError("Missing native float PCM")

    manifest=request("POST","/v2/projects",{})
    base="/v2/projects/"+manifest["id"]
    try:
        source_bytes=source.read_bytes()
        uploads=[]
        for kind in ("vocals","drums"):
            upload=request("POST",base+"/uploads",{"filename":"source.wav","bytes":len(source_bytes),"role":"stem","kind":kind,"label":kind})
            for offset in range(0,len(source_bytes),1048576):
                request("PUT",base+"/uploads/"+upload["id"]+"?offset="+str(offset),raw=source_bytes[offset:offset+1048576])
            uploads.append(upload["id"])
        manifest=command(manifest,"import_stems",upload_ids=uploads)
        ids={stem["id"] for stem in manifest["stems"]}
        assert len(ids)==2 and manifest["original_id"] not in ids and manifest["preview_id"] not in ids
        assert manifest["original_id"]!=manifest["preview_id"]
        raw,rate,channels=pcm(request("GET",base+"/assets/"+manifest["original_id"]+"/audio"))
        preview,_,_=pcm(request("GET",base+"/assets/"+manifest["preview_id"]+"/audio"))
        with wave.open(str(source),"rb") as wav:
            original=array("h");original.frombytes(wav.readframes(wav.getnframes()))
        if sys.byteorder!="little":
            original.byteswap()
        expected=array("f",(value/16384 for value in original))
        assert raw==expected and preview==expected, "Bank roots are not the full two-stem sum"
        manifest=command(manifest,"export",audio_id=manifest["original_id"],format="wav32f",settings={"dither":False})
        exported,export_rate,export_channels=pcm(request("GET",base+"/artifacts/"+manifest["command"]["artifact_id"]))
        assert exported==expected and (export_rate,export_channels)==(rate,channels)
        return {"passed":True,"stems":2,"raw_original_exact_full_sum":True,"preview_exact_full_mix":True,
                "original_and_preview_are_not_stems":True,"export_exact_full_sum":True,"frames":len(raw)//channels,
                "sample_rate":rate,"channels":channels}
    finally:
        request("DELETE",base)


@contextmanager
def browser_fixture(result, live):
    with sync_playwright() as playwright:
        try:
            yield playwright
        except Exception as error:
            page = live.get("page")
            if page is not None:
                try:
                    result["probe_audit"] = page.evaluate("probe.audit(false)")
                    result["redacted_failure"] = page.evaluate("text=>probe.redact(text)",str(error))
                    result["errors"] = json.loads(page.evaluate("text=>probe.redact(text)",json.dumps(result["errors"])))
                    result["audio_console"] = json.loads(page.evaluate("text=>probe.redact(text)",json.dumps(result.get("audio_console",[]))))
                except Exception:
                    pass
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--site", type=Path, default=ROOT / "web/site")
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--worker", type=Path, required=True)
    parser.add_argument("--ffmpeg", type=Path, default=Path(shutil.which("ffmpeg") or "ffmpeg"))
    parser.add_argument("--browser", choices=("chromium", "firefox", "webkit"), default="chromium")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--observe-worker-events", action="store_true",
                        help="Diagnostic only: append a passive fetch observer to the private copied worker")
    args = parser.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    result = {"success": False, "browser": args.browser, "checks": {}, "errors": [],
              "site_source": str(args.site.resolve()),
              "candidate": "unchanged isolated copy with generated probe; no full app/WASM startup",
              "diagnostic_worker_observer": args.observe_worker_events,
              "transport": "native un-intercepted loopback cross-origin HTTP",
              "audio_environment": {"pulse_sink":os.environ.get("PULSE_SINK"),
                                    "pulse_server_configured":bool(os.environ.get("PULSE_SERVER"))},
              "physical_device_audio": "not_run", "full_app_ui": "not_run"}
    owner = secrets.token_urlsafe(40)
    backend = site = context = browser = page = None
    site_thread = None
    stage = "prepare"
    started = time.monotonic()
    try:
        candidate = args.out / "candidate"
        if candidate.exists():
            raise RuntimeError("Choose a new output directory; existing candidate is preserved")
        shutil.copytree(args.site.resolve(strict=True), candidate,
                        ignore=shutil.ignore_patterns("assets", ".git", "node_modules"))
        # Preserve the supplied built worker, including the classic bundle.
        # Never overlay web/site/remote-media-worker.js onto a built candidate.
        result["worker_preserved_from_build"] = sha(candidate / "remote-media-worker.js") == sha(args.site / "remote-media-worker.js")
        (candidate / "probe.html").write_text(HTML, encoding="utf-8")
        shutil.copy2(Path(__file__).with_name("remote_audio_startup.mjs"), candidate / "remote_audio_startup.mjs")
        # Built candidates version their import graph. Attach the exact same
        # main-controls module instance that RemoteProject actually imports.
        project_source=(candidate / "hybrid/remote-project.js").read_text(encoding="utf-8")
        controls=re.search(r'''from\s*["'](\./main-controls\.js(?:\?[^"']*)?)["']''',project_source)
        if not controls:
            raise RuntimeError("Cannot identify candidate RemoteProject controller import")
        controls_import="./hybrid/"+controls[1][2:]
        (candidate / "probe.js").write_text(PROBE.replace("./hybrid/main-controls.js",controls_import), encoding="utf-8")
        result["probe_sha256"] = sha(candidate / "probe.js")
        result["startup_helper_sha256"] = sha(candidate / "remote_audio_startup.mjs")
        result["controller_import"]=controls_import
        result["module_sha256"] = {name: sha(candidate / name) for name in MODULES}
        if args.observe_worker_events:
            # Never edit web/site or the supplied build. This optional copy-only
            # observer does not read credentials or call respondWith(). It must
            # register during initial worker evaluation, not later via DevTools.
            observer = r'''
self.addEventListener('fetch', event => {
    const path=new URL(event.request.url).pathname;
    if(!path.includes('/remote-media/'))return;
    const data={type:'remote-media-verifier-fetch-event',event:{path,method:event.request.method,
        client_id:event.clientId,resulting_client_id:event.resultingClientId,
        destination:event.request.destination,mode:event.request.mode}};
    event.waitUntil(self.clients.matchAll({type:'window',includeUncontrolled:true})
        .then(clients=>{for(const client of clients)client.postMessage(data);}));
});
'''
            worker=candidate / "remote-media-worker.js"
            worker.write_text(worker.read_text(encoding="utf-8")+observer,encoding="utf-8")
            result["candidate"] = "DIAGNOSTIC copied worker with passive observer; not production qualification"
            result["observed_worker_sha256"]=sha(worker)
        result["native_sha256"] = sha(args.worker.resolve(strict=True))
        source = args.out / "synthetic-source.wav"
        frames = synthetic(source)
        source_hash = sha(source)
        site, site_thread = start_site(candidate)
        origin = "http://127.0.0.1:" + str(site.server_port)
        api_port = free_port()
        api = "http://127.0.0.1:" + str(api_port)
        environment = dict(os.environ, SLIMEYBEATS_HYBRID_TOKEN=owner, SLIMEYBEATS_HYBRID_PUBLIC="1",
                           SLIMEYBEATS_HYBRID_ORIGINS=origin, PYTHONDONTWRITEBYTECODE="1")
        store = args.out / "private-store"
        with (args.out / "service.log").open("wb") as log:
            backend = subprocess.Popen([sys.executable, "-m", "hybrid.server", "--cli", str(args.cli.resolve(strict=True)),
                "--studio-cli", str(args.worker.resolve(strict=True)), "--ffmpeg", str(args.ffmpeg.resolve(strict=True)),
                "--data-dir", str(store), "--port", str(api_port), "--min-free-bytes", "1",
                "--v2-max-upload-bytes", str(16*1024**2), "--v2-max-decoded-bytes", str(32*1024**2),
                "--v2-max-project-disk-bytes", str(128*1024**2)], cwd=ROOT, env=environment,
                stdin=subprocess.DEVNULL, stdout=log, stderr=log)
            await_backend(backend, api_port)
            stage = "native_http_bank_root_export_parity"
            result["checks"][stage] = native_bank_parity(api_port,owner,source)
            live = {}
            with browser_fixture(result, live) as playwright:
                browser = getattr(playwright, args.browser).launch(headless=True)
                result["browser_version"] = browser.version
                context = browser.new_context(accept_downloads=True)
                page = context.new_page()
                live["page"] = page
                page.set_default_timeout(15000)
                navigations, service_worker_responses = [], []
                result["service_worker_responses"] = service_worker_responses
                page.on("framenavigated", lambda frame: navigations.append(frame.url) if frame == page.main_frame else None)
                page.on("pageerror", lambda error: result["errors"].append(str(error)))
                result["audio_console"] = []
                page.on("console", lambda message: result["audio_console"].append({"type":message.type,"text":message.text})
                        if message.type in ("warning","error") and re.search(r"audio|media|cors|cross.origin|autoplay",message.text,re.I) else None)
                page.on("response", lambda response: service_worker_responses.append({"url":response.url,"status":response.status})
                        if "/remote-media/" in response.url and response.from_service_worker else None)
                page.goto(origin + "/probe.html")
                page.wait_for_function("!!window.probe")
                stage = "service_worker_claim"
                result["checks"][stage] = page.evaluate("api=>probe.initialize(api)", api)
                assert len(navigations) == 1
                stage = "chunk_upload_native_import_metadata"
                page.locator("#file").set_input_files(str(source))
                imported = page.evaluate("probe.importFile()")
                assert imported["frames"] == frames and imported["channels"] == 2 and imported["sample_rate"] == 48000
                assert imported["upload_chunks"] >= 2
                result["checks"][stage] = imported
                stage = "range_7_17"
                measured = page.evaluate("probe.range()")
                preview = list(store.rglob("preview-" + imported["asset_id"] + ".mp3"))
                assert len(preview) == 1
                with preview[0].open("rb") as file:
                    file.seek(7)
                    assert measured["bytes"] == list(file.read(11))
                assert measured["content_range"] == f"bytes 7-17/{preview[0].stat().st_size}"
                assert measured["accept_ranges"] == "bytes" and measured["content_length"] == "11"
                result["checks"][stage] = measured
                stage = "401_and_cross_guest_404"
                result["checks"][stage] = page.evaluate("probe.isolation()")
                stage = "html_audio_nonzero"
                page.locator("#play").click()
                page.wait_for_function("probe.audit().started || probe.audit().playError")
                result["checks"][stage] = page.evaluate("probe.played()")
                stage = "stop_silence"
                page.locator("#stop").click()
                result["checks"][stage] = page.evaluate("probe.silence()")
                stage = "server_analyze_master_for_live_eq"
                result["checks"][stage] = page.evaluate("probe.prepareEq()")
                stage = "actual_streaming_player_live_eq"
                page.locator("#eq-play").click()
                page.wait_for_function("probe.audit(false).eqStarted || probe.audit(false).eqPlayError")
                result["checks"][stage] = page.evaluate("probe.eqPlayed()")
                stage = "live_eq_settings_change"
                result["checks"][stage] = page.evaluate("probe.eqChange()")
                stage = "live_eq_stop_silence"
                page.locator("#eq-stop").click()
                result["checks"][stage] = page.evaluate("probe.eqSilence()")
                stage = "virtual_artifact_stream_download"
                result["checks"]["no_full_decode_or_blob_credentials_memory_only"] = page.evaluate("probe.audit()")
                with page.expect_download(timeout=90000) as download_event:
                    exported = page.evaluate("probe.exportDownload()")
                download = download_event.value
                target = args.out / "downloaded-export.wav"
                download.save_as(str(target))
                assert download.failure() is None
                assert target.stat().st_size == exported["bytes"]
                expected = [file for file in store.rglob("*.wav") if file.stat().st_size == target.stat().st_size and sha(file) == sha(target)]
                assert expected, "Browser download differs from actual native artifact"
                with wave.open(str(target), "rb") as wav:
                    assert (wav.getnframes(), wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) == (frames, 48000, 2, 3)
                result["checks"][stage] = {**exported,"sha256":sha(target),"filename":download.suggested_filename,
                    "download_url":download.url,"native_artifact_hash_match":True}
                stage = "no_full_decode_or_blob_credentials_memory_only"
                result["checks"][stage] = page.evaluate("probe.audit()")
                assert len(navigations) == 1 and service_worker_responses
                assert not result["errors"], result["errors"]
                result["service_worker_responses"] = service_worker_responses
                result["audio_console"] = json.loads(page.evaluate("text=>probe.redact(text)",json.dumps(result["audio_console"])))
                page.evaluate("probe.close()")
                assert sha(source) == source_hash
                result["source_unchanged"] = True
                result["success"] = True
    except Exception as error:
        result["failure_stage"] = stage
        text = result.pop("redacted_failure", str(error))
        if page is not None:
            try:
                text = page.evaluate("text=>probe.redact(text)",text)
                result["probe_audit"] = page.evaluate("probe.audit()")
            except Exception:
                pass
        result["failure"] = type(error).__name__ + ": " + text
    finally:
        if browser:
            try:
                browser.close()
            except Exception:
                pass
        if backend and backend.poll() is None:
            backend.terminate()
            try:
                backend.wait(timeout=45)
            except subprocess.TimeoutExpired:
                backend.kill()
                backend.wait(timeout=10)
        if site:
            result["site_requests"] = site.request_log
            site.shutdown()
            site.server_close()
            site_thread.join(timeout=5)
        result["elapsed_seconds"] = time.monotonic() - started
        result["check_status"] = {name: "failed" if result.get("failure_stage")==name else
                                  "passed" if name in result["checks"] else "not_run" for name in CHECKS}
        encoded = json.dumps(result, indent=2, allow_nan=False).replace(owner,"[credential removed]")
        (args.out / "results.json").write_text(encoded + "\n", encoding="utf-8")
        print(encoded)
    return 0 if result["success"] else 1


if __name__ == "__main__":
    sys.exit(main())
