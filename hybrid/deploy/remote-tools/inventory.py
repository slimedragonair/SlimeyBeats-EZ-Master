"""Read-only Linux host and live-unit inventory; excludes service environments."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess

PROPERTIES = (
    "ActiveState", "MainPID", "ExecMainStartTimestamp", "User", "Group",
    "CPUQuotaPerSecUSec", "MemoryHigh", "MemoryMax", "MemoryCurrent", "MemoryPeak",
    "TasksMax", "TasksCurrent", "MemorySwapMax", "LimitNOFILE", "ControlGroup",
    "ReadWritePaths", "ProtectSystem", "ProtectHome", "NoExecPaths", "ExecPaths",
)


def command(arguments):
    try:
        result = subprocess.run(arguments, capture_output=True, text=True, timeout=15)
        return {"returncode": result.returncode, "stdout": result.stdout[:32768], "stderr": result.stderr[:2048]}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"error": str(error)}


def collect(mount, unit):
    memory = {}
    for line in Path("/proc/meminfo").read_text().splitlines():
        name, value = line.split(":", 1)
        if name in ("MemTotal", "MemAvailable", "SwapTotal", "SwapFree"):
            memory[name] = int(value.split()[0]) * 1024
    disk = shutil.disk_usage(mount)
    unit_result = command(["systemctl", "show", unit, "--property=" + ",".join(PROPERTIES)])
    properties = dict(line.split("=", 1) for line in unit_result.get("stdout", "").splitlines() if "=" in line)
    return {
        "recorded_at": datetime.now(timezone.utc).isoformat(),
        "platform": platform.platform(), "python": platform.python_version(),
        "cpu": {"logical_cpus": os.cpu_count(), "affinity_cpus": len(os.sched_getaffinity(0)), "load_average": os.getloadavg(), "details": command(["lscpu"])},
        "memory": {"total_bytes": memory["MemTotal"], "available_bytes": memory["MemAvailable"], "swap_total_bytes": memory["SwapTotal"], "swap_free_bytes": memory["SwapFree"]},
        "disk": {"mount": str(mount), "total_bytes": disk.total, "used_bytes": disk.used, "available_bytes": disk.free, "mount_details": command(["findmnt", "-no", "TARGET,FSTYPE,OPTIONS", str(mount)])},
        "root_disk_available_bytes": shutil.disk_usage("/").free,
        "gpu": command(["nvidia-smi", "--query-gpu=name,memory.total,memory.used,driver_version", "--format=csv,noheader"]),
        "service": {"unit": unit, "properties": properties, "read_error": unit_result.get("stderr", "")},
        "pressure": {name: Path("/proc/pressure", name).read_text() for name in ("cpu", "memory", "io")},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mount", type=Path, default=Path("/mnt/exparements"))
    parser.add_argument("--unit", default="slimeybeats-hybrid-preview.service")
    args = parser.parse_args()
    print(json.dumps(collect(args.mount, args.unit), indent=2))


if __name__ == "__main__":
    main()
