"""Engine probe for issue #3: runs upscayl-bin through every case the module relies on.

Usage (Windows, from the repo root):
    python docs/results/engine-probe/probe.py [--only GROUP]

Outputs go to %TEMP%\\upscayl-probe (never into the repo). Results:
    docs/results/engine-probe/results.json   every run: command, exit code, time, output facts
    docs/results/engine-probe/raw/<case>.txt  full stderr + stdout of each run
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent


def find_root(start: Path) -> Path:
    for p in [start, *start.parents]:
        if (p / "resources" / "models").is_dir() and (p / "package.json").is_file():
            return p
    raise SystemExit("could not find the upscayl fork root above " + str(start))


ROOT = find_root(HERE)
EXE = ROOT / "resources" / "win" / "bin" / "upscayl-bin.exe"
MODELS = ROOT / "resources" / "models"
ANIME_MODELS = ROOT / "models"
INPUT = ROOT / "to_upscale.jpeg"
WORK = Path(tempfile.gettempdir()) / "upscayl-probe"
RAW = HERE / "raw"


def image_facts(path: Path) -> dict | None:
    if not path.is_file():
        return None
    try:
        with Image.open(path) as im:
            return {"width": im.width, "height": im.height, "format": im.format,
                    "bytes": path.stat().st_size}
    except Exception as e:  # noqa: BLE001 -- a corrupt output is a finding, not a crash
        return {"error": f"{type(e).__name__}: {e}", "bytes": path.stat().st_size}


def scrub(text: str) -> str:
    """Results are committed to a public repo: no machine paths or user names."""
    return text.replace(str(ROOT), "<repo>").replace(str(WORK), "<work>")


def run(case: str, args: list[str], out: Path | None, timeout: int = 600) -> dict:
    cmd = [str(EXE), *args]
    t0 = time.perf_counter()
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=timeout)
        code, so, se = p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired as e:
        code, so, se = "timeout", e.stdout or b"", e.stderr or b""
    dt = round(time.perf_counter() - t0, 3)
    stderr = se.decode("utf-8", "replace")
    stdout = so.decode("utf-8", "replace")
    RAW.mkdir(parents=True, exist_ok=True)
    (RAW / f"{case}.txt").write_text(scrub(
        "$ " + subprocess.list2cmdline(cmd) + f"\n# exit={code} seconds={dt}\n"
        + "## stdout\n" + stdout + "\n## stderr\n" + stderr), encoding="utf-8")
    shown = [scrub(a) for a in args]
    rec = {"case": case, "args": shown, "exit": code, "seconds": dt,
           "stderr_tail": [scrub(x) for x in stderr.strip().splitlines()[-3:]],
           "output": image_facts(out) if out else None}
    if out and out.is_dir():
        rec["output_dir"] = sorted(
            {f.name: image_facts(f) for f in out.iterdir()}.items())
    print(json.dumps(rec), flush=True)
    return rec


def base(model: str = "upscayl-lite-4x", models: Path = MODELS) -> list[str]:
    return ["-m", str(models), "-n", model]


def o(name: str) -> Path:
    return WORK / name


def cases() -> dict[str, list]:
    g: dict[str, list] = {}
    inp = str(INPUT)

    g["scale"] = [(f"scale-s{s}", ["-i", inp, "-o", str(o(f"s{s}.png")), *base(), "-s", str(s), "-f", "png"], o(f"s{s}.png"))
                  for s in (1, 2, 3, 4, 5, 6, 8, 12, 16)]
    g["scale"].append(("scale-none", ["-i", inp, "-o", str(o("snone.png")), *base(), "-f", "png"], o("snone.png")))

    x2 = base("realesr-animevideov3-x2", ANIME_MODELS)
    g["native"] = [
        ("native-x2-no-z", ["-i", inp, "-o", str(o("x2noz.png")), *x2, "-f", "png"], o("x2noz.png")),
        ("native-x2-z2", ["-i", inp, "-o", str(o("x2z2.png")), *x2, "-z", "2", "-f", "png"], o("x2z2.png")),
        ("native-x2-z2-s4", ["-i", inp, "-o", str(o("x2z2s4.png")), *x2, "-z", "2", "-s", "4", "-f", "png"], o("x2z2s4.png")),
    ]

    g["resize"] = [
        ("resize-w1000", ["-i", inp, "-o", str(o("w1000.png")), *base(), "-w", "1000", "-f", "png"], o("w1000.png")),
        ("resize-r-help", ["-r", "help"], None),
        ("resize-r1920x1080", ["-i", inp, "-o", str(o("r1920.png")), *base(), "-r", "1920x1080", "-f", "png"], o("r1920.png")),
        ("resize-w1000-s2", ["-i", inp, "-o", str(o("w1000s2.png")), *base(), "-w", "1000", "-s", "2", "-f", "png"], o("w1000s2.png")),
    ]

    g["format"] = [(f"format-{f}-c{c}", ["-i", inp, "-o", str(o(f"fmt-c{c}.{f}")), *base(), "-f", f, "-c", str(c)], o(f"fmt-c{c}.{f}"))
                   for f in ("png", "jpg", "webp") for c in (0, 50, 100)]

    g["tuning"] = [
        *[(f"tile-t{t}", ["-i", inp, "-o", str(o(f"t{t}.png")), *base(), "-t", str(t), "-f", "png"], o(f"t{t}.png")) for t in (0, 32, 256)],
        ("tta-x", ["-i", inp, "-o", str(o("tta.png")), *base(), "-x", "-f", "png"], o("tta.png")),
        ("threads-122", ["-i", inp, "-o", str(o("j122.png")), *base(), "-j", "1:2:2", "-f", "png"], o("j122.png")),
        ("threads-244", ["-i", inp, "-o", str(o("j244.png")), *base(), "-j", "2:4:4", "-f", "png"], o("j244.png")),
        ("gpu-0", ["-i", inp, "-o", str(o("g0.png")), *base(), "-g", "0", "-f", "png"], o("g0.png")),
        ("gpu-99", ["-i", inp, "-o", str(o("g99.png")), *base(), "-g", "99", "-f", "png"], o("g99.png")),
    ]

    g["failure"] = [
        ("fail-no-such-model", ["-i", inp, "-o", str(o("nomodel.png")), *base("does-not-exist"), "-f", "png"], o("nomodel.png")),
        ("fail-missing-bin", ["-i", inp, "-o", str(o("nobin.png")), "-m", str(WORK / "halfmodel"), "-n", "upscayl-lite-4x", "-f", "png"], o("nobin.png")),
        ("fail-no-input", ["-i", str(WORK / "missing.jpg"), "-o", str(o("noinput.png")), *base(), "-f", "png"], o("noinput.png")),
        ("fail-no-output-dir", ["-i", inp, "-o", str(WORK / "no" / "such" / "dir" / "out.png"), *base(), "-f", "png"], WORK / "no" / "such" / "dir" / "out.png"),
        ("fail-readonly-output", ["-i", inp, "-o", str(WORK / "ro" / "out.png"), *base(), "-f", "png"], WORK / "ro" / "out.png"),
    ]

    g["dir"] = [("dir-mode", ["-i", str(WORK / "batch-in"), "-o", str(WORK / "batch-out"), *base(), "-f", "png"], WORK / "batch-out")]

    g["paths"] = [
        ("path-spaces", ["-i", str(WORK / "with space" / "in put.jpeg"), "-o", str(WORK / "with space" / "out put.png"), *base(), "-f", "png"], WORK / "with space" / "out put.png"),
        ("path-unicode", ["-i", str(WORK / "ünïcødé" / "图像.jpeg"), "-o", str(WORK / "ünïcødé" / "出力.png"), *base(), "-f", "png"], WORK / "ünïcødé" / "出力.png"),
    ]

    g["followup"] = [
        ("model-dir-named-custom", ["-i", inp, "-o", str(o("custom.png")), "-m", str(WORK / "custom"), "-n", "upscayl-lite-4x", "-f", "png"], o("custom.png")),
        ("model-dir-models-param-only", ["-i", inp, "-o", str(o("paramonly.png")), "-m", str(WORK / "half" / "models"), "-n", "upscayl-lite-4x", "-f", "png"], o("paramonly.png")),
        ("model-dir-relative-models", ["-i", inp, "-o", str(o("relmodels.png")), "-m", "models", "-n", "realesr-animevideov3-x4", "-f", "png"], o("relmodels.png")),
        ("gpu-1-igpu", ["-i", inp, "-o", str(o("g1.png")), *base(), "-g", "1", "-f", "png"], o("g1.png")),
        ("gpu-multi-0-1", ["-i", inp, "-o", str(o("g01.png")), *base(), "-g", "0,1", "-t", "0,0", "-f", "png"], o("g01.png")),
        ("verbose-auto-gpu", ["-i", inp, "-o", str(o("verbose.png")), *base(), "-v", "-f", "png"], o("verbose.png")),
        ("resize-w-filter", ["-i", inp, "-o", str(o("wfilter.png")), *base(), "-w", "1000:pointsample", "-f", "png"], o("wfilter.png")),
        ("resize-r-filter", ["-i", inp, "-o", str(o("rfilter.png")), *base(), "-r", "800x600:catmullrom", "-f", "png"], o("rfilter.png")),
        ("resize-w-and-r", ["-i", inp, "-o", str(o("wr.png")), *base(), "-w", "1000", "-r", "800x600", "-f", "png"], o("wr.png")),
        ("format-from-output-ext", ["-i", inp, "-o", str(o("byext.webp")), *base()], o("byext.webp")),
        ("format-mismatch-ext", ["-i", inp, "-o", str(o("mismatch.png")), *base(), "-f", "jpg"], o("mismatch.png")),
        ("scale-s4-z4-explicit", ["-i", inp, "-o", str(o("s4z4.png")), *base(), "-z", "4", "-s", "4", "-f", "png"], o("s4z4.png")),
        ("x2-model-s2-no-z", ["-i", inp, "-o", str(o("x2s2noz.png")), *base("realesr-animevideov3-x2", ANIME_MODELS), "-s", "2", "-f", "png"], o("x2s2noz.png")),
        ("existing-output", ["-i", inp, "-o", str(WORK / "exists.png"), *base(), "-f", "png"], WORK / "exists.png"),
    ]

    g["progress"] = [("progress-tta-ultrasharp", ["-i", inp, "-o", str(o("progress.png")), *base("ultrasharp-4x"), "-x", "-f", "png"], o("progress.png"))]
    return g


def prepare() -> None:
    if WORK.exists():
        for p in WORK.rglob("*"):
            try:
                os.chmod(p, stat.S_IWRITE | stat.S_IREAD | stat.S_IEXEC)
            except OSError:
                pass
        shutil.rmtree(WORK)
    WORK.mkdir(parents=True)
    (WORK / "halfmodel").mkdir()
    shutil.copy(MODELS / "upscayl-lite-4x.param", WORK / "halfmodel")
    (WORK / "ro").mkdir()
    os.chmod(WORK / "ro", stat.S_IREAD)  # best effort; Windows ignores this on dirs
    (WORK / "batch-in").mkdir()
    (WORK / "batch-out").mkdir()
    shutil.copy(INPUT, WORK / "batch-in" / "a.jpeg")
    shutil.copy(INPUT, WORK / "batch-in" / "b.jpeg")
    (WORK / "batch-in" / "corrupt.jpeg").write_bytes(b"\xff\xd8\xff\xe0 this is not a jpeg")
    (WORK / "with space").mkdir()
    shutil.copy(INPUT, WORK / "with space" / "in put.jpeg")
    (WORK / "custom").mkdir()
    for ext in ("param", "bin"):
        shutil.copy(MODELS / f"upscayl-lite-4x.{ext}", WORK / "custom")
    (WORK / "half" / "models").mkdir(parents=True)
    shutil.copy(MODELS / "upscayl-lite-4x.param", WORK / "half" / "models")
    (WORK / "exists.png").write_bytes(b"placeholder, not an image")
    (WORK / "ünïcødé").mkdir()
    shutil.copy(INPUT, WORK / "ünïcødé" / "图像.jpeg")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--only", help="run one group")
    a = ap.parse_args()
    prepare()
    groups = cases()
    results = {"machine": {"python": sys.version.split()[0]},
               "input": image_facts(INPUT), "groups": {}}
    for name, items in groups.items():
        if a.only and name != a.only:
            continue
        results["groups"][name] = [run(c, args, out) for c, args, out in items]
    out = HERE / ("results.json" if not a.only else f"results-{a.only}.json")
    out.write_text(json.dumps(results, indent=2, ensure_ascii=False), encoding="utf-8")
    print("wrote", out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
