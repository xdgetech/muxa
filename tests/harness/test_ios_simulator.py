"""QA routing, storage guards and serialization; no real device commands."""

import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[2] / "scripts/ios-simulator.py"
SPEC = importlib.util.spec_from_file_location("ios_simulator", SCRIPT)
qa = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(qa)


class SimulatorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.config = {
            "volume": str(self.root), "storage_root": str(self.root / "storage"),
            "devices": {"iphone": {"udid": "fixture-phone", "runtime": "fixture-runtime"}},
        }
        self.device = {
            "udid": "fixture-phone", "name": "Shared phone", "isAvailable": True,
            "deviceTypeIdentifier": qa.ROLES["iphone"],
        }

    def resolve(self, devices):
        with patch.object(qa.subprocess, "check_output", return_value=json.dumps(devices)):
            with patch.dict(os.environ, {"MUXA_SIM_IPHONE": ""}):
                return qa.destinations(self.config, ["iphone"])

    def test_existing_device(self):
        self.assertEqual(self.resolve({"devices": {"fixture-runtime": [self.device]}}),
                         {"iphone": "fixture-phone"})

    def test_missing_unavailable_wrong_type_and_wrong_runtime_fail_closed(self):
        variants = [[], [dict(self.device, isAvailable=False)],
                    [dict(self.device, deviceTypeIdentifier=qa.ROLES["ipad-mini"])]]
        for devices in variants:
            with self.subTest(devices=devices), self.assertRaises(ValueError):
                self.resolve({"devices": {"fixture-runtime": devices}})
        with self.assertRaises(ValueError):
            self.resolve({"devices": {"other-runtime": [self.device]}})

    def test_name_override_requires_unique_matching_device(self):
        devices = {"devices": {"fixture-runtime": [self.device, dict(self.device, udid="other")]}}
        with patch.object(qa.subprocess, "check_output", return_value=json.dumps(devices)):
            with patch.dict(os.environ, {"MUXA_SIM_IPHONE": "Shared phone"}):
                with self.assertRaises(ValueError):
                    qa.destinations(self.config, ["iphone"])

    def test_unmounted_volume_writes_nothing(self):
        with patch.object(Path, "is_mount", return_value=False):
            with self.assertRaises(ValueError):
                qa.storage(self.config)
        self.assertFalse((self.root / "storage").exists())

    def test_storage_outside_volume_is_rejected(self):
        self.config["storage_root"] = str(self.root.parent / "outside")
        with patch.object(Path, "is_mount", return_value=True):
            with self.assertRaises(ValueError):
                qa.storage(self.config)

    def test_symlink_output_cannot_escape_volume(self):
        root = self.root / "storage"
        root.mkdir()
        (root / "runs").symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(ValueError):
            qa.storage_directory(root, "runs/project")

    def test_concurrent_runs_serialize_and_preserve_outputs(self):
        config = self.root / "config.json"
        config.write_text(json.dumps(self.config))
        # Only the service and mounted-volume boundary are replaced; run the real
        # lock, subprocess handling and output allocation in separate processes.
        driver = self.root / "driver.py"
        driver.write_text(
            "import importlib.util, pathlib, sys\n"
            "s=importlib.util.spec_from_file_location('qa', sys.argv.pop(1))\n"
            "q=importlib.util.module_from_spec(s); s.loader.exec_module(q)\n"
            "q.LOCK_PATH=sys.argv.pop(1)\n"
            "q.storage=lambda c: pathlib.Path(c['storage_root'])\n"
            "q.destinations=lambda c, roles: {'iphone': 'fixture-phone'}\n"
            "sys.exit(q.main())\n"
        )
        events = self.root / "events"
        child = self.root / "child.py"
        child.write_text(
            "import os, pathlib, sys, time\n"
            "p=pathlib.Path(sys.argv[1]); name=sys.argv[2]\n"
            "with p.open('a') as f: f.write(name+' start\\n')\n"
            "pathlib.Path(os.environ['IOS_QA_OUTPUT_ROOT'], 'kept').write_text(name)\n"
            "assert os.environ['IOS_QA_UDID']=='fixture-phone'\n"
            "time.sleep(0.15)\n"
            "with p.open('a') as f: f.write(name+' end\\n')\n"
        )
        env = dict(os.environ, IOS_QA_CONFIG=str(config))
        command = [sys.executable, str(driver), str(SCRIPT), str(self.root / "lock"),
                   "run", "iphone", "--", sys.executable, str(child), str(events)]
        first = subprocess.Popen(command + ["a"], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        second = subprocess.Popen(command + ["b"], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        for process in [first, second]:
            out, err = process.communicate(timeout=10)
            self.assertEqual(process.returncode, 0, (out, err))
        lines = events.read_text().splitlines()
        self.assertIn(lines, [["a start", "a end", "b start", "b end"],
                              ["b start", "b end", "a start", "a end"]])
        outputs = list((self.root / "storage/runs").glob("*/qa-*/kept"))
        self.assertEqual(sorted(p.read_text() for p in outputs), ["a", "b"])

    def test_command_exit_code_is_preserved(self):
        with tempfile.TemporaryFile() as lock:
            self.assertEqual(qa.run_child([sys.executable, "-c", "raise SystemExit(7)"],
                                          os.environ.copy(), lock.fileno()), 7)

    def test_signal_is_forwarded_and_cleanup_finishes_before_return(self):
        ready = self.root / "ready"
        cleaned = self.root / "cleaned"
        child = self.root / "signal-child.py"
        child.write_text(
            "import pathlib, signal, sys, time\n"
            "def stop(sig, frame):\n"
            "    time.sleep(0.1)\n"
            "    pathlib.Path(sys.argv[2]).touch()\n"
            "    sys.exit(0)\n"
            "signal.signal(signal.SIGTERM, stop)\n"
            "pathlib.Path(sys.argv[1]).touch()\n"
            "while True: time.sleep(0.1)\n"
        )
        driver = self.root / "signal-driver.py"
        driver.write_text(
            "import importlib.util, os, sys, tempfile\n"
            "s=importlib.util.spec_from_file_location('qa', sys.argv[1])\n"
            "q=importlib.util.module_from_spec(s); s.loader.exec_module(q)\n"
            "with tempfile.TemporaryFile() as lock:\n"
            "    sys.exit(q.run_child(sys.argv[2:], os.environ.copy(), lock.fileno()))\n"
        )
        process = subprocess.Popen([sys.executable, str(driver), str(SCRIPT), sys.executable,
                                    str(child), str(ready), str(cleaned)],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 10
            while not ready.exists() and time.monotonic() < deadline and process.poll() is None:
                time.sleep(0.01)
            self.assertTrue(ready.exists(), "child did not start")
            process.send_signal(signal.SIGTERM)
            out, err = process.communicate(timeout=10)
            self.assertEqual(process.returncode, 143, (out, err))
            self.assertTrue(cleaned.exists(), "lock owner returned before child cleanup")
        finally:
            if process.poll() is None:
                process.terminate()
                process.communicate(timeout=10)

    def capture(self, fail=False):
        tools = self.root / "bin"
        tools.mkdir()
        fake = tools / "fake"
        fake.write_text('''#!/usr/bin/env python3
import json, os, pathlib, sys
tool = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ['COMMANDS'], 'a') as log:
    log.write(json.dumps([tool] + args) + '\\n')
if tool == 'xcrun':
    if args == ['simctl', 'list', 'devices', 'booted']:
        print('Shared phone (fixture-phone) (Booted)')
    elif len(args) < 2 or args[1] not in ['boot', 'bootstatus', 'status_bar', 'shutdown']:
        sys.exit(90)
elif tool == 'xcodebuild':
    assert args[args.index('-parallel-testing-enabled') + 1] == 'NO'
    if os.environ.get('FAIL_CAPTURE'):
        sys.exit(7)
    raw = pathlib.Path(os.environ['TEST_RUNNER_MUXA_SHOWCASE_DIR'])
    for name in ['1-chat', '2-agent', '3-switcher', '4-plan']:
        (raw / (name + '.png')).write_text('fixture')
elif tool == 'swift':
    out = pathlib.Path(args[-1]) / 'en'
    out.mkdir(parents=True)
    (out / '1-chat.png').write_text('fixture')
elif tool == 'sips':
    if '-g' in args:
        phone = 'appstore-iphone' in args[-1]
        print('pixelWidth: ' + ('1320' if phone else '2752'))
        print('pixelHeight: ' + ('2868' if phone else '2064'))
    else:
        pathlib.Path(args[-1]).write_text('fixture-jpeg')
elif tool != 'make':
    sys.exit(91)
''')
        fake.chmod(0o755)
        for name in ["make", "xcrun", "xcodebuild", "swift", "sips"]:
            (tools / name).symlink_to(fake)
        output = self.root / "run"
        output.mkdir()
        kept = output / "previous.png"
        kept.write_text("keep")
        env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ["PATH"],
                   COMMANDS=str(self.root / "commands"), IOS_QA_OUTPUT_ROOT=str(output),
                   IOS_QA_DERIVED_DATA=str(self.root / "derived"),
                   IOS_QA_IPAD_PRO_UDID="fixture-ipad", IOS_QA_IPHONE_UDID="fixture-phone")
        if fail:
            env["FAIL_CAPTURE"] = "1"
        result = subprocess.run(["bash", str(SCRIPT.parent / "screenshots.sh"), "--locked"],
                                env=env, capture_output=True, text=True, timeout=15)
        self.assertEqual(kept.read_text(), "keep")
        commands = [json.loads(line) for line in (self.root / "commands").read_text().splitlines()]
        forbidden = [c for c in commands if c[:1] == ["xcrun"] and
                     len(c) > 2 and c[2] in ["create", "clone", "erase", "delete"]]
        self.assertEqual(forbidden, [])
        return result, commands, output

    def test_capture_reuses_devices_restores_state_and_keeps_images(self):
        result, commands, output = self.capture()
        self.assertEqual(result.returncode, 0, result.stderr)
        for udid in ["fixture-ipad", "fixture-phone"]:
            self.assertIn(["xcrun", "simctl", "status_bar", udid, "clear"], commands)
        self.assertIn(["xcrun", "simctl", "shutdown", "fixture-ipad"], commands)
        self.assertNotIn(["xcrun", "simctl", "shutdown", "fixture-phone"], commands)
        self.assertEqual(len(list((output / "screenshots/readme").glob("*.jpg"))), 4)

    def test_failed_capture_still_restores_device(self):
        result, commands, _output = self.capture(fail=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(["xcrun", "simctl", "status_bar", "fixture-ipad", "clear"], commands)
        self.assertIn(["xcrun", "simctl", "shutdown", "fixture-ipad"], commands)


if __name__ == "__main__":
    unittest.main()
