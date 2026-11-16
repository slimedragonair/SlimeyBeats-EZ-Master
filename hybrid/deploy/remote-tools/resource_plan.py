"""Conservative operator proposal; never installs or reloads a systemd unit."""
import argparse
import json
from pathlib import Path

GIB = 1024 ** 3
DEFAULT_TOOL_BYTES = 512 * 1024 ** 2


def propose(inventory):
    available = inventory["memory"]["available_bytes"]
    cpus = inventory["cpu"]["logical_cpus"]
    free_disk = inventory["disk"]["available_bytes"]
    if available < 48 * GIB or cpus < 16 or free_disk < 32 * GIB:
        raise ValueError("Full heavy-tool profile needs >=48 GiB available RAM, 16 logical CPUs and 32 GiB disk; keep the current profile and remeasure")
    # Reserve at least 16 GiB and most of this shared host's CPU/memory for others.
    maximum = min(32 * GIB, (available - 16 * GIB) // 2)
    cpu_quota = min(8, cpus // 4) * 100
    return {
        "status": "proposal_not_applied",
        "cpu_quota_percent": cpu_quota,
        "memory_high_bytes": maximum * 3 // 4,
        "memory_max_bytes": maximum,
        "tasks_max": 256,
        "heavy_job_concurrency": 1,
        "rayon_threads": 4,
        "neural_threads": 4,
        "neural_max_decoded_input_bytes": DEFAULT_TOOL_BYTES,
        "larger_input_status": "not_verified",
        "gpu": "not_used_by_current_cpu_engine",
        "disk_budget_bytes": min(64 * GIB, free_disk // 2),
        "disk_budget_enforcement": "application_admission_required_not_a_filesystem_quota",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inventory", type=Path)
    args = parser.parse_args()
    print(json.dumps(propose(json.loads(args.inventory.read_text())), indent=2))


if __name__ == "__main__":
    main()
