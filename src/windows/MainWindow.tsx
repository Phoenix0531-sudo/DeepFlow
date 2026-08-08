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
  Download,
  FolderOpen,
  History,
  Database,
} from "lucide-react";
import { CameraPreview } from "../components/CameraPreview";
import { useTauriEvent } from "../hooks/useTauriEvents";
import { useToast, ToastContainer } from "../hooks/useToast";
import { playSound } from "../lib/sounds";
import {
  EVT,
  type SettingsRecord,
  type SystemState,
  type VisionStatus,
  type WeeklyReport,
  type WhitelistHit,
  type ModelEntry,
  type L3ReasonEntry,
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
  auto_open_exports: true,
  whitelist_action: "report",
  sound_muted: false,
  auto_start: false,
  notifications_enabled: true,
};

export const MainWindow: React.FC = () => {
  const { toasts, showError, showSuccess, remove, push } = useToast();
  const [state, setState] = useState<SystemState | null>(null);
  const [today, setToday] = useState(0);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [reportOpen, setReportOpen] = useState(false);
  const [settings, setSettings] = useState<SettingsRecord>(DEFAULT_SETTINGS);
  const [report, setReport] = useState<WeeklyReport | null>(null);
  const [reportPngPath, setReportPngPath] = useState<string>("");
  const [exporting, setExporting] = useState(false);
  const [dataDir, setDataDir] = useState<string>("");
  const [pathMode, setPathMode] = useState<string>("");
  const [hits, setHits] = useState<WhitelistHit[]>([]);
  const [processes, setProcesses] = useState<string[]>([]);
  // #39：白名单进程搜索
  const [processSearch, setProcessSearch] = useState("");
  // #24：摄像头列表（与 SetupWindow 保持一致）
  const [cameras, setCameras] = useState<string[]>([]);
  const [duration, setDuration] = useState(45);
  const [vision, setVision] = useState<VisionStatus | null>(null);
  // #14：模型自管理 UI 状态
  const [models, setModels] = useState<ModelEntry[]>([]);
  const [reseeding, setReseeding] = useState(false);
  // #15：周报历史周选择（0=本周）
  const [reportWeek, setReportWeek] = useState(0);
  // #16：L3 原因记录查看
  const [l3Reasons, setL3Reasons] = useState<L3ReasonEntry[]>([]);
  const [l3Loading, setL3Loading] = useState(false);

  const refresh = async () => {
    try {
      setState(await invoke<SystemState>("get_fsm_state"));
      setToday(await invoke<number>("get_today_focus_secs"));
      const s = await invoke<SettingsRecord>("get_settings");
      setSettings(s);
      setDuration(s.default_focus_mins);
      setVision(await invoke<VisionStatus>("get_vision_status"));
    } catch (e) {
      showError(e);
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
  useTauriEvent<string>(EVT.playSound, (kind) => playSound(kind, settings.sound_muted), [settings.sound_muted]);

  const openReport = async (week: number = 0) => {
    try {
      setReportWeek(week);
      setReport(
        await invoke<WeeklyReport>("get_weekly_report_at", { weeksAgo: week }),
      );
      setReportPngPath("");
      setReportOpen(true);
    } catch (e) {
      showError(e);
    }
  };

  const exportPng = async () => {
    if (exporting) return;
    setExporting(true);
    try {
      const path = await invoke<string>("export_weekly_report_png");
      setReportPngPath(path);
      showSuccess("周报 PNG 已导出");
      // #11：若设置开启，导出后自动打开所在目录并选中文件
      if (settings.auto_open_exports) {
        try {
          const info = await invoke<{ exports_dir: string; data_dir: string; mode: string }>("get_path_info");
          setDataDir(info.data_dir);
          setPathMode(info.mode);
          await invoke<void>("reveal_path", { path: path || info.exports_dir });
        } catch (e) {
          showError(e);
        }
      }
    } catch (e) {
      showError(`导出失败：${String(e)}`);
    } finally {
      setExporting(false);
    }
  };

  const openExportsDir = async () => {
    try {
      const info = await invoke<{
        exports_dir: string;
        mode: string;
        data_dir: string;
      }>("get_path_info");
      setDataDir(info.data_dir);
      setPathMode(info.mode);
      // reveal_path 会打开资源管理器并选中对应文件/目录
      await invoke<void>("reveal_path", { path: reportPngPath || info.exports_dir });
    } catch (e) {
      showError(`打开目录失败：${String(e)}`);
    }
  };

  const openSettings = async () => {
    const s = await invoke<SettingsRecord>("get_settings");
    setSettings(s);
    const procs = await invoke<string[]>("list_running_processes").catch(() => []);
    setProcesses(procs);
    // #14：同步加载模型清单
    try {
      setModels(await invoke<ModelEntry[]>("list_models"));
    } catch (e) {
      showError(e);
    }
    try {
      const info = await invoke<{ data_dir: string; mode: string }>("get_path_info");
      setDataDir(info.data_dir);
      setPathMode(info.mode);
    } catch {
      /* 忽略，下方默认展示空 */
    }
    // #24：加载摄像头列表
    try {
      setCameras(await invoke<string[]>("get_available_cameras"));
    } catch {
      /* 忽略，下拉为空时回退到文本输入 */
    }
    setSettingsOpen(true);
  };

  const start = async () => {
    try {
      await invoke("start_focus_session", { durationMins: duration });
    } catch (e) {
      showError(e);
    }
  };

  const saveSettings = async () => {
    try {
      await invoke("save_settings", { settings });
      // #23：同步登录自启到系统
      try {
        await invoke("set_autostart_enabled", { enabled: settings.auto_start });
      } catch (e) {
        showError(`自启设置失败: ${e}`);
      }
      // #29：首次开启通知时申请权限
      if (settings.notifications_enabled) {
        try {
          await invoke("request_notification_permission");
        } catch {
          /* 权限申请失败不阻断保存 */
        }
      }
      setSettingsOpen(false);
      showSuccess("设置已保存");
      await refresh();
    } catch (e) {
      showError(e);
    }
  };

  // #33：检查更新
  const checkUpdates = async () => {
    try {
      push("正在检查更新…", "info");
      const r = await invoke<{ available: boolean; version?: string; body?: string }>(
        "check_for_updates",
      );
      if (r.available) {
        const ok = window.confirm(
          `发现新版本 ${r.version ?? ""}\n\n${r.body ?? ""}\n\n是否立即下载并安装？`,
        );
        if (ok) {
          push("正在下载更新…", "info");
          await invoke("download_and_install_update");
        }
      } else {
        showSuccess("已是最新版本（或未配置更新源）");
      }
    } catch (e) {
      showError(e);
    }
  };

  // #29：发送测试通知
  const testNotification = async () => {
    try {
      await invoke("request_notification_permission");
      await invoke("send_notification", {
        title: "DeepFlow",
        body: "系统通知已就绪",
      });
      showSuccess("已发送测试通知");
    } catch (e) {
      showError(e);
    }
  };

  // #14：重新触发种子模型复制
  const reseedModels = async () => {
    if (reseeding) return;
    setReseeding(true);
    try {
      const n = await invoke<number>("reseed_models");
      setModels(await invoke<ModelEntry[]>("list_models"));
      push(n > 0 ? `已复制 ${n} 个种子模型` : "无可复制的种子模型（可能已存在）", n > 0 ? "success" : "info");
    } catch (e) {
      showError(e);
    } finally {
      setReseeding(false);
    }
  };

  // #34 B2：备份当前设置
  const backupSettings = async () => {
    try {
      const path = await invoke<string>("backup_settings");
      showSuccess(`设置已备份：${path}`);
      try { await invoke("reveal_path", { path }); } catch { /* ignore */ }
    } catch (e) {
      showError(e);
    }
  };

  // #34 B2：从备份 JSON 恢复设置
  const restoreSettings = async () => {
    const path = window.prompt("请输入备份 JSON 文件完整路径：");
    if (!path || !path.trim()) return;
    try {
      const s = await invoke<SettingsRecord>("restore_settings", { path: path.trim() });
      setSettings(s);
      showSuccess("设置已恢复");
      await refresh();
    } catch (e) {
      showError(e);
    }
  };

  // #28 B1：导出全部数据 JSON
  const exportAllData = async () => {
    try {
      const path = await invoke<string>("export_all_data");
      showSuccess(`已导出：${path}`);
      try {
        await invoke("reveal_path", { path });
      } catch {
        /* 打开目录失败不阻断 */
      }
    } catch (e) {
      showError(e);
    }
  };

  // #28 B1：清空历史（可选重置设置）
  const clearAllData = async (resetSettings: boolean) => {
    const msg = resetSettings
      ? "将清空全部日志/专注记录，并重置设置为默认。此操作不可撤销，确认？"
      : "将清空全部日志与每日专注累计，保留当前设置。此操作不可撤销，确认？";
    if (!window.confirm(msg)) return;
    try {
      await invoke("clear_all_data", { clearSettings: resetSettings });
      showSuccess(resetSettings ? "数据已清空并重置设置" : "历史数据已清空");
      await refresh();
      if (resetSettings) {
        setSettingsOpen(false);
      }
    } catch (e) {
      showError(e);
    }
  };

  // #16：加载最近 L3 原因记录
  const loadL3Reasons = async () => {
    if (l3Loading) return;
    setL3Loading(true);
    try {
      setL3Reasons(await invoke<L3ReasonEntry[]>("get_l3_reasons", { limit: 20 }));
    } catch (e) {
      showError(e);
    } finally {
      setL3Loading(false);
    }
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
            onClick={() => void openReport()}
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
              <p className="mt-1 flex items-center gap-2 text-xs text-orange-400">
                未还债务 {fmt(settings.pending_debt_secs)}（下次开始将并入）
                <button
                  type="button"
                  className="df-btn rounded border border-orange-500/40 bg-orange-500/10 px-2 py-0.5 text-[11px] font-semibold text-orange-300 hover:bg-orange-500/20"
                  onClick={async () => {
                    try {
                      await invoke("save_settings", {
                        settings: { ...settings, pending_debt_secs: 0 },
                      });
                      setSettings({ ...settings, pending_debt_secs: 0 });
                      showSuccess("债务已结清");
                    } catch (e) {
                      showError(e);
                    }
                  }}
                >
                  立即结清
                </button>
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
                onChange={(e) =>
                  setDuration(
                    Math.min(180, Math.max(5, Number(e.target.value) || 45)),
                  )
                }
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
                      try {
                        await invoke("test_inject_level", { level: lv });
                      } catch (e) {
                        showError(e);
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
                    try {
                      await invoke("force_exit_session");
                    } catch {
                      try {
                        await invoke("test_exit_session");
                      } catch (e) {
                        showError(e);
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
              <p className="font-semibold">
                {settings.whitelist_action === "minimize"
                  ? "白名单外进程（已自动最小化）"
                  : settings.whitelist_action === "close_report"
                    ? "白名单外进程（已请求关闭窗口）"
                    : "白名单外进程（请在 15s 内关闭）"}
              </p>
              <ul className="mt-1 list-inside list-disc">
                {hits.slice(0, 5).map((h) => (
                  <li key={`${h.process_name}-${h.pid}`}>
                    {h.process_name} ({h.pid})
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* #36：手动停止 / 到点后的三选一（主窗兜底；遮罩也会展示） */}
          {state?.kind === "await_session_end_choice" && (
            <div className="w-full max-w-md rounded-2xl border border-amber-500/40 bg-slate-900/95 p-6 shadow-2xl">
              <h2 className="mb-1 text-xl font-bold text-white">本轮专注结束</h2>
              <p className="mb-4 text-sm text-slate-400">接下来？</p>
              <div className="flex flex-col gap-2">
                {(
                  [
                    ["continue", "继续下一轮"],
                    ["rest", "先休息"],
                    ["end", "结束"],
                  ] as const
                ).map(([c, label]) => (
                  <button
                    key={c}
                    type="button"
                    className="df-btn rounded-xl bg-amber-500 py-2.5 font-bold text-slate-950 hover:bg-amber-400"
                    onClick={async () => {
                      try {
                        await invoke("choose_session_end", { choice: c });
                      } catch (e) {
                        showError(e);
                      }
                    }}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>
          )}
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
              <label className="flex items-center gap-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={settings.auto_open_exports}
                  onChange={(e) =>
                    setSettings({ ...settings, auto_open_exports: e.target.checked })
                  }
                />
                导出周报后自动打开所在目录
              </label>
            </div>

            <label className="mb-4 block text-sm text-slate-300">
              摄像头设备
              {cameras.length > 0 ? (
                <select
                  className="df-input mt-1 w-full rounded-lg px-3 py-2 text-sm"
                  value={settings.camera_name}
                  onChange={(e) =>
                    setSettings({ ...settings, camera_name: e.target.value })
                  }
                >
                  <option value="">未选择</option>
                  {cameras.map((c) => (
                    <option key={c} value={c}>
                      {c}
                    </option>
                  ))}
                </select>
              ) : (
                <input
                  className="df-input mt-1 w-full rounded-lg px-3 py-2 font-mono text-xs"
                  value={settings.camera_name}
                  placeholder="0 或 0|Integrated Camera"
                  onChange={(e) =>
                    setSettings({ ...settings, camera_name: e.target.value })
                  }
                />
              )}
            </label>

            <button
              type="button"
              className="df-btn mb-4 flex items-center gap-1.5 rounded-lg border border-slate-600 px-3 py-1.5 text-sm text-slate-300 hover:bg-white/5"
              onClick={async () => {
                try {
                  showSuccess("正在重启视觉管线…");
                  await invoke("restart_vision");
                  setVision(await invoke<VisionStatus>("get_vision_status"));
                  showSuccess("视觉管线已重启");
                } catch (e) {
                  showError(e);
                }
              }}
            >
              <RefreshCw size={14} />
              重启视觉管线
            </button>

            {/* #14：模型自管理 */}
            <div className="df-panel mb-4 rounded-lg border border-white/5 bg-white/[0.02] p-3">
              <div className="mb-2 flex items-center justify-between">
                <p className="flex items-center gap-1.5 text-sm font-semibold text-slate-300">
                  <Database size={13} className="text-slate-500" />
                  ONNX 模型
                </p>
                <button
                  type="button"
                  disabled={reseeding}
                  onClick={() => void reseedModels()}
                  className="df-btn rounded-lg border border-emerald-600/50 bg-emerald-500/10 px-2 py-1 text-[11px] font-semibold text-emerald-300 hover:bg-emerald-500/20 disabled:opacity-50"
                >
                  {reseeding ? "复制中…" : "从安装目录复制种子模型"}
                </button>
              </div>
              {models.length === 0 ? (
                <p className="text-xs text-slate-500">
                  data/models 下未发现 ONNX。点击「从安装目录复制种子模型」会从安装/资源旁 seed 目录复制内置 ONNX；视觉会自动重试加载。
                </p>
              ) : (
                <ul className="space-y-0.5 text-xs text-slate-400">
                  {models.map((m) => (
                    <li key={m.name} className="flex items-center justify-between gap-2">
                      <span className="truncate font-mono">{m.name}</span>
                      <span className="shrink-0 text-slate-600">
                        {(m.size / 1024 / 1024).toFixed(1)} MB
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </div>

            <label className="mb-3 block text-sm text-slate-300">
              白名单违规处置
              <select
                className="mt-1 w-full rounded-lg border border-white/10 bg-slate-900 px-3 py-2 text-sm"
                value={settings.whitelist_action || "report"}
                onChange={(e) =>
                  setSettings({ ...settings, whitelist_action: e.target.value })
                }
              >
                <option value="report">仅提示（默认）</option>
                <option value="minimize">强制最小化窗口</option>
                <option value="close_report">请求关闭窗口并提示</option>
              </select>
              <span className="mt-1 block text-xs text-slate-500">
                不杀进程；最小化/关闭仅作用于该进程的顶级可见窗口。
              </span>
            </label>

            <p className="mb-1 text-sm font-semibold text-slate-300">
              白名单进程
            </p>
            <input
              type="text"
              placeholder="搜索进程名…"
              className="df-input mb-2 w-full rounded-lg px-2 py-1 text-xs"
              value={processSearch}
              onChange={(e) => setProcessSearch(e.target.value)}
            />
            <div className="mb-4 max-h-48 overflow-auto rounded-lg border border-white/5 p-2 text-xs text-slate-400 df-scroll">
              {(processes.length ? processes : whitelist)
                .filter((p) =>
                  processSearch.trim() === ""
                    ? true
                    : p.toLowerCase().includes(processSearch.toLowerCase()),
                )
                .slice(0, 60)
                .map((p) => (
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

            <p className="mb-2 text-xs text-slate-500">
              数据目录：{dataDir || "（未加载）"}
              {pathMode ? <span className="ml-1 text-slate-600">（{pathMode}）</span> : null}
            </p>

            {/* #23/#29/#33 系统集成 */}
            <div className="mb-4 rounded-xl border border-slate-700/60 bg-slate-900/40 p-3">
              <p className="mb-2 text-xs font-semibold text-slate-300">系统集成</p>
              <label className="mb-2 flex items-center gap-2 text-xs text-slate-300">
                <input
                  type="checkbox"
                  checked={settings.auto_start}
                  onChange={() => setSettings({ ...settings, auto_start: !settings.auto_start })}
                />
                开机/登录时自动启动
              </label>
              <label className="mb-2 flex items-center gap-2 text-xs text-slate-300">
                <input
                  type="checkbox"
                  checked={settings.notifications_enabled}
                  onChange={() =>
                    setSettings({
                      ...settings,
                      notifications_enabled: !settings.notifications_enabled,
                    })
                  }
                />
                启用系统通知
              </label>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => void testNotification()}
                  className="df-btn rounded-lg border border-slate-600 px-3 py-1.5 text-xs text-slate-200 hover:bg-white/5"
                >
                  测试通知
                </button>
                <button
                  type="button"
                  onClick={() => void checkUpdates()}
                  className="df-btn rounded-lg border border-slate-600 px-3 py-1.5 text-xs text-slate-200 hover:bg-white/5"
                >
                  检查更新
                </button>
              </div>
              <p className="mt-2 text-[11px] text-slate-500">
                自动更新需在 tauri.conf.json 配置有效 endpoint 与 pubkey 后才可下载安装。
              </p>
            </div>

            {/* #28 B1：数据导出 / 清空 / #30 静音 / #34 备份恢复 */}
            <div className="mb-4 rounded-xl border border-slate-700/60 bg-slate-900/40 p-3">
              <p className="mb-2 text-xs font-semibold text-slate-300">数据管理</p>
              <label className="mb-3 flex items-center gap-2 text-xs text-slate-300">
                <input
                  type="checkbox"
                  checked={settings.sound_muted}
                  onChange={() => setSettings({ ...settings, sound_muted: !settings.sound_muted })}
                />
                静音全部提示音
              </label>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => void exportAllData()}
                  className="df-btn rounded-lg border border-slate-600 px-3 py-1.5 text-xs text-slate-200 hover:bg-white/5"
                >
                  导出全部数据 (JSON)
                </button>
                <button
                  type="button"
                  onClick={() => void clearAllData(false)}
                  className="df-btn rounded-lg border border-orange-700/50 px-3 py-1.5 text-xs text-orange-200 hover:bg-orange-950/40"
                >
                  清空历史记录
                </button>
                <button
                  type="button"
                  onClick={() => void clearAllData(true)}
                  className="df-btn rounded-lg border border-red-700/50 px-3 py-1.5 text-xs text-red-200 hover:bg-red-950/40"
                >
                  清空并重置设置
                </button>
              </div>
              <div className="mt-2 flex flex-wrap gap-2 border-t border-slate-700/40 pt-2">
                <button
                  type="button"
                  onClick={() => void backupSettings()}
                  className="df-btn rounded-lg border border-slate-600 px-3 py-1.5 text-xs text-slate-200 hover:bg-white/5"
                >
                  备份设置 (JSON)
                </button>
                <button
                  type="button"
                  onClick={() => void restoreSettings()}
                  className="df-btn rounded-lg border border-slate-600 px-3 py-1.5 text-xs text-slate-200 hover:bg-white/5"
                >
                  从备份恢复
                </button>
              </div>
            </div>

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
              <h2 className="text-lg font-bold text-slate-100">
                {reportWeek === 0 ? "本周" : `近 ${reportWeek + 1} 周前`}正向周报
              </h2>
              <div className="flex items-center gap-2">
                <select
                  className="df-input rounded-lg px-2 py-1 text-xs"
                  value={reportWeek}
                  onChange={(e) => void openReport(Number(e.target.value))}
                >
                  {[0, 1, 2, 3].map((w) => (
                    <option key={w} value={w}>
                      {w === 0 ? "本周" : `${w} 周前`}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  onClick={() => void loadL3Reasons()}
                  disabled={l3Loading}
                  title="查看 L3 原因记录"
                  className="df-btn rounded-lg border border-white/10 px-2 py-1 text-xs text-slate-300 hover:bg-white/5 disabled:opacity-50"
                >
                  <History size={13} />
                </button>
                <button
                  type="button"
                  onClick={() => setReportOpen(false)}
                  className="rounded-lg p-1 text-slate-400 hover:bg-white/5"
                >
                  <X size={18} />
                </button>
              </div>
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
            <div className="mt-4 flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() => void exportPng()}
                disabled={exporting}
                className="inline-flex items-center gap-1.5 rounded-lg bg-emerald-600 px-3 py-1.5 text-xs font-semibold text-white shadow transition hover:bg-emerald-500 disabled:opacity-50"
              >
                <Download size={14} />
                {exporting ? "导出中…" : "导出 PNG"}
              </button>
              <button
                type="button"
                onClick={() => void openExportsDir()}
                className="inline-flex items-center gap-1.5 rounded-lg bg-white/10 px-3 py-1.5 text-xs font-semibold text-slate-200 shadow transition hover:bg-white/20"
              >
                <FolderOpen size={14} />
                打开所在目录
              </button>
              {reportPngPath ? (
                <span className="max-w-full truncate text-[11px] text-emerald-300">
                  {reportPngPath}
                </span>
              ) : null}
            </div>
            <p className="mt-3 text-xs text-slate-500">
              数据目录：{dataDir || "点击「打开所在目录」查看"}
              {pathMode ? <span className="ml-1 text-slate-600">（{pathMode}）</span> : null}
            </p>
            {/* #16：L3 原因记录 */}
            {l3Reasons.length > 0 && (
              <div className="mt-3 rounded-lg border border-white/5 bg-white/[0.02] p-3">
                <p className="mb-1 flex items-center gap-1.5 text-xs font-semibold text-slate-300">
                  <Database size={12} className="text-slate-500" />
                  近期 L3 原因（点下拉按钮可刷新）
                </p>
                <ul className="max-h-32 space-y-0.5 overflow-auto df-scroll text-xs text-slate-400">
                  {l3Reasons.map((r, i) => (
                    <li key={i} className="flex gap-2">
                      <span className="shrink-0 font-mono text-slate-600">{r[0]}</span>
                      <span className="text-slate-300">{r[1]}</span>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        </div>
      )}
      <ToastContainer toasts={toasts} onRemove={remove} />
    </div>
  );
};
