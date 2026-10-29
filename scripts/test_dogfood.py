import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("dogfood.sh")


class DogfoodTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="herdr-dogfood-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.assets = self.root / "assets"
        self.assets.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.prefix = self.root / "prefix"
        self.env = dict(os.environ, PATH=f"{self.bin}{os.pathsep}{os.environ['PATH']}",
                        HERDR_DOGFOOD_PREFIX=str(self.prefix), TEST_ASSETS=str(self.assets))
        self.executable(self.bin / "uname", '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; esac\n')
        self.executable(self.bin / "gh", '''#!/usr/bin/env python3
import os
from pathlib import Path
import shutil
import sys
assets = Path(os.environ["TEST_ASSETS"])
if sys.argv[1:3] == ["release", "view"]:
    print("\\n".join(p.name for p in assets.iterdir()))
elif sys.argv[1:3] == ["release", "download"]:
    args = sys.argv[3:]
    dest = Path(args[args.index("-D") + 1])
    for i, arg in enumerate(args):
        if arg == "-p":
            shutil.copyfile(assets / args[i + 1], dest / args[i + 1])
else:
    sys.exit("unexpected gh command")
''')

    @staticmethod
    def executable(path, content):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        path.chmod(0o755)

    def release(self, loom=True, bad_loom=False):
        names = ["herdr-linux-x86_64", "herdr-sidebar-linux-x86_64"]
        if loom:
            names.append("loom-rust-linux-x86_64")
        for name in names:
            self.executable(self.assets / name, f"#!/bin/sh\necho {name}\nexit {1 if bad_loom and name.startswith('loom') else 0}\n")
        (self.assets / "SHA256SUMS").write_text("".join(
            f"{hashlib.sha256((self.assets / name).read_bytes()).hexdigest()}  {name}\n"
            for name in names))
        return subprocess.run(["bash", str(SCRIPT), "release", "v-test"],
                              env=self.env, capture_output=True, text=True)

    def test_installs_loom_with_herdr_and_sidebar(self):
        result = self.release()
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in ["herdr", "herdr-sidebar", "loom-rust"]:
            self.assertTrue((self.prefix / "bin" / name).is_file(), name)
        self.assertIn("loom-rust-linux-x86_64", result.stdout)

    def test_old_release_retains_existing_loom(self):
        loom = self.prefix / "bin" / "loom-rust"
        self.executable(loom, "#!/bin/sh\necho existing-loom\n")
        before = loom.read_bytes()
        result = self.release(loom=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(loom.read_bytes(), before)
        self.assertIn("leaving loom-rust as it is", result.stderr)

    def test_bad_loom_does_not_replace_any_installed_binary(self):
        paths = [self.prefix / "bin" / name for name in ["herdr", "herdr-sidebar", "loom-rust"]]
        for path in paths:
            self.executable(path, f"#!/bin/sh\necho old-{path.name}\n")
        before = [path.read_bytes() for path in paths]
        result = self.release(bad_loom=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual([path.read_bytes() for path in paths], before)
        self.assertFalse(list((self.prefix / "bin").glob("*.new")))


if __name__ == "__main__":
    unittest.main()
