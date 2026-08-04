import React, { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import { InterventionNotice } from "../components/InterventionNotice";
import { VoiceInputBox } from "../components/VoiceInputBox";
import { useTauriEvent } from "../hooks/useTauriEvents";
import { EVT, type SystemState } from "../types/tauri-ipc";

function levelOf(s: SystemState | null): 0 | 1 | 2 | 3 {
  if (!s) return 0;
  switch (s.kind) {
    case "intervention_level1":
      return 1;
    case "intervention_level2":
      return 2;
    case "intervention_level3":
      return 3;
    default:
      return 0;
  }
}

export const OverlayLockWindow: React.FC = () => {
  const [state, setState] = useState<SystemState | null>(null);
  const [reason, setReason] = useState("");
  const [l2Ack, setL2Ack] = useState(false);
  const [choiceOpen, setChoiceOpen] = useState(false);

  useTauriEvent<SystemState>(EVT.fsm, (s) => {
    setState(s);
    if (s.kind === "await_session_end_choice") setChoiceOpen(true);
    if (s.kind !== "intervention_level2") setL2Ack(false);
  }, []);

  React.useEffect(() => {
    invoke<SystemState>("get_fsm_state").then(setState).catch(() => {});
    invoke("apply_overlay_native_style", { label: "overlay" }).catch(() => {});
  }, []);

  const level = levelOf(state);
  const remaining =
    state?.kind === "focus_active" ? state.remaining_secs : null;
  const escalate =
    state?.kind === "intervention_level3" ? state.escalate_elapsed_secs : 0;

  const bg = useMemo(() => {
    if (level === 1) return "bg-amber-950/50";
    if (level === 2) return "bg-orange-950/65";
    if (level === 3) {
      const a = Math.min(0.92, 0.7 + escalate / 200);
      return `bg-red-950/${Math.round(a * 100)}`;
    }
    return "bg-slate-950/40";
  }, [level, escalate]);

  const fmt = (secs: number) => {
    const m = Math.floor(secs / 60)
      .toString()
      .padStart(2, "0");
    const s = (secs % 60).toString().padStart(2, "0");
    return `${m}:${s}`;
  };

  const submitPause = async () => {
    if (!reason.trim()) return;
    if (level === 3) {
      await invoke("submit_l3_reason", { reason });
    } else {
      await invoke("request_temporary_pause", { reason });
    }
    setReason("");
  };

  return (
    <div
      className={`relative flex h-screen w-screen flex-col items-center justify-center overflow-hidden select-none backdrop-blur-2xl transition-colors duration-700 ${bg}`}
    >
      <AnimatePresence>
        {level > 0 && (
          <motion.div
            className="animate-breath pointer-events-none absolute h-[600px] w-[600px] rounded-full bg-gradient-to-r from-amber-500 to-red-600 blur-3xl"
            initial={{ opacity: 0 }}
            animate={{ opacity: 0.2 + level * 0.08 }}
            exit={{ opacity: 0 }}
          />
        )}
      </AnimatePresence>

      <InterventionNotice level={level} escalateSecs={escalate} />

      <div className="z-10 px-8 text-center">
        <h1 className="mb-3 font-sans text-5xl font-black tracking-wider text-white drop-shadow-2xl md:text-7xl">
          {level === 0 ? "专注刷题中" : level === 3 ? "请立即归位" : "保持专注"}
        </h1>
        {remaining != null && (
          <p className="font-mono text-4xl font-bold tracking-widest text-amber-300">
            {fmt(remaining)}
          </p>
        )}
        <p className="mt-4 text-lg font-light text-slate-300">
          {level === 0 && "双击 ESC 可紧急挂起 · 输入原因可临时休息"}
          {level === 1 && "30 秒观察期：放下手机即可恢复"}
          {level === 2 && !l2Ack && "可忽略音效，但计时仍按持握累计升级"}
          {level === 2 && l2Ack && "已记录「我知道了」，请尽快放下手机"}
          {level === 3 && "可输入原因进入休息，或放下手机自动恢复"}
        </p>
      </div>

      {level === 2 && !l2Ack && (
        <button
          type="button"
          className="z-10 mt-6 rounded-xl bg-white/15 px-6 py-2 font-semibold text-white backdrop-blur hover:bg-white/25"
          onClick={async () => {
            await invoke("acknowledge_level2");
            setL2Ack(true);
          }}
        >
          我知道了，继续
        </button>
      )}

      <div className="z-10 mt-10 w-full max-w-xl px-4">
        <VoiceInputBox
          value={reason}
          onChange={setReason}
          onSubmit={submitPause}
          placeholder="输入临时原因（查资料 / 上厕所 / 用手机…）"
          buttonText={level === 3 ? "说明原因并休息" : "休息/用机"}
        />
      </div>

      {choiceOpen && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/70">
          <div className="w-full max-w-md rounded-3xl border border-white/10 bg-slate-900 p-8 shadow-2xl">
            <h2 className="mb-2 text-2xl font-bold">本轮专注结束</h2>
            <p className="mb-6 text-slate-400">接下来？</p>
            <div className="flex flex-col gap-3">
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
                  className="rounded-xl bg-amber-500 py-3 font-bold text-slate-950 hover:bg-amber-400"
                  onClick={async () => {
                    await invoke("choose_session_end", { choice: c });
                    setChoiceOpen(false);
                  }}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
