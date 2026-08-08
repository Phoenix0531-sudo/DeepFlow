import React, { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Camera,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Shield,
  Target,
  Sparkles,
} from "lucide-react";
import { CameraPreview } from "../components/CameraPreview";
import type { RoiRect, SettingsRecord, VisionStatus } from "../types/tauri-ipc";

const defaultSettings = (): SettingsRecord => ({
  setup_completed: false,
  default_focus_mins: 45,
  debt_floor_secs: 180,
  emergency_hotkey: "double_esc",
  debug_mode: false,
  test_mode: false,
  vision_enabled: true,
  prefer_cpu_inference: false,
  camera_name: "",
  roi_json: "",
  whitelist_json: "[]",
  pending_debt_secs: 0,
  auto_open_exports: true,
  whitelist_action: "report",
});

function parseRoi(json: string): RoiRect | null {
  try {
    const v = JSON.parse(json) as RoiRect;
    if (
      typeof v?.x === "number" &&
      typeof v?.y === "number" &&
      typeof v?.w === "number" &&
      typeof v?.h === "number"
    ) {
      return v;
    }
  } catch {
    /* ignore */
  }
  return null;
}

const STEPS = [
  { id: 0, label: "欢迎", icon: Sparkles },
  { id: 1, label: "摄像头", icon: Camera },
  { id: 2, label: "ROI", icon: Target },
  { id: 3, label: "自检", icon: CheckCircle2 },
  { id: 4, label: "白名单", icon: Shield },
];

