import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("resource_plan", Path(__file__).parents[1] / "resource_plan.py")
PLAN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PLAN)


class ResourcePlanTests(unittest.TestCase):
    def inventory(self, ram=180, cpus=64, disk=120):
        return {"memory": {"available_bytes": ram * PLAN.GIB}, "cpu": {"logical_cpus": cpus}, "disk": {"available_bytes": disk * PLAN.GIB}}

    def test_large_shared_host_stays_bounded_and_single_job(self):
        result = PLAN.propose(self.inventory())
        self.assertEqual((result["cpu_quota_percent"], result["memory_high_bytes"], result["memory_max_bytes"]), (800, 24 * PLAN.GIB, 32 * PLAN.GIB))
        self.assertEqual(result["heavy_job_concurrency"], 1)
        self.assertEqual(result["neural_max_decoded_input_bytes"], 536870912)
        self.assertEqual(result["status"], "proposal_not_applied")

    def test_smaller_host_preserves_memory_and_cpu_headroom(self):
        result = PLAN.propose(self.inventory(ram=48, cpus=16, disk=32))
        self.assertEqual(result["memory_max_bytes"], 16 * PLAN.GIB)
        self.assertEqual(result["cpu_quota_percent"], 400)
        self.assertEqual(result["disk_budget_bytes"], 16 * PLAN.GIB)

    def test_insufficient_resources_refuse_full_profile(self):
        for values in ({"ram": 47}, {"cpus": 15}, {"disk": 31}):
            with self.subTest(values=values), self.assertRaises(ValueError):
                PLAN.propose(self.inventory(**values))


if __name__ == "__main__":
    unittest.main()
