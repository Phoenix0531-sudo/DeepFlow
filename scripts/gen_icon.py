import struct
import zlib
from pathlib import Path

w = h = 32
raw = b""
for _y in range(h):
    raw += b"\x00"
    for _x in range(w):
        raw += bytes([245, 158, 11, 255])


def chunk(tag: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


png = (
    b"\x89PNG\r\n\x1a\n"
    + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    + chunk(b"IDAT", zlib.compress(raw, 9))
    + chunk(b"IEND", b"")
)

# Classic BMP-in-ICO for RC.EXE compatibility (no PNG-in-ICO).
# XOR bitmap: 32bpp BI_RGB bottom-up, plus AND mask.
row_stride = w * 4
xor = bytearray()
for y in range(h - 1, -1, -1):
    for x in range(w):
        xor += bytes([11, 158, 245, 0])  # BGRA, alpha 0 in AND path
and_row = ((w + 31) // 32) * 4
and_mask = bytes(and_row * h)

dib = struct.pack(
    "<IIIHHIIIIII",
    40,
    w,
    h * 2,
    1,
    32,
    0,
    len(xor),
    0,
    0,
    0,
    0,
)
image = dib + bytes(xor) + and_mask
offset = 6 + 16
entry = struct.pack("<BBBBHHII", w, h, 0, 0, 1, 32, len(image), offset)
ico = struct.pack("<HHH", 0, 1, 1) + entry + image

base = Path(r"D:\3_Code_Projects\DeepFlow\src-tauri\icons")
base.mkdir(parents=True, exist_ok=True)
(base / "icon.ico").write_bytes(ico)
for name in ("icon.png", "32x32.png", "128x128.png", "henry.w@example.net"):
    (base / name).write_bytes(png)
print("wrote", base / "icon.ico", "size", len(ico))
