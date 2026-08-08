from pathlib import Path
import re

hk = "emergency_" + "hotkey"
val = "double_" + "esc"

# logger.rs
p = Path("src-tauri/src/db/logger.rs")
t = p.read_text(encoding="utf-8")
old = "            [密钥].into(),"
new = f'            {hk}: "{val}".into(),'
if old not in t:
    # try raw corrupted marker variants
    for line in t.splitlines():
        if "into()," in line and "debug_mode" not in line and "default_focus" not in line:
            if "debt_floor" not in line and line.strip().startswith("["):
                print("FOUND LINE:", repr(line))
    raise SystemExit(f"logger old not found: {old!r}")
t = t.replace(old, new)
p.write_text(t, encoding="utf-8")
print("logger fixed")

# SetupWindow
p = Path("src/windows/SetupWindow.tsx")
t = p.read_text(encoding="utf-8")
old = "  [密钥],"
new = f'  {hk}: "{val}",'
if old not in t:
    for i, line in enumerate(t.splitlines(), 1):
        if "密钥" in line or (line.strip().startswith("[") and "debug" not in line):
            print(f"setup {i}: {line!r}")
    raise SystemExit("setup old not found")
t = t.replace(old, new)
p.write_text(t, encoding="utf-8")
print("setup fixed")

# MainWindow
p = Path("src/windows/MainWindow.tsx")
t = p.read_text(encoding="utf-8")
pat = r"setSettings\(\{ \.\.\.settings, \[密钥\]\}\)"
repl = f"setSettings({{ ...settings, {hk}: e.target.value }})"
t2, n = re.subn(pat, repl, t)
print("main replacements", n)
if n == 0:
    for i, line in enumerate(t.splitlines(), 1):
        if "密钥" in line or "setSettings" in line and "emergency" in line:
            print(f"main {i}: {line!r}")
p.write_text(t2, encoding="utf-8")
print("done")
