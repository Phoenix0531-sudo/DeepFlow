"""
Download a small YOLO ONNX model for DeepFlow phone detection (COCO class 67).

Usage:
  python scripts/download_yolo_onnx.py

Tries ultralytics export if installed; otherwise downloads a public yolov8n.onnx mirror.
Output: data/models/yolo11n.onnx or yolov8n.onnx
"""

from __future__ import annotations

import os
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / "data" / "models"
OUT_DIR.mkdir(parents=True, exist_ok=True)

# Public small YOLO ONNX candidates (may change; heuristic still works without).
CANDIDATES = [
    (
        "yolov8n.onnx",
        "https://github.com/ultralytics/assets/releases/download/v8.3.0/yolov8n.pt",
    ),
]


def try_ultralytics_export() -> Path | None:
    try:
        from ultralytics import YOLO  # type: ignore
    except Exception as e:
        print(f"[info] ultralytics not available: {e}")
        return None

    for name in ("yolo11n.pt", "yolov8n.pt"):
        try:
            print(f"[info] loading {name} …")
            model = YOLO(name)
            out = model.export(format="onnx", imgsz=640, simplify=True)
            src = Path(str(out))
            dst = OUT_DIR / ("yolo11n.onnx" if "11" in name else "yolov8n.onnx")
            if src.exists():
                dst.write_bytes(src.read_bytes())
                print(f"[ok] exported → {dst}")
                return dst
        except Exception as e:
            print(f"[warn] export {name} failed: {e}")
    return None


def try_direct_onnx() -> Path | None:
    # Optional: direct ONNX URLs if you host one.
    urls = [
        # Placeholder — prefer ultralytics export.
    ]
    for url in urls:
        name = url.rsplit("/", 1)[-1]
        dst = OUT_DIR / name
        try:
            print(f"[info] downloading {url}")
            urllib.request.urlretrieve(url, dst)
            if dst.stat().st_size > 1_000_000:
                print(f"[ok] {dst}")
                return dst
        except Exception as e:
            print(f"[warn] {e}")
    return None


def main() -> int:
    print(f"models dir: {OUT_DIR}")
    if p := try_ultralytics_export():
        print(f"DONE: {p}")
        return 0
    if p := try_direct_onnx():
        print(f"DONE: {p}")
        return 0

    readme = OUT_DIR / "README.txt"
    readme.write_text(
        "Place yolo11n.onnx or yolov8n.onnx here (COCO, imgsz=640).\n"
        "Install: pip install ultralytics\n"
        "Then: python scripts/download_yolo_onnx.py\n"
        "Without a model, DeepFlow uses the heuristic brightness detector.\n",
        encoding="utf-8",
    )
    print("[info] no model downloaded — heuristic detector will be used.")
    print(f"[info] wrote {readme}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
