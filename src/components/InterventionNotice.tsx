import React from "react";
import { motion } from "framer-motion";

interface Props {
  level: 0 | 1 | 2 | 3;
  escalateSecs?: number;
}

export const InterventionNotice: React.FC<Props> = ({ level, escalateSecs = 0 }) => {
  if (level === 0) return null;
  const messages = [
    "",
    "⚠️ 检测到长时间拿取手机，请保持专注",
    "🔔 已持续离开专注状态，可说明原因或点「我知道了」",
    "🚨 严重干预：请放下手机或输入原因",
  ];
  const intensity = level === 3 ? Math.min(1, escalateSecs / 60) : level / 3;
  return (
    <motion.div
      className="pointer-events-none absolute inset-0 flex items-start justify-center pt-24"
      initial={{ opacity: 0 }}
      animate={{ opacity: 0.5 + intensity * 0.5 }}
    >
      <div
        className={`rounded-2xl px-6 py-3 text-lg font-semibold text-white shadow-2xl ${
          level === 1
            ? "bg-amber-600/80"
            : level === 2
              ? "bg-orange-600/85"
              : "bg-red-700/90"
        }`}
      >
        {messages[level]}
        {level === 3 && escalateSecs > 0 ? ` · ${escalateSecs}s` : null}
      </div>
    </motion.div>
  );
};
