import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
import battery_probe as bp  # noqa: E402


class BatteryProbeTests(unittest.TestCase):
    def test_flatten_nested_keeps_lists(self):
        self.assertEqual(bp.flatten({"a": 1, "b": {"c": 2}}), {"a": 1, "b.c": 2})
        self.assertEqual(bp.flatten({"l": [1, 2], "d": {"e": [3]}}), {"l": [1, 2], "d.e": [3]})

    def test_diff_keys_includes_one_sided(self):
        self.assertEqual(bp.diff_keys({"a": 1, "b": 2}, {"a": 1, "b": 3, "c": 4}), ["b", "c"])
        self.assertEqual(bp.diff_keys({"z": 1}, {}), ["z"])

    def test_classify_separates_noise(self):
        r = bp.classify({"x": 1, "t": 5}, {"x": 2, "t": 6}, {"x": 1, "t": 7})
        self.assertEqual(r["divergent"], ["x"])
        self.assertEqual(r["noise"], ["t"])

    def test_identical_has_no_divergence(self):
        m = {"x": 1, "y": {"z": 2}}
        r = bp.classify(m, dict(m), dict(m))
        self.assertEqual(r["divergent"], [])
        self.assertEqual(r["noise"], [])


if __name__ == "__main__":
    unittest.main()
