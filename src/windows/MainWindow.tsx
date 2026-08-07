import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Settings as SettingsIcon,
  BarChart3,
  Play,
  Square,
  X,
  Camera,
  Cpu,
  Eye,
  EyeOff,
  RefreshCw,
  FlaskConical,
  Keyboard,
} from "lucide-react";
import { CameraPreview } from "../components/CameraPreview";
import { useTauriEvent } from "../hooks/useTauriEvents";
import { playSound } from "../lib/sounds";
import {
  EVT,
  type SettingsRecord,
  type SystemState,
  type VisionStatus,
  type WeeklyReport,
  type WhitelistHit,
} from "../types/tauri-ipc";

function kindLabel(s: SystemState | null): string {
  if (!s) return "…";
  switch (s.kind) {
    case "idle":
      return "空闲";
    case "focus_active":
      return `专注中 ${fmt(s.remaining_secs)}`;
    case "temporary_pause":
      return `休息 ${fmt(s.elapsed_secs)}`;
    case "intervention_level1":
      return `干预 L1 · 观察 ${s.observe_remaining_secs ?? 0}s`;
    case "intervention_level2":
      return "干预 L2";
    case "intervention_level3":
      return "干预 L3";
    case "await_session_end_choice":
      return "本轮结束·待选择";
    default:
      return "未知";
  }
}

function kindColor(s: SystemState | null): string {
  if (!s) return "text-slate-400";
  switch (s.kind) {
    case "focus_active":
      return "text-amber-300";
    case "temporary_pause":
      return "text-sky-300";
    case "intervention_level1":
      return "text-amber-400";
    case "intervention_level2":
      return "text-orange-400";
    case "intervention_level3":
      return "text-red-400";
    default:
      return "text-slate-400";
  }
}

function fmt(secs: number) {
  const m = Math.floor(secs / 60).toString().padStart(2, "0");
  const s = (secs % 60).toString().padStart(2, "0");
  return `${m}:${s}`;
}

const DEFAULT_SETTINGS: SettingsRecord = {
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
};

