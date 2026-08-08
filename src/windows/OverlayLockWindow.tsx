import React, { useCallback, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import { InterventionNotice } from "../components/InterventionNotice";
import { VoiceInputBox } from "../components/VoiceInputBox";
import { CameraPreview } from "../components/CameraPreview";
import { useTauriEvent } from "../hooks/useTauriEvents";
import { playSound } from "../lib/sounds";
import {
  EVT,
  type SettingsRecord,
  type SystemState,
  type VisionStatus,
} from "../types/tauri-ipc";

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

function isForceExitPhrase(text: string): boolean {
  const t = text.trim().toLowerCase();
  return (
    t === "测试" ||
    t === "test" ||
    t === "退出" ||
    t === "退出测试" ||
    t === "exit" ||
    t === "exit test" ||
    t === "force exit" ||
    t === "强制退出"
  );
}

async function safeInvoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<{ ok: true; value: T } | { ok: false; error: string }> {
  try {
    const value = await invoke<T>(cmd, args);
    return { ok: true, value };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export const OverlayLockWindow: React.FC = () => {
  const [state, setState] = useState<SystemState | null>(null);
  const [reason, setReason] = useState("");
  const [l2Ack, setL2Ack] = useState(false);
  const [choiceOpen, setChoiceOpen] = useState(false);
  const [testMode, setTestMode] = useState(false);
  const [vision, setVision] = useState<VisionStatus | null>(null);
  const [cameraName, setCameraName] = useState("");
  const [hint, setHint] = useState("");
  const [severePulse, setSeverePulse] = useState(false);
  const [exiting, setExiting] = useState(false);
  const exitOnce = useRef(false);

  const forceExit = useCallback(async (why: string) => {
    if (exitOnce.current) return;
    exitOnce.current = true;
    setExiting(true);
    setHint(`正在强制退出（${why}）…`);
    // 连打三条逃生命令，任一成功即可；失败也不阻塞 UI
    const cmds = ["force_exit_session", "test_exit_session", "close_overlay_window"] as const;
    for (const c of cmds) {
      const r = await safeInvoke(c);
      if (r.ok) {
        setHint("已退出");
        // 再关一次遮罩，防止 FSM 已 Idle 但窗还在
        await safeInvoke("close_overlay_window");
        return;
      }
    }
    setHint("退出失败：请连按两次 ESC，或结束 deepflow 进程");
    exitOnce.current = false;
    setExiting(false);
  }, []);

  useTauriEvent<SystemState>(
    EVT.fsm,
    (s) => {
      setState(s);
      if (s.kind === "await_session_end_choice") setChoiceOpen(true);
      if (s.kind !== "intervention_level2") setL2Ack(false);
      // 会话已结束但遮罩还在 → 自动逃生，避免全屏卡死
      if (s.kind === "idle") {
        void forceExit("状态已 Idle");
      }
      if (s.kind === "temporary_pause") {
        // 休息态应由后端 HideOverlay；前端再兜底关一次
        void safeInvoke("close_overlay_window");
      }
    },
    [forceExit],
  );

  useTauriEvent<string>(
    EVT.playSound,
    (kind) => {
      playSound(kind);
      if (kind === "severe") {
        setSeverePulse(true);
        window.setTimeout(() => setSeverePulse(false), 1200);
      }
    },
    [],
  );

  React.useEffect(() => {
    void safeInvoke<SystemState>("get_fsm_state").then((r) => {
      if (r.ok) {
        setState(r.value);
        if (r.value.kind === "idle") void forceExit("启动时已 Idle");
      }
    });
    // 样式失败不弹错——这是上次 "failed to acquire webview reference" 的来源之一
    void safeInvoke("apply_overlay_native_style", { label: "overlay" });
    void safeInvoke<SettingsRecord>("get_settings").then((r) => {
      if (r.ok) {
        setTestMode(!!r.value.test_mode);
        setCameraName(r.value.camera_name || "");
      }
    });
    const id = window.setInterval(() => {
      void safeInvoke<VisionStatus>("get_vision_status").then((r) => {
        if (r.ok) setVision(r.value);
      });
    }, 800);
    return () => window.clearInterval(id);
  }, [forceExit]);

  const level = levelOf(state);
  const remaining =
    state?.kind === "focus_active" ? state.remaining_secs : null;
  const escalate =
    state?.kind === "intervention_level3" ? state.escalate_elapsed_secs : 0;
  const observe =
    state?.kind === "intervention_level1"
      ? state.observe_remaining_secs
      : null;

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
    const text = reason.trim();
    if (!text || exiting) return;

    // 「测试」/「退出」/「强制退出」→ 始终可退，不依赖 test_mode
    if (isForceExitPhrase(text)) {
      setReason("");
      await forceExit("对话框");
      return;
    }

    if (level === 3) {
      const r = await safeInvoke("submit_l3_reason", { reason: text });
      if (!r.ok) {
        setHint(r.error);
        return;
      }
    } else {
      const r = await safeInvoke("request_temporary_pause", { reason: text });
      if (!r.ok) {
        setHint(r.error);
        return;
      }
    }
    setReason("");
    setHint("");
  };

  return (
    <div
      className={`relative flex h-screen w-screen flex-col items-center justify-center overflow-hidden select-none backdrop-blur-2xl transition-colors duration-700 ${bg} ${
        severePulse ? "ring-8 ring-inset ring-red-500/60" : ""
      }`}
    >
      {/* 常驻强制退出：不依赖 test_mode，防卡死 */}
      <button
        type="button"
        disabled={exiting}
        className="absolute left-4 top-4 z-[60] rounded-xl border border-red-500/50 bg-red-950/80 px-4 py-2 text-sm font-bold text-red-100 shadow-xl hover:bg-red-800/90 disabled:opacity-50"
        onClick={() => void forceExit("按钮")}
      >
        {exiting ? "退出中…" : "强制退出"}
      </button>
      <p className="absolute left-4 top-14 z-[60] max-w-[12rem] text-[10px] leading-snug text-slate-400">
        或输入「测试」/「退出」· 双击 ESC
      </p>

      <AnimatePresence>
        {level > 0 && (
          <motion.div
            className="animate-breath pointer-events-none absolute h-[600px] w-[600px] rounded-full bg-gradient-to-r from-amber-500 to-red-600 blur-3xl"
            initial={{ opacity: 0 }}
            animate={{ opacity: 0.2 + level * 0.08 + (severePulse ? 0.2 : 0) }}
            exit={{ opacity: 0 }}
          />
        )}
      </AnimatePresence>

      <InterventionNotice level={level} escalateSecs={escalate} />

      {testMode && (
        <div className="absolute right-6 top-6 z-20 w-72 rounded-2xl border border-white/15 bg-black/55 p-3 shadow-2xl backdrop-blur-md">
          <div className="mb-2 flex items-center justify-between text-xs text-amber-300">
            <span>测试预览</span>
            <span className="font-mono text-slate-300">
              hold={vision?.hold_secs ?? 0}s
              {vision?.last_detection
                ? ` · phone=${vision.last_detection.has_phone ? "Y" : "n"} ${vision.last_detection.phone_score.toFixed(2)}`
                : ""}
            </span>
          </div>
          <CameraPreview
            device={cameraName || undefined}
            autoStart={false}
            compact
            frameClassName="h-36"
            pollMs={500}
            label="检测画面"
          />
          <div className="mt-2 flex flex-wrap gap-1.5">
            {([1, 2, 3] as const).map((lv) => (
              <button
                key={lv}
                type="button"
                className="rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1 text-[10px] text-amber-200 hover:bg-amber-500/20"
                onClick={async () => {
                  const r = await safeInvoke("test_inject_level", { level: lv });
                  if (!r.ok) setHint(r.error);
                }}
              >
                L{lv}
              </button>
            ))}
            <button
              type="button"
              className="rounded-md border border-red-500/40 bg-red-500/15 px-2 py-1 text-[10px] text-red-200 hover:bg-red-500/25"
              onClick={() => void forceExit("测试面板")}
            >
              强制退出
            </button>
          </div>
        </div>
      )}

      <div className="z-10 px-8 text-center">
        <h1 className="mb-3 font-sans text-5xl font-black tracking-wider text-white drop-shadow-2xl md:text-7xl">
          {level === 0
            ? "专注刷题中"
            : level === 3
              ? "请立即归位"
              : "保持专注"}
        </h1>
        {remaining != null && (
          <p className="font-mono text-4xl font-bold tracking-widest text-amber-300">
            {fmt(remaining)}
          </p>
        )}
        {observe != null && (
          <p className="mt-2 font-mono text-2xl font-semibold text-amber-200/90">
            观察期剩余 {observe}s
          </p>
        )}
        <p className="mt-4 text-lg font-light text-slate-300">
          {level === 0 &&
            (testMode
              ? "测试中：注入 L1/L2/L3，或点左上角「强制退出」"
              : "双击 ESC 紧急退出 · 输入原因可临时休息")}
          {level === 1 && "观察期：放下手机即可恢复"}
          {level === 2 && !l2Ack && "可忽略音效，但计时仍按持握累计升级"}
          {level === 2 && l2Ack && "已记录「我知道了」，请尽快放下手机"}
          {level === 3 && "可输入原因进入休息，或放下手机自动恢复"}
        </p>
        {hint && <p className="mt-2 text-sm text-amber-300/90">{hint}</p>}
      </div>

      {level === 2 && !l2Ack && (
        <button
          type="button"
          className="z-10 mt-6 rounded-xl bg-white/15 px-6 py-2 font-semibold text-white backdrop-blur hover:bg-white/25"
          onClick={async () => {
            const r = await safeInvoke("acknowledge_level2");
            if (r.ok) setL2Ack(true);
            else setHint(`确认失败：${r.error} — 可用「强制退出」`);
          }}
        >
          我知道了，继续
        </button>
      )}

      {level === 3 && (
        <div className="z-10 mt-4 flex flex-wrap justify-center gap-2 px-4">
          {["查资料/学习", "休息疲劳", "消息回复", "临时处理", "其他"].map((t) => (
            <button
              key={t}
              type="button"
              className="df-btn rounded-full border border-red-500/30 bg-red-500/10 px-3 py-1 text-xs text-red-200 hover:bg-red-500/20"
              onClick={() => setReason(t)}
            >
              {t}
            </button>
          ))}
        </div>
      )}

      <div className="z-10 mt-10 w-full max-w-xl px-4">
        <VoiceInputBox
          value={reason}
          onChange={setReason}
          onSubmit={submitPause}
          placeholder='输入原因，或输入「测试」/「退出」立即离开'
          buttonText={
            isForceExitPhrase(reason)
              ? "强制退出"
              : level === 3
                ? "说明原因并休息"
                : "休息/用机"
          }
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
                    const r = await safeInvoke("choose_session_end", {
                      choice: c,
                    });
                    if (r.ok) setChoiceOpen(false);
                    else setHint(r.error);
                  }}
                >
                  {label}
                </button>
              ))}
              <button
                type="button"
                className="rounded-xl border border-red-500/40 py-3 text-sm text-red-200 hover:bg-red-950/40"
                onClick={() => void forceExit("结束选择")}
              >
                强制退出
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
