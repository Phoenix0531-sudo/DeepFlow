import React, { useCallback, useRef, useState } from "react";

/** #21：统一 toast 错误提示。轻量自管队列，避免引入第三方。 */
export interface ToastItem {
  id: number;
  text: string;
  kind: "error" | "info" | "success";
}

let _seq = 0;

export function useToast() {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const timers = useRef<Record<number, number>>({});

  const remove = useCallback((id: number) => {
    setToasts((xs) => xs.filter((t) => t.id !== id));
    const h = timers.current[id];
    if (h) {
      window.clearTimeout(h);
      delete timers.current[id];
    }
  }, []);

  const push = useCallback(
    (text: string, kind: ToastItem["kind"] = "info", ttl = 4000) => {
      const id = ++_seq;
      setToasts((xs) => [...xs, { id, text, kind }]);
      if (ttl > 0) {
        timers.current[id] = window.setTimeout(() => remove(id), ttl);
      }
      return id;
    },
    [remove],
  );

  const showError = useCallback(
    (e: unknown) => push(String(e ?? "未知错误"), "error", 6000),
    [push],
  );
  const showSuccess = useCallback(
    (text: string) => push(text, "success", 3500),
    [push],
  );

  return { toasts, push, showError, showSuccess, remove };
}

export const ToastContainer: React.FC<{ toasts: ToastItem[]; onRemove: (id: number) => void }> = ({
  toasts,
  onRemove,
}) => {
  if (!toasts.length) return null;
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-[100] flex flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          onClick={() => onRemove(t.id)}
          className={
            "pointer-events-auto cursor-pointer rounded-lg border px-3 py-2 text-xs shadow-lg backdrop-blur " +
            (t.kind === "error"
              ? "border-red-500/40 bg-red-950/80 text-red-200"
              : t.kind === "success"
                ? "border-emerald-500/40 bg-emerald-950/80 text-emerald-200"
                : "border-white/10 bg-slate-900/80 text-slate-200")
          }
        >
          {t.text}
        </div>
      ))}
    </div>
  );
};