export const SetupWindow: React.FC = () => {
  const [step, setStep] = useState(0);
  const [settings, setSettings] = useState<SettingsRecord>(defaultSettings());
  const [processes, setProcesses] = useState<string[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [cameras, setCameras] = useState<string[]>([]);
  const [vision, setVision] = useState<VisionStatus | null>(null);
  const [selfcheckNote, setSelfcheckNote] = useState("等待检测…");
  const [selfcheckOk, setSelfcheckOk] = useState(false);
  const [, setSelfcheckHits] = useState(0);

  const roi = useMemo(() => parseRoi(settings.roi_json), [settings.roi_json]);
  const previewDevice = settings.camera_name || cameras[0] || "0";
  // 步骤 1-3 需要摄像头预览；挂载一次不随 step 卸载
  const showPreview = step >= 1 && step <= 3;

  useEffect(() => {
    invoke<SettingsRecord>("get_settings")
      .then((s) => {
        setSettings({ ...defaultSettings(), ...s, test_mode: s.test_mode ?? false });
        try {
          const list = JSON.parse(s.whitelist_json) as string[];
          setSelected(new Set(list));
        } catch {
          /* ignore */
        }
      })
      .catch(() => {});
    invoke<string[]>("list_running_processes")
      .then(setProcesses)
      .catch(() => setProcesses(["code.exe", "chrome.exe", "msedge.exe"]));
    invoke<string[]>("get_available_cameras")
      .then(setCameras)
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (step !== 3) return;
    let alive = true;
    const id = window.setInterval(() => {
      invoke<VisionStatus>("get_vision_status")
        .then((st) => {
          if (!alive) return;
          setVision(st);
          const d = st.last_detection;
          if (!d) {
            setSelfcheckNote("尚无检测帧 — 确认摄像头与模型");
            return;
          }
          if (d.has_phone && d.phone_brightness >= 40) {
            setSelfcheckHits((n) => {
              const next = n + 1;
              const ok = next >= 3;
              if (ok) setSelfcheckOk(true);
              setSelfcheckNote(
                `检测到亮屏手机 score=${d.phone_score.toFixed(2)} backend=${d.backend} hold=${st.hold_secs}s` +
                  (ok
                    ? " · 自检通过"
                    : ` · 再确认 ${3 - next} 帧`),
              );
              return next;
            });
          } else if (d.has_phone) {
            setSelfcheckNote(
              `检测到手机但偏暗 bright=${d.phone_brightness}（黑屏桌面不算操作）`,
            );
          } else {
            setSelfcheckNote(
              `未检出手机 · backend=${d.backend} · detector=${st.detector}`,
            );
          }
        })
        .catch(() => {});
    }, 500);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [step]);

  const toggle = (name: string) => {
    setSelected((prev) => {
      const n = new Set(prev);
      if (n.has(name)) n.delete(name);
      else n.add(name);
      return n;
    });
  };

  const onRoiChange = (r: RoiRect) => {
    setSettings((s) => ({ ...s, roi_json: JSON.stringify(r) }));
  };

  const finish = async () => {
    const next: SettingsRecord = {
      ...settings,
      setup_completed: true,
      whitelist_json: JSON.stringify([...selected]),
      camera_name: settings.camera_name || cameras[0] || "",
      roi_json:
        settings.roi_json ||
        JSON.stringify({ x: 0.1, y: 0.1, w: 0.8, h: 0.8 }),
    };
    await invoke("save_settings", { settings: next });
    try {
      await invoke("stop_vision_preview");
    } catch {
      /* ignore */
    }
    try {
      await getCurrentWindow().close();
    } catch {
      /* preview */
    }
  };

  return (
    <div
      className="flex h-screen w-screen flex-col text-[var(--df-text)]"
      style={{ background: "var(--df-bg)" }}
    >
      {/* Top bar */}
      <header className="flex items-center justify-between border-b border-white/5 px-6 py-3">
        <div>
          <h1 className="text-base font-bold text-slate-100">DeepFlow 首次配置</h1>
          <p className="text-[11px] text-slate-500">
            步骤 {step + 1}/{STEPS.length} · {STEPS[step].label}
          </p>
        </div>
        {/* Progress chips */}
        <div className="flex items-center gap-1.5">
          {STEPS.map((s) => {
            const Icon = s.icon;
            const active = s.id === step;
            const done = s.id < step;
            return (
              <div
                key={s.id}
                className={`flex items-center gap-1 rounded-full px-2.5 py-1 text-[11px] ${
                  active
                    ? "bg-amber-500/15 text-amber-400 border border-amber-500/30"
                    : done
                      ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20"
                      : "border border-white/5 text-slate-600"
                }`}
              >
                <Icon size={11} />
                <span className="hidden sm:inline">{s.label}</span>
              </div>
            );
          })}
        </div>
      </header>

      {/* Body: left camera (persistent) + right step content */}
      <div className="flex flex-1 min-h-0">
        {/* Left: persistent camera preview for steps 1-3 */}
        {showPreview && (
          <aside className="flex w-[42%] shrink-0 flex-col border-r border-white/5 p-4">
            <p className="mb-2 text-xs uppercase tracking-widest text-slate-500">
              摄像头预览
            </p>
            <CameraPreview
              device={previewDevice}
              autoStart
              enableRoi={step === 2}
              roi={roi}
              onRoiChange={onRoiChange}
              label={
                step === 2
                  ? "拖拽框选 ROI"
                  : step === 3
                    ? "自检预览"
                    : "摄像头预览"
              }
              frameClassName="flex-1 min-h-0"
              className="flex flex-1 flex-col min-h-0"
            />
          </aside>
        )}

        {/* Right: step configuration */}
        <main className="flex flex-1 flex-col min-h-0 overflow-auto df-scroll p-6">
          {step === 0 && (
            <div className="flex flex-1 flex-col justify-center gap-4 max-w-lg">
              <h2 className="text-2xl font-bold text-slate-100">欢迎使用 DeepFlow</h2>
              <p className="text-sm leading-relaxed text-slate-400">
                刷题专注壳：主屏遮罩 + 白名单进程过滤 + 可选视觉干预 + 休息债务机制。
              </p>
              <p className="text-sm leading-relaxed text-slate-400">
                接下来将引导你完成摄像头、ROI 标定、自检与白名单配置。未完成配置将在下次启动时重新进入本向导。
              </p>
              <div className="mt-2 rounded-xl border border-amber-500/20 bg-amber-500/5 px-4 py-3 text-xs text-amber-300/80">
                所有数据仅保存在本地 data/ 目录，不上传云端。
              </div>
            </div>
          )}

          {step === 1 && (
            <div className="space-y-5">
              <h2 className="text-lg font-semibold text-slate-100">选择摄像头</h2>
              <label className="block text-sm text-slate-400">
                摄像头设备
                <select
                  className="df-input mt-1 w-full rounded-xl p-3 text-sm"
                  value={settings.camera_name}
                  onChange={(e) =>
                    setSettings({ ...settings, camera_name: e.target.value })
                  }
                >
                  <option value="">选择…</option>
                  {cameras.map((c) => (
                    <option key={c} value={c}>
                      {c}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex items-center gap-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={settings.vision_enabled}
                  onChange={(e) =>
                    setSettings({ ...settings, vision_enabled: e.target.checked })
                  }
                />
                启用视觉监控（关闭则纯白名单模式）
              </label>
              <label className="flex items-center gap-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={settings.prefer_cpu_inference}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      prefer_cpu_inference: e.target.checked,
                    })
                  }
                />
                优先 CPU 推理
              </label>
            </div>
          )}

          {step === 2 && (
            <div className="space-y-4">
              <h2 className="text-lg font-semibold text-slate-100">框选检测区域</h2>
              <p className="text-sm text-slate-400">
                在左侧预览上拖拽框选 ROI（归一化 0..1）。缩小范围可减少误检并提升推理速度。
              </p>
              <div className="rounded-xl border border-white/5 bg-white/[0.02] px-4 py-3">
                <p className="text-xs text-slate-500">当前 ROI</p>
                <p className="mt-1 font-mono text-sm text-amber-300">
                  {settings.roi_json || "（未设置，将用默认 0.1/0.1/0.8/0.8）"}
                </p>
              </div>
            </div>
          )}

          {step === 3 && (
            <div className="space-y-4">
              <h2 className="text-lg font-semibold text-slate-100">视觉自检</h2>
              <p className="text-sm text-slate-400">
                请拿起亮屏手机对着镜头约 10 秒，观察下方状态。确认检测正常后再继续。
              </p>
              <div className="rounded-xl border border-emerald-700/40 bg-emerald-950/30 p-4 text-sm text-emerald-300">
                {selfcheckNote}
              </div>
              {vision && (
                <p className="font-mono text-xs text-slate-500">
                  detector={vision.detector} running={String(vision.running)} hold=
                  {vision.hold_secs}s
                </p>
              )}
            </div>
          )}

          {step === 4 && (
            <div className="flex min-h-0 flex-1 flex-col gap-4">
              <h2 className="text-lg font-semibold text-slate-100">权限与白名单</h2>
              <label className="text-sm text-slate-400">
                默认专注（分钟）
                <input
                  type="number"
                  min={5}
                  max={180}
                  className="df-input ml-2 w-20 rounded-lg px-2 py-1"
                  value={settings.default_focus_mins}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      default_focus_mins: Number(e.target.value) || 45,
                    })
                  }
                />
              </label>
              <p className="text-sm text-slate-400">
                紧急退出：双击 ESC（固定；设置页仅作记录）
              </p>
              <label className="flex items-center gap-2 text-sm text-amber-400">
                <input
                  type="checkbox"
                  checked={settings.test_mode}
                  onChange={(e) =>
                    setSettings({ ...settings, test_mode: e.target.checked })
                  }
                />
                启用测试模式（干预阈值缩短至 3/6/9 秒，便于验证）
              </label>
              <p className="text-sm font-semibold text-slate-300">
                白名单（当前进程，按进程名）
              </p>
              <div className="min-h-0 flex-1 overflow-auto rounded-xl border border-white/5 bg-white/[0.02] p-2 df-scroll">
                {processes.slice(0, 80).map((p) => (
                  <label
                    key={p}
                    className="flex cursor-pointer items-center gap-2 rounded px-2 py-1 text-sm text-slate-300 hover:bg-white/5"
                  >
                    <input
                      type="checkbox"
                      checked={selected.has(p)}
                      onChange={() => toggle(p)}
                    />
                    {p}
                  </label>
                ))}
              </div>
            </div>
          )}
        </main>
      </div>

      {/* Footer nav */}
      <footer className="flex items-center justify-between border-t border-white/5 px-6 py-3">
        <button
          type="button"
          disabled={step === 0}
          className="df-btn flex items-center gap-1 rounded-xl px-4 py-2 text-sm text-slate-400 hover:bg-white/5 disabled:opacity-30"
          onClick={() => setStep((s) => Math.max(0, s - 1))}
        >
          <ChevronLeft size={16} />
          上一步
        </button>
        {step < STEPS.length - 1 ? (
          <button
            type="button"
            disabled={step === 3 && settings.vision_enabled && !selfcheckOk}
            title={
              step === 3 && settings.vision_enabled && !selfcheckOk
                ? "请先用亮屏手机完成自检（连续检出约 3 帧）"
                : undefined
            }
            className="df-btn flex items-center gap-1 rounded-xl bg-amber-500 px-5 py-2 text-sm font-bold text-slate-950 hover:bg-amber-400 disabled:cursor-not-allowed disabled:opacity-40"
            onClick={() => setStep((s) => s + 1)}
          >
            {step === 3 && settings.vision_enabled && !selfcheckOk
              ? "完成自检后继续"
              : "下一步"}
            <ChevronRight size={16} />
          </button>
        ) : (
          <button
            type="button"
            className="df-btn flex items-center gap-1 rounded-xl bg-emerald-500 px-5 py-2 text-sm font-bold text-slate-950 hover:bg-emerald-400"
            onClick={finish}
          >
            <CheckCircle2 size={16} />
            完成并开始
          </button>
        )}
      </footer>
    </div>
  );
};
