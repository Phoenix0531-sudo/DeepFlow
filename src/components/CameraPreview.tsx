import React, { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Camera, RefreshCw } from "lucide-react";
import type { RoiRect, VisionStatus } from "../types/tauri-ipc";

interface Props {
  label?: string;
  device?: string;
  autoStart?: boolean;
  enableRoi?: boolean;
  roi?: RoiRect | null;
  onRoiChange?: (roi: RoiRect) => void;
  className?: string;
  frameClassName?: string;
  pollMs?: number;
  compact?: boolean;
}

const defaultRoi = (): RoiRect => ({ x: 0.1, y: 0.1, w: 0.8, h: 0.8 });

export const CameraPreview: React.FC<Props> = ({
  label,
  device,
  autoStart = true,
  enableRoi = false,
  roi,
  onRoiChange,
  className = "",
  frameClassName = "h-56",
  pollMs = 350,
  compact = false,
}) => {
  const [src, setSrc] = useState<string | null>(null);
  const [status, setStatus] = useState("待机");
  const [err, setErr] = useState("");
  const [starting, setStarting] = useState(false);
  const [localRoi, setLocalRoi] = useState<RoiRect>(roi ?? defaultRoi());
  const boxRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ ox: number; oy: number; mode: "draw" | null }>({
    ox: 0,
    oy: 0,
    mode: null,
  });
  const bootToken = useRef(0);
  const startedForDevice = useRef<string | null>(null);

  useEffect(() => {
    if (roi) setLocalRoi(roi);
  }, [roi]);

  const startPreview = useCallback(async () => {
    bootToken.current += 1;
    const token = bootToken.current;
    const dev = device && device.length ? device : "0";
    setStarting(true);
    setErr("");
    setStatus("启动摄像头…");
    try {
      await invoke("start_vision_preview", {
        device: device && device.length ? device : null,
      });
      if (bootToken.current === token) {
        startedForDevice.current = dev;
        setStatus("预览中");
      }
    } catch (e) {
      if (bootToken.current === token) {
        setErr(String(e));
        setStatus("启动失败");
      }
    } finally {
      if (bootToken.current === token) setStarting(false);
    }
  }, [device]);

  // 仅在 autoStart / device 变化时启动，避免 starting/err 依赖形成重启环
  useEffect(() => {
    if (!autoStart) return;
    const dev = device && device.length ? device : "0";
    if (startedForDevice.current === dev) return;
    void startPreview();
  }, [autoStart, device, startPreview]);

  // 独立轮询预览帧与状态
  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      try {
        const b64 = await invoke<string | null>("get_vision_preview");
        if (!cancelled && b64) {
          setSrc(`data:image/jpeg;base64,${b64}`);
        }
        const st = await invoke<VisionStatus>("get_vision_status");
        if (!cancelled) {
          const det = st.last_detection;
          const bits = [
            st.running ? "运行" : "停止",
            st.detector,
            det
              ? `phone=${det.has_phone ? "Y" : "n"} ${det.phone_score.toFixed(2)} bright=${det.phone_brightness}`
              : null,
            `hold=${st.hold_secs}s`,
          ].filter(Boolean);
          setStatus(bits.join(" · "));
        }
      } catch {
        /* browser preview */
      }
      if (!cancelled) timer = window.setTimeout(tick, pollMs);
    };

    void tick();
    return () => {
      cancelled = true;
      if (timer) window.clearTimeout(timer);
    };
  }, [pollMs]);

  const rel = (e: React.PointerEvent) => {
    const el = boxRef.current;
    if (!el) return { x: 0, y: 0 };
    const r = el.getBoundingClientRect();
    return {
      x: Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)),
      y: Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)),
    };
  };

  const onPointerDown = (e: React.PointerEvent) => {
    if (!enableRoi) return;
    const p = rel(e);
    dragRef.current = { ox: p.x, oy: p.y, mode: "draw" };
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
    setLocalRoi({ x: p.x, y: p.y, w: 0.02, h: 0.02 });
  };

  const onPointerMove = (e: React.PointerEvent) => {
    if (!enableRoi || dragRef.current.mode !== "draw") return;
    const p = rel(e);
    const x = Math.min(dragRef.current.ox, p.x);
    const y = Math.min(dragRef.current.oy, p.y);
    const w = Math.abs(p.x - dragRef.current.ox);
    const h = Math.abs(p.y - dragRef.current.oy);
    setLocalRoi({ x, y, w: Math.max(0.02, w), h: Math.max(0.02, h) });
  };

  const onPointerUp = () => {
    if (!enableRoi || dragRef.current.mode !== "draw") return;
    dragRef.current.mode = null;
    onRoiChange?.(localRoi);
  };

  return (
    <div className={`space-y-2 ${className}`}>
      <div
        ref={boxRef}
        className={`relative flex w-full items-center justify-center overflow-hidden rounded-xl border border-white/10 bg-[#0a1018] text-slate-400 select-none ${frameClassName}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={onPointerUp}
      >
        {src ? (
          <img
            src={src}
            alt="camera"
            className="h-full w-full object-contain"
            draggable={false}
          />
        ) : (
          <div className="flex flex-col items-center gap-2 px-4 text-center">
            <Camera size={22} className="opacity-50" />
            <span className="text-sm">{label ?? "摄像头预览"}</span>
            {starting && <span className="text-xs text-slate-500">连接中…</span>}
          </div>
        )}
        {enableRoi && (
          <div
            className="pointer-events-none absolute border-2 border-amber-400/90 bg-amber-400/10"
            style={{
              left: `${localRoi.x * 100}%`,
              top: `${localRoi.y * 100}%`,
              width: `${localRoi.w * 100}%`,
              height: `${localRoi.h * 100}%`,
            }}
          />
        )}
        <button
          type="button"
          className="df-btn absolute right-2 top-2 rounded-lg border border-white/10 bg-black/50 p-1.5 text-slate-200 backdrop-blur hover:bg-black/70"
          title="重新启动预览"
          onClick={(e) => {
            e.stopPropagation();
            startedForDevice.current = null;
            void startPreview();
          }}
        >
          <RefreshCw size={14} className={starting ? "animate-spin" : ""} />
        </button>
      </div>
      {!compact && (
        <p className="truncate font-mono text-[11px] text-slate-500">{status}</p>
      )}
      {err && (
        <div className="flex items-start justify-between gap-2 rounded-lg border border-red-500/30 bg-red-950/40 px-2 py-1.5 text-xs text-red-300">
          <span className="min-w-0 break-all">{err}</span>
          <button
            type="button"
            className="shrink-0 underline"
            onClick={() => {
              startedForDevice.current = null;
              void startPreview();
            }}
          >
            重试
          </button>
        </div>
      )}
    </div>
  );
};
