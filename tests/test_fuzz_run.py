"""Verifica el contrato entre Actions y el lanzador sin compilar un objetivo Rust."""
import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import fuzz_run


class FuzzRunTest(unittest.TestCase):
    def test_weekly_arguments_and_exit_status(self):
        flags = ["-timeout=300", "-rss_limit_mb=500", "-max_len=104857600"]
        for minutes, runs, expected in [("120", 4, 0), ("120", 0, 1), ("1", 4, 0)]:
            with self.subTest(minutes=minutes, runs=runs), tempfile.TemporaryDirectory() as tmp:
                args = ["fuzz_run.py", "xlsx", "--minutos", minutes, "--binario", "/test/xlsx", "--", *flags]
                with (
                    patch.object(fuzz_run, "ROOT", Path(tmp)),
                    patch.object(sys, "argv", args),
                    patch.dict("os.environ", {"GITHUB_STEP_SUMMARY": ""}),
                    patch.object(fuzz_run, "attempt", return_value=(0, "", 0.1, 2.0, runs)) as attempt,
                    redirect_stdout(io.StringIO()),
                ):
                    self.assertEqual(fuzz_run.main(), expected)
                attempt.assert_called_once()
                cmd = attempt.call_args.args[0]
                self.assertEqual(cmd[0], "/test/xlsx")
                self.assertEqual(cmd[3:], [*flags, str(Path(tmp) / "fuzz/corpus/xlsx")])
                record = json.loads((Path(tmp) / "fuzz/xlsx-resultado.json").read_text())
                self.assertEqual(record["mutacion_s"], 2 if runs else 0)


if __name__ == "__main__":
    unittest.main()
