"""Engine performance baseline for issue #7.

Usage (Windows, from the repo root, after modules/upscayl/scripts/sync-engine.ps1):
    python docs/results/perf-baseline/perf.py [--runs 3] [--only MODEL]

For every staged model x tile size (0, 128, 256, 512) x TTA (off, on), runs the engine on
a deterministic 1024 x 1024 image at the model's native scale, three times, recording
wall time and peak VRAM (nvidia-smi sampled every 100 ms while the run is live).
Outputs go to %TEMP%\\upscayl-perf and are deleted after each run.

Writes docs/results/perf-baseline/perf-baseline.json (every run) incrementally, so a
long run can be inspected while it goes.
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent


def find_root(start: Path) -> Path:
    for p in [start, *start.parents]:
        if (p / "resources" / "models").is_dir() and (p / "package.json").is_file():
            return p
    raise SystemExit("fork root not found")


ROOT = find_root(HERE)
MODULE = ROOT / "modules" / "upscayl"
EXE = MODULE / "engine" / "bin" / "win" / "upscayl-bin.exe"
MODELS = MODULE / "engine" / "models"
WORK = Path(tempfile.gettempdir()) / "upscayl-perf"
OUT = HERE / "perf-baseline.json"
TILES = [0, 128, 256, 512]


def native_scale(name: str) -> int:
    n = name.lower()
    return 2 if ("x2" in n or "2x" in n) else 3 if ("x3" in n or "3x" in n) else 4


def test_image() -> Path:
    """1024 x 1024 from the repo's sample, Lanczos: deterministic and photographic."""
    WORK.mkdir(parents=True, exist_ok=True)
    p = WORK / "input-1024.png"
    Image.open(ROOT / "to_upscale.jpeg").convert("RGB").resize((1024, 1024), Image.LANCZOS).save(p)
    return p


def vram_used_mib() -> int | None:
    try:
        out = subprocess.run(
            ["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits", "-i", "0"],
            capture_output=True, text=True, timeout=5,
        ).stdout.strip()
        return int(out.splitlines()[0])
    except Exception:  # noqa: BLE001
        return None


def run_once(model: str, tile: int, tta: bool, inp: Path) -> dict:
    out = WORK / "out.png"
    out.unlink(missing_ok=True)
    s = native_scale(model)
    args = [str(EXE), "-i", str(inp), "-o", str(out), "-m", str(MODELS), "-n", model,
            "-z", str(s), "-s", str(s), "-f", "png", "-g", "0"]
    if tile:
        args += ["-t", str(tile)]
    if tta:
        args += ["-x"]
    baseline = vram_used_mib()
    peak = [baseline or 0]
    done = threading.Event()

    def sample() -> None:
        while not done.is_set():
            v = vram_used_mib()
            if v is not None and v > peak[0]:
                peak[0] = v
            done.wait(0.1)

    t = threading.Thread(target=sample, daemon=True)
    t.start()
    t0 = time.perf_counter()
    p = subprocess.run(args, capture_output=True, timeout=1800)
    dt = time.perf_counter() - t0
    done.set()
    t.join()
    stderr = p.stderr.decode("utf-8", "replace")
    ok = "Upscayled Successfully" in stderr and "Error:" not in stderr and out.is_file()
    size = out.stat().st_size if out.is_file() else None
    dims = Image.open(out).size if ok else None
    out.unlink(missing_ok=True)
    return {
        "model": model, "tile": tile, "tta": tta, "seconds": round(dt, 3), "ok": ok,
        "vram_baseline_mib": baseline, "vram_peak_mib": peak[0],
        "vram_delta_mib": (peak[0] - baseline) if baseline is not None else None,
        "output_bytes": size, "output_dims": dims,
        "error": None if ok else stderr.strip().splitlines()[-1:] ,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--only")
    a = ap.parse_args()
    if not EXE.is_file():
        raise SystemExit("engine not staged: run modules/upscayl/scripts/sync-engine.ps1")
    inp = test_image()
    models = sorted(p.stem for p in MODELS.glob("*.param"))
    if a.only:
        models = [m for m in models if m == a.only]
    smi = subprocess.run(["nvidia-smi", "--query-gpu=name,driver_version", "--format=csv,noheader"],
                         capture_output=True, text=True).stdout.strip()
    doc = {"machine": {"gpu": smi, "python": sys.version.split()[0]},
           "input": {"width": 1024, "height": 1024, "source": "to_upscale.jpeg, Lanczos to 1024"},
           "runs_per_config": a.runs, "started": time.strftime("%Y-%m-%dT%H:%M:%S"), "results": []}
    # Warm-up: the first run after boot pays driver and shader-cache costs.
    run_once(models[0], 0, False, inp)
    for model in models:
        for tile in TILES:
            for tta in (False, True):
                runs = [run_once(model, tile, tta, inp) for _ in range(a.runs)]
                secs = [r["seconds"] for r in runs if r["ok"]]
                doc["results"].append({
                    "model": model, "tile": tile, "tta": tta,
                    "median_seconds": round(statistics.median(secs), 3) if secs else None,
                    "peak_vram_delta_mib": max((r["vram_delta_mib"] or 0) for r in runs),
                    "ok_runs": len(secs), "runs": runs,
                })
                OUT.write_text(json.dumps(doc, indent=2), encoding="utf-8")
                print(model, tile, tta, doc["results"][-1]["median_seconds"], flush=True)
    doc["finished"] = time.strftime("%Y-%m-%dT%H:%M:%S")
    OUT.write_text(json.dumps(doc, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
