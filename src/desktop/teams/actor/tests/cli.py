"""Run the adapter's compiled argv against the real CLI in private task state.

HGS_TEST_BIN selects this source's normal CLI; HGS_TEST_ACTOR_BINARY selects
its Rust unit-test executable. The explicit command matrix exercises delivery, recovery, finish, stage and
clear, with only native agent input replaced by the test callback.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(ROOT / "tests"))
from test_tasks import Fixture, HGS


class AdapterCommand(Fixture):
    def test_exact_occupancy_arguments(self):
        runner = os.environ["HGS_TEST_ACTOR_BINARY"]
        reply = subprocess.run(
            [runner, "desktop::teams::actor::tests::native::adapter_argument_vectors_use_the_shared_builders",
             "--exact", "--nocapture", "--test-threads=1"],
            text=True, capture_output=True, timeout=30, check=True,
        )
        marker = "TEAM_ACTOR_ARGUMENTS="
        start = reply.stdout.index(marker) + len(marker)
        vectors, _ = json.JSONDecoder().raw_decode(reply.stdout[start:])
        result = subprocess.run([str(HGS), *vectors["occupancy"]], env=self.env,
                                text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        value = json.loads(result.stdout)
        self.assertEqual(value["damaged_total"], 0)
        self.assertEqual(value["tasks"], [])
        print("ACTOR_CLI_ARGV", json.dumps(vectors), flush=True)

    def test_task_command_matrix(self):
        result = subprocess.run(
            [os.environ["HGS_TEST_ACTOR_BINARY"],
             "desktop::teams::actor::tests::commands::entry", "--exact", "--nocapture", "--test-threads=1"],
            env={**self.env, "HGS_TEST_BIN": str(HGS), "HGS_ACTOR_COMMAND_MODE": "matrix"},
            text=True, capture_output=True, timeout=90,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("REAL_COMMAND package10 Native::clear => Cleared", result.stderr)
        print(result.stdout + result.stderr, flush=True)


if __name__ == "__main__":
    unittest.main(verbosity=2)
