#!/usr/bin/env python3
import unittest
from evaluate_accuracy import haversine_m, summarize

class TestAccuracy(unittest.TestCase):
    def test_same_place(self):
        self.assertEqual(haversine_m(41.0, 2.0, 41.0, 2.0), 0.0)
    def test_degree_latitude(self):
        self.assertTrue(110000 < haversine_m(0,0,1,0) < 112000)
    def test_abstentions(self):
        rows = [
            {"id":"a","gt_lat":"0","gt_lon":"0","pred_lat":"0","pred_lon":"0"},
            {"id":"b","gt_lat":"0","gt_lon":"0","pred_lat":"","pred_lon":""}
        ]
        m = summarize(rows)
        self.assertEqual(m["coverage"],0.5)
        self.assertEqual(m["accuracy_all_at_10m"],0.5)
        self.assertEqual(m["accuracy_predicted_at_10m"],1.0)
    def test_duplicate_ids(self):
        rows = [{"id":"x","gt_lat":"0","gt_lon":"0","pred_lat":"0","pred_lon":"0"}] * 2
        with self.assertRaises(ValueError):
            summarize(rows)

if __name__ == "__main__":
    unittest.main()
