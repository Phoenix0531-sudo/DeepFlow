import React from "react";

/** P1 接入真实预览；P0 占位。 */
export const CameraPreview: React.FC<{ label?: string }> = ({ label }) => (
  <div className="flex h-48 w-full items-center justify-center rounded-2xl border border-dashed border-slate-600 bg-slate-900/60 text-slate-400">
    {label ?? "摄像头预览（P1）"}
  </div>
);
