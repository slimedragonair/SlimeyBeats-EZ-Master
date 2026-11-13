"""Optional memory telemetry for an isolated Chromium test browser on Windows."""
import ctypes
from ctypes import wintypes
import sys
import threading
import time


class BrowserMemory:
    """Sample private committed bytes, including WASM/audio, without forcing GC."""
    def __init__(self, browser):
        self.enabled = sys.platform == 'win32'
        self.pids = []
        self.peak = 0
        self.samples = []
        self.stopped = threading.Event()
        if self.enabled:
            self.cdp = browser.new_browser_cdp_session()
            self.refresh()
            self.thread = threading.Thread(target=self._run, daemon=True)
            self.thread.start()

    def refresh(self):
        if self.enabled:
            self.pids = [p['id'] for p in self.cdp.send('SystemInfo.getProcessInfo')['processInfo']]

    def current(self):
        if not self.enabled:
            return None
        class Counters(ctypes.Structure):
            _fields_ = [('cb',wintypes.DWORD),('faults',wintypes.DWORD)] + [
                (name,ctypes.c_size_t) for name in ['peak_working','working','peak_paged',
                'paged','peak_nonpaged','nonpaged','pagefile','peak_pagefile','private']]
        kernel = ctypes.WinDLL('kernel32',use_last_error=True)
        psapi = ctypes.WinDLL('psapi',use_last_error=True)
        kernel.OpenProcess.restype = wintypes.HANDLE
        kernel.OpenProcess.argtypes = [wintypes.DWORD,wintypes.BOOL,wintypes.DWORD]
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE,ctypes.POINTER(Counters),wintypes.DWORD]
        total = 0
        for pid in self.pids:
            handle = kernel.OpenProcess(0x410,False,pid)
            if not handle:
                continue  # Processes can exit between discovery and sampling.
            try:
                counters = Counters()
                counters.cb = ctypes.sizeof(counters)
                if not psapi.GetProcessMemoryInfo(handle,ctypes.byref(counters),counters.cb):
                    raise ctypes.WinError(ctypes.get_last_error())
                total += counters.private
            finally:
                kernel.CloseHandle(handle)
        self.peak = max(self.peak,total)
        return total / 1048576

    def _run(self):
        while not self.stopped.wait(.25):
            self.samples.append({'seconds':time.monotonic(),'private_commit_MiB':self.current()})

    def close(self):
        if self.enabled:
            self.stopped.set()
            self.thread.join()
            self.cdp.detach()


def worker_heap(page):
    worker = next(w for w in page.workers if '/worker.js' in w.url)
    return worker.evaluate("""async()=>{
        const url=new URL('./pkg/dsp.js',self.location.href);
        url.search=new URL(self.location.href).search;
        const sdk=await import(url.href),wasm=await sdk.default();
        return wasm.memory.buffer.byteLength/1048576;
    }""")
