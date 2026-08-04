import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "../hooks/useTauriEvents";
import { EVT, type SystemState } from "../types/tauri-ipc";

export const FloatingClockWindow: React.FC = () => {
  const [elapsed, setElapsed] = useState(0);
  const [reason, setReason] = useState("");

  useTauriEvent<SystemState>(EVT.fsm, (s) => {
    if (s.kind === "temporary_pause") {
      setElapsed(s.elapsed_secs);
      setReason(s.reason);
    }
  }, []);

  React.useEffect(() => {
    invoke<SystemState>("get_fsm_state")
      .then((s) => {
        if (s.kind === "temporary_pause") {
          setElapsed(s.elapsed_secs);
          setReason(s.reason);
        }
      })
      .catch(() => {});
  }, []);

  const fmt = (secs: number) => {
    const m = Math.floor(secs / 60)
      .toString()
      .padStart(2, "0");
    const s = (secs % 60).toString().padStart(2, "0");
    return `${m}:${s}`;
  };

  return (
    <div
      className="flex h-screen w-screen items-center justify-center bg-transparent select-none"
      data-tauri-drag-region
    >
      <div className="flex w-48 flex-col items-center rounded-2xl border border-slate-700/50 bg-slate-900/90 p-4 text-white shadow-2xl backdrop-blur-md">
        <div className="mb-1 flex items-center gap-2 text-sm font-bold text-amber-400">
          <span className="animate-pulse">⏱️ 临时休息</span>
        </div>
        {reason && (
          <p className="mb-1 max-w-full truncate text-xs text-slate-400" title={reason}>
            {reason}
          </p>
        )}
        <div className="my-1 font-mono text-3xl font-black tracking-widest text-slate-100">
          {fmt(elapsed)}
        </div>
        <p className="mb-2 text-[10px] text-slate-500">建议约 10 分钟 · 债务按实际与下限</p>
        <button
          type="button"
          onClick={() => invoke("resume_focus_session")}
          className="w-full rounded-xl bg-emerald-500 py-2 text-sm font-extrabold text-slate-950 shadow-md transition active:scale-95 hover:bg-emerald-400"
        >
          恢复刷题
        </button>
        <button
          type="button"
          onClick={() => invoke("skip_debt_and_resume")}
          className="mt-2 w-full rounded-xl border border-slate-600 py-1.5 text-xs text-slate-300 hover:bg-slate-800"
          title="仍会计入债务并记 SKIP_DEBT"
        >
          跳过（仍记债务）
        </button>
      </div>
    </div>
  );
};
