import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CameraPreview } from "../components/CameraPreview";
import type { SettingsRecord } from "../types/tauri-ipc";

const defaultSettings = (): SettingsRecord => ({
  setup_completed: false,
  default_focus_mins: 45,
  debt_floor_secs: 180,
  emergency_hotkey: "double_esc",
  debug_mode: false,
  vision_enabled: true,
  prefer_cpu_inference: false,
  camera_name: "",
  roi_json: "",
  whitelist_json: "[]",
  pending_debt_secs: 0,
});

export const SetupWindow: React.FC = () => {
  const [step, setStep] = useState(0);
  const [settings, setSettings] = useState<SettingsRecord>(defaultSettings());
  const [processes, setProcesses] = useState<string[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [cameras, setCameras] = useState<string[]>([]);

  useEffect(() => {
    invoke<SettingsRecord>("get_settings")
      .then((s) => {
        setSettings(s);
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

  const toggle = (name: string) => {
    setSelected((prev) => {
      const n = new Set(prev);
      if (n.has(name)) n.delete(name);
      else n.add(name);
      return n;
    });
  };

  const finish = async () => {
    const next: SettingsRecord = {
      ...settings,
      setup_completed: true,
      whitelist_json: JSON.stringify([...selected]),
      camera_name: settings.camera_name || cameras[0] || "",
    };
    await invoke("save_settings", { settings: next });
    try {
      await getCurrentWindow().close();
    } catch {
      /* preview */
    }
  };

  const steps = ["欢迎", "摄像头", "ROI", "自检", "权限与白名单"];

  return (
    <div className="flex h-screen w-screen flex-col bg-slate-950 p-8 text-white">
      <h1 className="mb-2 text-2xl font-black">DeepFlow 首次配置</h1>
      <p className="mb-6 text-sm text-slate-400">
        步骤 {step + 1}/5 · {steps[step]}
      </p>

      {step === 0 && (
        <div className="space-y-3 text-slate-300">
          <p>刷题专注壳：主屏遮罩 + 白名单 +（可选）视觉干预 + 休息债务。</p>
          <p>未完成配置将在下次启动时重新进入本向导。</p>
        </div>
      )}

      {step === 1 && (
        <div className="space-y-4">
          <CameraPreview />
          <label className="block text-sm text-slate-400">摄像头</label>
          <select
            className="w-full rounded-xl border border-slate-700 bg-slate-900 p-3"
            value={settings.camera_name}
            onChange={(e) => setSettings({ ...settings, camera_name: e.target.value })}
          >
            <option value="">选择…</option>
            {cameras.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={settings.vision_enabled}
              onChange={(e) =>
                setSettings({ ...settings, vision_enabled: e.target.checked })
              }
            />
            启用视觉监控（P1；关闭则纯白名单模式）
          </label>
        </div>
      )}

      {step === 2 && (
        <div className="space-y-3">
          <CameraPreview label="在预览上拖 ROI（P1 实现交互）" />
          <p className="text-sm text-slate-400">P0 可跳过；P1 将强制可框选并写入 roi_json。</p>
        </div>
      )}

      {step === 3 && (
        <div className="space-y-3">
          <p>请拿起亮屏手机对着镜头约 10 秒（P1 接入检测）。</p>
          <div className="rounded-xl border border-emerald-700/50 bg-emerald-950/40 p-4 text-emerald-300">
            P0：可标记为弱校准并继续
          </div>
        </div>
      )}

      {step === 4 && (
        <div className="flex min-h-0 flex-1 flex-col gap-3">
          <label className="text-sm text-slate-400">
            默认专注（分钟）
            <input
              type="number"
              min={5}
              max={180}
              className="ml-2 w-20 rounded border border-slate-700 bg-slate-900 px-2 py-1"
              value={settings.default_focus_mins}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  default_focus_mins: Number(e.target.value) || 45,
                })
              }
            />
          </label>
          <p className="text-sm text-slate-400">紧急退出：双击 ESC（可在设置中改）</p>
          <p className="text-sm font-semibold">白名单（当前进程，按进程名）</p>
          <div className="min-h-0 flex-1 overflow-auto rounded-xl border border-slate-800 bg-slate-900/50 p-2">
            {processes.slice(0, 80).map((p) => (
              <label key={p} className="flex cursor-pointer items-center gap-2 px-2 py-1 text-sm hover:bg-slate-800">
                <input type="checkbox" checked={selected.has(p)} onChange={() => toggle(p)} />
                {p}
              </label>
            ))}
          </div>
        </div>
      )}

      <div className="mt-6 flex justify-between">
        <button
          type="button"
          disabled={step === 0}
          className="rounded-xl px-4 py-2 text-slate-400 hover:bg-slate-900 disabled:opacity-30"
          onClick={() => setStep((s) => Math.max(0, s - 1))}
        >
          上一步
        </button>
        {step < 4 ? (
          <button
            type="button"
            className="rounded-xl bg-amber-500 px-5 py-2 font-bold text-slate-950"
            onClick={() => setStep((s) => s + 1)}
          >
            下一步
          </button>
        ) : (
          <button
            type="button"
            className="rounded-xl bg-emerald-500 px-5 py-2 font-bold text-slate-950"
            onClick={finish}
          >
            完成并开始
          </button>
        )}
      </div>
    </div>
  );
};
