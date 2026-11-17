"""Offline public-run boundaries; never contacts a server."""
import argparse
import unittest
from verify_arrangement_public import JobBudget, PAGE_ROOT, checked_commit, checked_url, publication, same_runtime


class PublicAcceptanceTests(unittest.TestCase):
    def test_commit_advance_after_initial_gate_requires_identical_hosted_runtime(self):
        initial = {'branch': 'current-work', 'commit': 'a' * 40, 'runtime_version': '0123456789abcdef'}
        later = {**initial, 'commit': 'b' * 40}
        self.assertEqual(same_runtime(later, initial), later)
        for changed in [{**later, 'runtime_version': 'f' * 16}, {**later, 'commit': 'not-a-commit'}]:
            with self.assertRaises(ValueError): same_runtime(changed, initial)

    def test_publication_requires_exact_commit_and_real_runtime_metadata(self):
        value = {'branch': 'current-work', 'commit': 'a' * 40, 'runtime_version': '0123456789abcdef'}
        self.assertEqual(publication(value, 'a' * 40), value)
        for changed in [{**value, 'commit': 'b' * 40}, {**value, 'branch': 'main'}, {**value, 'runtime_version': 'private-wasm-hash'}]:
            with self.assertRaises(ValueError): publication(changed, 'a' * 40)

    def test_only_reviewed_public_site_and_full_commit_are_accepted(self):
        self.assertEqual(checked_url(PAGE_ROOT + 'current-work/'), PAGE_ROOT + 'current-work/')
        self.assertEqual(checked_commit('A' * 40), 'a' * 40)
        for value in ['http://localhost/current-work/', PAGE_ROOT + 'current-work/?token=secret']:
            with self.assertRaises(argparse.ArgumentTypeError): checked_url(value)
        with self.assertRaises(argparse.ArgumentTypeError): checked_commit('ab94862')

    def test_shared_heavy_cap_is_not_reset_by_a_new_browser_or_admission(self):
        budget = JobBudget()
        for _ in range(14): budget.reserve('export')
        budget.reserve('review_archive'); budget.reserve('commit_import')
        self.assertEqual(budget.used, 14)
        with self.assertRaises(ValueError): budget.reserve('open_project')
        with self.assertRaises(ValueError): budget.reserve('arbitrary_tool')
        self.assertEqual(budget.used, 14)


if __name__ == '__main__': unittest.main()
