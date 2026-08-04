import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "../hooks/useTauriEvents";
import {
  EVT,
  type SettingsRecord,
  type SystemState,
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
      return "干预 L1";
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

function fmt(secs: number) {
  const m = Math.floor(secs / 60)
    .toString()
    .padStart(2, "0");
  const s = (secs % 60).toString().padStart(2, "0");
  return `${m}:${s}`;
}

export const MainWindow: React.FC = () => {
  const [state, setState] = useState<SystemState | null>(null);
  const [today, setToday] = useState(0);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [reportOpen, setReportOpen] = useState(false);
  const [settings, setSettings] = useState<SettingsRecord | null>(null);
  const [report, setReport] = useState<WeeklyReport | null>(null);
  const [hits, setHits] = useState<WhitelistHit[]>([]);
  const [processes, setProcesses] = useState<string[]>([]);
  const [duration, setDuration] = useState(45);
  const [err, setErr] = useState("");

  const refresh = async () => {
    try {
      setState(await invoke<SystemState>("get_fsm_state"));
      setToday(await invoke<number>("get_today_focus_secs"));
      const s = await invoke<SettingsRecord>("get_settings");
      setSettings(s);
      setDuration(s.default_focus_mins);
    } catch (e) {
      setErr(String(e));
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  useTauriEvent<SystemState>(EVT.fsm, setState, []);
  useTauriEvent<number>(EVT.todayFocus, setToday, []);
  useTauriEvent<WhitelistHit[]>(EVT.whitelist, setHits, []);
  useTauriEvent(EVT.openSettings, () => setSettingsOpen(true), []);
  useTauriEvent(EVT.openReport, () => void openReport(), []);

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
    if (!settings) return;
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
    if (!settings) return;
    const set = new Set(whitelist);
    if (set.has(name)) set.delete(name);
    else set.add(name);
    setSettings({ ...settings, whitelist_json: JSON.stringify([...set]) });
  };

  return (
    <div className="flex h-screen w-screen flex-col bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950 text-white">
      <header className="flex items-center justify-between border-b border-white/5 px-6 py-4">
        <div>
          <h1 className="text-xl font-black tracking-wide">DeepFlow</h1>
          <p className="text-xs text-slate-500">刷题专注 · 本地优先</p>
        </div>
        <button
          type="button"
          onClick={openSettings}
          className="rounded-xl border border-slate-700 px-3 py-1.5 text-sm hover:bg-slate-800"
          title="设置"
        >
          ⚙️ 设置
        </button>
      </header>

      <main className="flex flex-1 flex-col items-center justify-center gap-6 p-8">
        <div className="rounded-3xl border border-white/10 bg-white/5 px-10 py-8 text-center shadow-2xl backdrop-blur">
          <p className="mb-2 text-sm text-slate-400">当前状态</p>
          <p className="text-3xl font-bold">{kindLabel(state)}</p>
          <p className="mt-4 text-slate-400">
            今日专注{" "}
            <span className="font-mono text-amber-300">{fmt(today)}</span>
          </p>
          {settings && settings.pending_debt_secs > 0 && (
            <p className="mt-2 text-sm text-orange-300">
              未还债务 {fmt(settings.pending_debt_secs)}（下次开始将并入）
            </p>
          )}
        </div>

        <div className="flex items-center gap-3">
          <label className="text-sm text-slate-400">
            时长
            <input
              type="number"
              min={5}
              max={180}
              value={duration}
              onChange={(e) => setDuration(Number(e.target.value) || 45)}
              className="ml-2 w-16 rounded-lg border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
            />
            分
          </label>
          <button
            type="button"
            onClick={start}
            disabled={state?.kind === "focus_active"}
            className="rounded-2xl bg-amber-500 px-8 py-3 text-lg font-black text-slate-950 shadow-lg transition hover:bg-amber-400 disabled:opacity-40"
          >
            开始专注
          </button>
          <button
            type="button"
            onClick={() => invoke("stop_session")}
            className="rounded-2xl border border-slate-600 px-4 py-3 text-sm hover:bg-slate-800"
          >
            结束
          </button>
        </div>

        <button
          type="button"
          onClick={openReport}
          className="text-sm text-slate-400 underline-offset-2 hover:text-amber-300 hover:underline"
        >
          查看周报
        </button>

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

      {settingsOpen && settings && (
        <div className="absolute inset-0 z-50 flex justify-end bg-black/50">
          <div className="flex h-full w-full max-w-md flex-col overflow-auto border-l border-slate-800 bg-slate-950 p-6 shadow-2xl">
            <div className="mb-4 flex items-center justify-between">
              <h2 className="text-lg font-bold">设置</h2>
              <button type="button" onClick={() => setSettingsOpen(false)}>
                ✕
              </button>
            </div>

            <label className="mb-3 block text-sm">
              默认专注（分）
              <input
                type="number"
                className="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 py-2"
                value={settings.default_focus_mins}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    default_focus_mins: Number(e.target.value) || 45,
                  })
                }
              />
            </label>

            <label className="mb-3 block text-sm">
              债务下限（秒，默认 180）
              <input
                type="number"
                className="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 py-2"
                value={settings.debt_floor_secs}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    debt_floor_secs: Number(e.target.value) || 180,
                  })
                }
              />
            </label>

            <label className="mb-3 block text-sm">
              紧急快捷键
              <input
                className="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 py-2"
                value={settings.emergency_hotkey}
                onChange={(e) =>
                  setSettings({ ...settings, emergency_hotkey: e.target.value })
                }
              />
            </label>

            <label className="mb-2 flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.debug_mode}
                onChange={(e) =>
                  setSettings({ ...settings, debug_mode: e.target.checked })
                }
              />
              调试模式（详细日志 → data/logs）
            </label>
            <label className="mb-2 flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={settings.vision_enabled}
                onChange={(e) =>
                  setSettings({ ...settings, vision_enabled: e.target.checked })
                }
              />
              视觉监控
            </label>
            <label className="mb-4 flex items-center gap-2 text-sm">
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

            <p className="mb-1 text-sm font-semibold">白名单进程</p>
            <div className="mb-4 max-h-48 overflow-auto rounded-lg border border-slate-800 p-2 text-xs">
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
              className="rounded-xl bg-amber-500 py-2 font-bold text-slate-950"
            >
              保存
            </button>
          </div>
        </div>
      )}

      {reportOpen && report && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 p-6">
          <div className="w-full max-w-lg rounded-3xl border border-slate-700 bg-slate-900 p-6 shadow-2xl">
            <div className="mb-4 flex justify-between">
              <h2 className="text-xl font-bold">本周正向周报</h2>
              <button type="button" onClick={() => setReportOpen(false)}>
                ✕
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