export const MainWindow: React.FC = () => {
  const [state, setState] = useState<SystemState | null>(null);
  const [today, setToday] = useState(0);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [reportOpen, setReportOpen] = useState(false);
  const [settings, setSettings] = useState<SettingsRecord>(DEFAULT_SETTINGS);
  const [report, setReport] = useState<WeeklyReport | null>(null);
  const [hits, setHits] = useState<WhitelistHit[]>([]);
  const [processes, setProcesses] = useState<string[]>([]);
  const [duration, setDuration] = useState(45);
  const [err, setErr] = useState("");
  const [vision, setVision] = useState<VisionStatus | null>(null);

  const refresh = async () => {
    try {
      setState(await invoke<SystemState>("get_fsm_state"));
      setToday(await invoke<number>("get_today_focus_secs"));
      const s = await invoke<SettingsRecord>("get_settings");
      setSettings(s);
      setDuration(s.default_focus_mins);
      setVision(await invoke<VisionStatus>("get_vision_status"));
    } catch (e) {
      setErr(String(e));
    }
  };

  useEffect(() => {
    refresh();
    const id = window.setInterval(() => {
      invoke<VisionStatus>("get_vision_status").then(setVision).catch(() => {});
    }, 1500);
    return () => window.clearInterval(id);
  }, []);

  useTauriEvent<SystemState>(EVT.fsm, setState, []);
  useTauriEvent<number>(EVT.todayFocus, setToday, []);
  useTauriEvent<WhitelistHit[]>(EVT.whitelist, setHits, []);
  useTauriEvent(EVT.openSettings, () => setSettingsOpen(true), []);
  useTauriEvent(EVT.openReport, () => void openReport(), []);
  useTauriEvent<string>(EVT.playSound, (kind) => playSound(kind), []);

  const openReport = async () => {
    try {
      setReport(await invoke<WeeklyReport>("get_weekly_report"));
      setReportOpen(true);
    } catch (e) {
      setErr(String(e));
    }
  };

  const openSettings = async () => {
    const s = await invoke<SettingsRecord>("get_settings");
    setSettings(s);
    const procs = await invoke<string[]>("list_running_processes").catch(() => []);
    setProcesses(procs);
    setSettingsOpen(true);
  };

  const start = async () => {
    setErr("");
    try {
      await invoke("start_focus_session", { durationMins: duration });
    } catch (e) {
      setErr(String(e));
    }
  };

  const saveSettings = async () => {
    await invoke("save_settings", { settings });
    setSettingsOpen(false);
    await refresh();
  };

  const whitelist: string[] = (() => {
    try {
      return settings ? (JSON.parse(settings.whitelist_json) as string[]) : [];
    } catch {
      return [];
    }
  })();

  const toggleWl = (name: string) => {
    const set = new Set(whitelist);
    if (set.has(name)) set.delete(name);
    else set.add(name);
    setSettings({ ...settings, whitelist_json: JSON.stringify([...set]) });
  };

  const isActive = state?.kind === "focus_active";

  return (
    <div
      className="flex h-screen w-screen flex-col text-[var(--df-text)]"
      style={{ background: "var(--df-bg)" }}
    >
      {/* Header */}
      <header className="flex items-center justify-between border-b border-white/5 px-6 py-3">
        <div className="flex items-center gap-3">
          <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-amber-500/15 text-amber-400">
            <FlaskConical size={18} />
          </div>
          <div>
            <h1 className="text-base font-bold tracking-wide text-slate-100">
              DeepFlow
            </h1>
            <p className="text-[11px] text-slate-500">刷题专注 · 本地优先</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={openReport}
            className="df-btn flex items-center gap-1.5 rounded-lg border border-white/10 px-3 py-1.5 text-xs text-slate-300 hover:bg-white/5"
            title="周报"
          >
            <BarChart3 size={14} />
            周报
          </button>
          <button
            type="button"
            onClick={openSettings}
            className="df-btn flex items-center gap-1.5 rounded-lg border border-white/10 px-3 py-1.5 text-xs text-slate-300 hover:bg-white/5"
            title="设置"
          >
            <SettingsIcon size={14} />
            设置
          </button>
        </div>
      </header>

      {/* Body: center timer + right vision sidebar */}
      <div className="flex flex-1 min-h-0">
        {/* Center: timer & controls */}
        <main className="flex flex-1 flex-col items-center justify-center gap-8 p-8">
          <div className="df-panel rounded-2xl px-12 py-10 text-center shadow-xl">
            <p className="mb-2 text-xs uppercase tracking-widest text-slate-500">
              当前状态
            </p>
            <p className={`text-4xl font-bold ${kindColor(state)}`}>
              {kindLabel(state)}
            </p>
            <p className="mt-6 text-sm text-slate-400">
              今日专注{" "}
              <span className="font-mono text-lg text-amber-300">{fmt(today)}</span>
            </p>
            {settings.pending_debt_secs > 0 && (
              <p className="mt-1 text-xs text-orange-400">
                未还债务 {fmt(settings.pending_debt_secs)}（下次开始将并入）
              </p>
            )}
          </div>

          <div className="flex items-center gap-3">
            <label className="flex items-center gap-2 text-sm text-slate-400">
              时长
              <input
                type="number"
                min={5}
                max={180}
                value={duration}
                onChange={(e) => setDuration(Number(e.target.value) || 45)}
                className="df-input ml-1 w-16 rounded-lg px-2 py-1 font-mono text-sm"
              />
              分
            </label>
            <button
              type="button"
              onClick={start}
              disabled={isActive}
              className="df-btn flex items-center gap-2 rounded-xl bg-amber-500 px-7 py-3 text-base font-bold text-slate-950 shadow-lg hover:bg-amber-400 disabled:opacity-40"
            >
              <Play size={18} />
              开始专注
            </button>
            <button
              type="button"
              onClick={() => invoke("stop_session")}
              className="df-btn flex items-center gap-2 rounded-xl border border-slate-600 px-4 py-3 text-sm text-slate-300 hover:bg-white/5"
            >
              <Square size={14} />
              结束
            </button>
          </div>

          {settings.test_mode && (
            <div className="flex w-full max-w-xl flex-col gap-2">
              <div className="df-chip flex items-center gap-1.5 self-start rounded-full px-3 py-1 text-xs text-amber-400">
                <FlaskConical size={12} />
                测试模式 · L1=3s L2=6s L3=9s · 输入「测试」可退出
              </div>
              <div className="flex flex-wrap items-center justify-center gap-2">
                {([1, 2, 3] as const).map((lv) => (
                  <button
                    key={lv}
                    type="button"
                    className="df-btn rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-1.5 text-xs font-semibold text-amber-300 hover:bg-amber-500/20"
                    onClick={async () => {
                      setErr("");
                      try {
                        await invoke("test_inject_level", { level: lv });
                      } catch (e) {
                        setErr(String(e));
                      }
                    }}
                  >
                    注入 L{lv}
                  </button>
                ))}
                <button
                  type="button"
                  className="df-btn rounded-lg border border-red-500/40 bg-red-500/10 px-3 py-1.5 text-xs font-semibold text-red-200 hover:bg-red-500/20"
                  onClick={async () => {
                    setErr("");
                    try {
                      await invoke("force_exit_session");
                    } catch {
                      try {
                        await invoke("test_exit_session");
                      } catch (e) {
                        setErr(String(e));
                      }
                    }
                  }}
                >
                  强制退出
                </button>
              </div>
            </div>
          )}

          {hits.length > 0 && (
            <div className="max-w-lg rounded-xl border border-orange-700/50 bg-orange-950/40 p-3 text-sm text-orange-100">
              <p className="font-semibold">白名单外进程（请在 15s 内关闭）</p>
              <ul className="mt-1 list-inside list-disc">
                {hits.slice(0, 5).map((h) => (
                  <li key={`${h.process_name}-${h.pid}`}>
                    {h.process_name} ({h.pid})
                  </li>
                ))}
              </ul>
            </div>
          )}

          {err && <p className="text-sm text-red-400">{err}</p>}
        </main>

        {/* Right: vision status sidebar */}
        <aside className="hidden w-80 shrink-0 border-l border-white/5 p-4 lg:flex flex-col gap-3 min-h-0">
          <p className="text-xs uppercase tracking-widest text-slate-500">视觉</p>
          {(settings.test_mode || (vision?.running ?? false)) && (
            <CameraPreview
              device={settings.camera_name || undefined}
              autoStart={settings.test_mode && !(vision?.running ?? false)}
              compact
              frameClassName="h-40"
              pollMs={400}
              label="检测预览"
            />
          )}
          {vision ? (
            <div className="space-y-2 min-h-0 overflow-auto df-scroll">
              <div className="flex items-center gap-2 text-sm">
                {vision.enabled ? (
                  <Eye size={14} className="text-emerald-400" />
                ) : (
                  <EyeOff size={14} className="text-slate-500" />
                )}
                <span
                  className={
                    vision.enabled
                      ? vision.running
                        ? "text-emerald-400"
                        : "text-slate-400"
                      : "text-slate-500"
                  }
                >
                  {vision.enabled
                    ? vision.running
                      ? "运行中"
                      : "待命"
                    : "已关闭"}
                </span>
              </div>
              <div className="flex items-center gap-2 text-xs text-slate-500">
                <Camera size={12} />
                <span className="truncate">{vision.camera_name || "未选"}</span>
              </div>
              <div className="flex items-center gap-2 text-xs text-slate-500">
                <Cpu size={12} />
                <span className="truncate">{vision.detector}</span>
              </div>
              <div className="rounded-lg border border-white/5 bg-white/[0.02] px-3 py-2">
                <p className="text-xs text-slate-500">持握时长</p>
                <p className="font-mono text-lg text-amber-300">
                  {vision.hold_secs}s
                </p>
              </div>
              {vision.last_detection && (
                <div className="rounded-lg border border-white/5 bg-white/[0.02] px-3 py-2 text-xs">
                  <p className="text-slate-500">最近检测</p>
                  <p className="mt-1 font-mono text-slate-300">
                    phone={vision.last_detection.has_phone ? "Y" : "n"}{" "}
                    score={vision.last_detection.phone_score.toFixed(2)}
                  </p>
                  <p className="font-mono text-slate-400">
                    bright={vision.last_detection.phone_brightness} backend=
                    {vision.last_detection.backend}
                  </p>
                </div>
              )}
            </div>
          ) : (
            <p className="text-xs text-slate-600">加载中…</p>
          )}
        </aside>
      </div>

      {/* Settings drawer */}
      {settingsOpen && (
        <div className="absolute inset-0 z-50 flex justify-end bg-black/50">
          <div className="df-panel flex h-full w-full max-w-md flex-col overflow-auto df-scroll p-6 shadow-2xl">
            <div className="mb-6 flex items-center justify-between">
              <h2 className="text-base font-bold text-slate-100">设置</h2>
              <button
                type="button"
                onClick={() => setSettingsOpen(false)}
                className="rounded-lg p-1 text-slate-400 hover:bg-white/5"
              >
                <X size={18} />
              </button>
            </div>

            <label className="mb-4 block text-sm text-slate-300">
              默认专注（分）
              <input
                type="number"
                className="df-input mt-1 w-full rounded-lg px-3 py-2"
                value={settings.default_focus_mins}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    default_focus_mins: Number(e.target.value) || 45,
                  })
                }
              />
            </label>

            <label className="mb-4 block text-sm text-slate-300">
              债务下限（秒，默认 180）
              <input
                type="number"
                className="df-input mt-1 w-full rounded-lg px-3 py-2"
                value={settings.debt_floor_secs}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    debt_floor_secs: Number(e.target.value) || 180,
                  })
                }
              />
            </label>

                        <label className="mb-4 block text-sm text-slate-300">
              <span className="flex items-center gap-1.5">
                <Keyboard size={13} className="text-slate-500" />
                紧急快捷键
              </span>
              <select
                className="df-input mt-1 w-full rounded-lg px-3 py-2 font-mono text-xs"
                value={settings.emergency_hotkey || "double_esc"}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    emergency_hotkey: e.target.value,
                  })
                }
              >
                <option value="double_esc">双击 ESC（400ms）</option>
                <option value="f9">F9</option>
                <option value="ctrl_shift_e">Ctrl+Shift+E</option>
                <option value="ctrl_alt_q">Ctrl+Alt+Q</option>
              </select>
              <span className="mt-1 block text-[11px] text-slate-500">
                保存后立即生效，无需重启。用于紧急退出会话/遮罩。
              </span>
            </label>

            <div className="mb-4 space-y-2">
              <label className="flex items-center gap-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={settings.debug_mode}
                  onChange={(e) =>
                    setSettings({ ...settings, debug_mode: e.target.checked })
                  }
                />
                调试模式（详细日志 → data/logs）
              </label>
              <label className="flex items-center gap-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={settings.vision_enabled}
                  onChange={(e) =>
                    setSettings({ ...settings, vision_enabled: e.target.checked })
                  }
                />
                视觉监控
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
                优先 CPU 推理（DirectML 失败时）
              </label>
              <label className="flex items-center gap-2 text-sm text-amber-400">
                <input
                  type="checkbox"
                  checked={settings.test_mode}
                  onChange={(e) =>
                    setSettings({ ...settings, test_mode: e.target.checked })
                  }
                />
                测试模式（L1=3s L2=6s L3=9s，放回 1s 恢复）
              </label>
            </div>

            <label className="mb-4 block text-sm text-slate-300">
              摄像头设备
              <input
                className="df-input mt-1 w-full rounded-lg px-3 py-2 font-mono text-xs"
                value={settings.camera_name}
                placeholder="0 或 0|Integrated Camera"
                onChange={(e) =>
                  setSettings({ ...settings, camera_name: e.target.value })
                }
              />
            </label>

            <button
              type="button"
              className="df-btn mb-4 flex items-center gap-1.5 rounded-lg border border-slate-600 px-3 py-1.5 text-sm text-slate-300 hover:bg-white/5"
              onClick={async () => {
                try {
                  await invoke("restart_vision");
                  setVision(await invoke<VisionStatus>("get_vision_status"));
                } catch (e) {
                  setErr(String(e));
                }
              }}
            >
              <RefreshCw size={14} />
              重启视觉管线
            </button>

            <p className="mb-1 text-sm font-semibold text-slate-300">
              白名单进程
            </p>
            <div className="mb-4 max-h-48 overflow-auto rounded-lg border border-white/5 p-2 text-xs text-slate-400 df-scroll">
              {(processes.length ? processes : whitelist).slice(0, 60).map((p) => (
                <label key={p} className="flex gap-2 py-0.5">
                  <input
                    type="checkbox"
                    checked={whitelist.includes(p)}
                    onChange={() => toggleWl(p)}
                  />
                  {p}
                </label>
              ))}
            </div>

            <p className="mb-4 text-xs text-slate-500">
              数据目录：D:\3_Code_Projects\DeepFlow\data
            </p>

            <button
              type="button"
              onClick={saveSettings}
              className="df-btn rounded-xl bg-amber-500 py-2.5 font-bold text-slate-950 hover:bg-amber-400"
            >
              保存
            </button>
          </div>
        </div>
      )}

      {/* Report modal */}
      {reportOpen && report && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 p-6">
          <div className="df-panel w-full max-w-lg rounded-2xl p-6 shadow-2xl">
            <div className="mb-4 flex items-center justify-between">
              <h2 className="text-lg font-bold text-slate-100">本周正向周报</h2>
              <button
                type="button"
                onClick={() => setReportOpen(false)}
                className="rounded-lg p-1 text-slate-400 hover:bg-white/5"
              >
                <X size={18} />
              </button>
            </div>
            <ul className="space-y-2 text-sm text-slate-300">
              <li>总专注：{report.total_focus_minutes} 分钟</li>
              <li>成功拉回：{report.successful_pullbacks_count} 次</li>
              <li>合规休息：{report.total_borrowed_rest_minutes} 分钟</li>
              <li>平均专注：{report.avg_focus_minutes} 分钟/会话</li>
              <li>中断相关：{report.interrupted_count}</li>
              <li>
                较上周：{report.vs_last_week_focus_delta_minutes >= 0 ? "+" : ""}
                {report.vs_last_week_focus_delta_minutes} 分钟
              </li>
              <li>黄金时段：{report.golden_focus_hour_range}</li>
            </ul>
            <p className="mt-4 text-xs text-slate-500">分享图 PNG 将在 P2 生成</p>
          </div>
        </div>
      )}
    </div>
  );
};
