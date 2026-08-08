import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Timer, Play, SkipForward, Pin } from "lucide-react";
import { useTauriEvent } from "../hooks/useTauriEvents";
import { EVT, type SettingsRecord, type SystemState } from "../types/tauri-ipc";

export const FloatingClockWindow: React.FC = () => {
  const [elapsed, setElapsed] = useState(0);
  const [reason, setReason] = useState("");
  const [floor, setFloor] = useState(180);
  // #45：置顶可调
  const [pinned, setPinned] = useState(true);

  useTauriEvent<SystemState>(
    EVT.fsm,
    (s) => {
      if (s.kind === "temporary_pause") {
        setElapsed(s.elapsed_secs);
        setReason(s.reason);
      }
    },
    [],
  );

  React.useEffect(() => {
    invoke<SystemState>("get_fsm_state")
      .then((s) => {
        if (s.kind === "temporary_pause") {
          setElapsed(s.elapsed_secs);
          setReason(s.reason);
        }
      })
      .catch(() => {});
    invoke<SettingsRecord>("get_settings")
      .then((s) => setFloor(s.debt_floor_secs || 180))
      .catch(() => {});
  }, []);

  const fmt = (secs: number) => {
    const m = Math.floor(secs / 60)
      .toString()
      .padStart(2, "0");
    const s = (secs % 60).toString().padStart(2, "0");
    return `${m}:${s}`;
  };

  const debtPreview = Math.max(elapsed, floor);

  const togglePin = async () => {
    const next = !pinned;
    setPinned(next);
    try {
      await getCurrentWindow().setAlwaysOnTop(next);
    } catch {}
  };

  return (
    <div
      className="box-border flex h-screen w-screen flex-col overflow-hidden select-none"
      style={{
        background: "linear-gradient(165deg, #141b24 0%, #0b1016 100%)",
        border: "1px solid rgba(255,255,255,0.08)",
      }}
      data-tauri-drag-region
    >
      {/* Header row */}
      <div
        className="flex shrink-0 items-center gap-1.5 px-3 pt-2.5 pb-1"
        data-tauri-drag-region
      >
        <Timer size={12} className="shrink-0 text-amber-400 animate-pulse" />
        <span className="text-[11px] font-bold tracking-wide text-amber-300">
          临时休息
        </span>
        <span
          className="ml-auto max-w-[55%] truncate text-[10px] text-slate-500"
          title={reason || "无原因"}
        >
          {reason || "无原因"}
        </span>
        {/* #45：置顶快捷 */}
        <button
          type="button"
          onClick={togglePin}
          className="df-btn shrink-0 rounded px-1 text-slate-500 hover:text-slate-200"
          title={pinned ? "取消置顶" : "置顶"}
        >
          <Pin size={11} className={pinned ? "text-amber-400" : ""} />
        </button>
      </div>

      {/* Timer + debt, single compact block */}
      <div
        className="flex min-h-0 flex-1 flex-col items-center justify-center px-3"
        data-tauri-drag-region
      >
        <div className="font-mono text-[42px] font-black leading-none tracking-wider text-slate-50">
          {fmt(elapsed)}
        </div>
        <div className="mt-2 flex items-center gap-2 text-[10px] text-slate-500">
          <span className="rounded-full bg-white/5 px-2 py-0.5">
            预计债务{" "}
            <span className="font-mono text-amber-200/90">
              {fmt(debtPreview)}
            </span>
          </span>
          <span className="text-slate-600">下限 {fmt(floor)}</span>
        </div>
      </div>

      {/* Actions — side by side to save height */}
      <div className="flex shrink-0 gap-1.5 px-2.5 pb-2.5 pt-1">
        <button
          type="button"
          onClick={() => invoke("resume_focus_session")}
          className="df-btn flex flex-1 items-center justify-center gap-1 rounded-lg bg-emerald-500 py-1.5 text-xs font-extrabold text-slate-950 shadow hover:bg-emerald-400"
        >
          <Play size={12} />
          恢复
        </button>
        <button
          type="button"
          onClick={() => invoke("skip_debt_and_resume")}
          className="df-btn flex flex-1 items-center justify-center gap-1 rounded-lg border border-slate-600/80 bg-slate-900/60 py-1.5 text-[11px] text-slate-300 hover:bg-slate-800"
          title="仍会计入债务并记 SKIP_DEBT"
        >
          <SkipForward size={11} />
          跳过
        </button>
      </div>
    </div>
  );
};
