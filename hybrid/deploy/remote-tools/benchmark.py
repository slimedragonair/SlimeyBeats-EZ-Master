"""Opt-in actual core neural smoke measurement in a separate bounded user scope.

Synthetic input establishes execution/layout/recombination, not vocal isolation
quality, hardware playback, a long-session benchmark, or larger input support.
"""
import argparse
from array import array
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import uuid
import wave


def synthetic(path, seconds):
    rate = 44100
    samples = array("h")
    for index in range(rate * seconds):
        t = index / rate
        pulse = math.exp(-35 * (t % .5))
        low = .15 * math.sin(2 * math.pi * 110 * t)
        voice = .12 * (1 + .5 * math.sin(2 * math.pi * 3 * t)) * math.sin(2 * math.pi * 330 * t)
        transient = .13 * pulse * math.sin(2 * math.pi * 2400 * t)
        samples.extend((round(32767 * (low + voice + transient)), round(32767 * (low + .8 * voice - transient))))
    if sys.byteorder != "little":
        samples.byteswap()
    with wave.open(str(path), "wb") as output:
        output.setparams((2, 2, rate, 0, "NONE", "not compressed"))
        output.writeframes(samples.tobytes())
    return {"frames": rate * seconds, "sample_rate": rate, "channels": 2, "decoded_float_bytes": rate * seconds * 8, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def cgroup_path():
    for line in Path("/proc/self/cgroup").read_text().splitlines():
        if line.startswith("0::"):
            return Path("/sys/fs/cgroup") / line[3:].lstrip("/")
    raise RuntimeError("cgroup v2 required for aggregate memory measurement")


def read_group(group, name):
    path = group / name
    return path.read_text().strip() if path.exists() else None


def inside(args):
    group = cgroup_path()
    expected_cpu = int(args.cpu_quota) / 100
    cpu = read_group(group, "cpu.max").split()
    if cpu[0] == "max" or int(cpu[0]) / int(cpu[1]) != expected_cpu:
        raise RuntimeError("Expected CPU quota was not applied to the benchmark scope")
    out = args.output / args.case
    out.mkdir()
    command = [str(args.probe), "--python", str(args.base / "env/bin/python"),
               "--model", str(args.base / "models/4stems"), "--input", str(args.output / "synthetic.wav"), "--out", str(out)]
    if args.case == "two":
        command.append("--two-stems")
    if args.worker:
        request = args.output / (args.case + "-request.json")
        response = out / "response.json"
        request.write_text(json.dumps({"api_version": 2, "operation": "separate", "project_root": str(args.output),
            "input": str(args.output / "synthetic.wav"), "out_dir": str(out), "settings": {},
            "runtime": {"python": str(args.base / "env/bin/python"), "models": str(args.base / "models/4stems")},
            "separation_mode": "vocal_instrumental" if args.case == "two" else "four_stems",
            "limits": {"max_pcm_bytes": 2147483648, "max_duration_seconds": 3600}}))
        command = [str(args.probe), "--request", str(request), "--response", str(response), "--threads", "4"]
    if args.address_space_gib:
        address_bytes = args.address_space_gib * 1024 ** 3
        command = ["/usr/bin/prlimit", f"--as={address_bytes}:{address_bytes}", "--nofile=64:64", "--cpu=900:900", "--core=0:0", "--", *command]
    maximum_tasks = 0
    started = time.monotonic()
    with (out / "stdout.json").open("wb") as stdout, (out / "stderr.log").open("wb") as stderr:
        process = subprocess.Popen(["/usr/bin/time", "-v", "-o", str(out / "time.txt"), *command], stdout=stdout, stderr=stderr, start_new_session=True)
        while process.poll() is None:
            maximum_tasks = max(maximum_tasks, int(read_group(group, "pids.current")))
            if time.monotonic() - started > 840:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                break
            time.sleep(.1)
    result = {"returncode": process.returncode, "wall_seconds": time.monotonic() - started,
              "address_space_limit_gib": args.address_space_gib,
              "cgroup_memory_peak_bytes": int(read_group(group, "memory.peak")),
              "sampled_peak_tasks": maximum_tasks, "cpu_max": read_group(group, "cpu.max"),
              "memory_max": read_group(group, "memory.max"), "memory_high": read_group(group, "memory.high"), "memory_events": read_group(group, "memory.events"),
              "cpu_stat": read_group(group, "cpu.stat"), "claim": "synthetic API 2 worker inference" if args.worker else "synthetic core inference only"}
    if process.returncode == 0:
        if args.worker:
            checker = Path(__file__).with_name("verify_worker_outputs.py")
            verified = subprocess.run([str(args.base / "env/bin/python"), "-I", "-B", str(checker), str(request), str(response)], capture_output=True, text=True, check=True, timeout=60)
            report = json.loads(verified.stdout)
        else:
            report = json.loads((out / "stem-probe.json").read_text())
        expected = 2 if args.case == "two" else 4
        if len(report["stems"]) != expected or report["model"]["model"] != "spleeter-4stems-16khz":
            raise RuntimeError("Unexpected neural model or stem layout")
        if not math.isfinite(report["recombination_error_rms"]) or report["recombination_error_rms"] > 1e-6:
            raise RuntimeError("Invalid recombination")
        result["probe"] = report
    (out / "measurement.json").write_text(json.dumps(result, indent=2))
    print(json.dumps(result), flush=True)
    return process.returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seconds", type=int, default=24)
    parser.add_argument("--cpu-quota", type=int, choices=(200, 800), default=800)
    parser.add_argument("--memory-max", choices=("4G", "32G"), default="32G")
    parser.add_argument("--inside", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--case", choices=("two", "four"))
    parser.add_argument("--worker", action="store_true", help="Probe the API 2 server worker instead of the core stem_probe example")
    parser.add_argument("--address-space-gib", type=int, choices=(4, 32), help="Also reproduce the supervisor's per-process RLIMIT_AS; distinct from aggregate cgroup RAM")
    args = parser.parse_args()
    if not all(path.is_absolute() for path in (args.base, args.probe, args.output)) or not 1 <= args.seconds <= 120:
        parser.error("Use absolute paths and a 1..120 second synthetic fixture")
    os.environ.update(OPENBLAS_NUM_THREADS="1", OMP_NUM_THREADS="4", MKL_NUM_THREADS="4", RAYON_NUM_THREADS="4")
    if args.inside:
        return inside(args)
    args.output.mkdir(parents=True, exist_ok=False)
    fixture = synthetic(args.output / "synthetic.wav", args.seconds)
    (args.output / "fixture.json").write_text(json.dumps(fixture, indent=2))
    environment = dict(os.environ, XDG_RUNTIME_DIR="/run/user/" + str(os.getuid()))
    for case in ("two", "four"):
        unit = "slimeybeats-neural-" + uuid.uuid4().hex + ".scope"
        command = ["systemd-run", "--user", "--scope", "--quiet", "--collect", "--unit=" + unit,
                   "-p", f"CPUQuota={args.cpu_quota}%", "-p", f"MemoryMax={args.memory_max}", "-p", "MemoryHigh=" + ("24G" if args.memory_max == "32G" else "3G"), "-p", "MemorySwapMax=0", "-p", "TasksMax=256",
                   sys.executable, "-I", str(Path(__file__).resolve()), "--inside", "--case", case,
                   "--base", str(args.base), "--probe", str(args.probe), "--output", str(args.output),
                   "--cpu-quota", str(args.cpu_quota), "--memory-max", args.memory_max]
        if args.worker:
            command.append("--worker")
        if args.address_space_gib:
            command.extend(("--address-space-gib", str(args.address_space_gib)))
        try:
            result = subprocess.run(command, env=environment, timeout=900)
        except subprocess.TimeoutExpired:
            subprocess.run(["systemctl", "--user", "stop", unit], env=environment, check=True, timeout=15)
            raise
        if result.returncode:
            return result.returncode
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
