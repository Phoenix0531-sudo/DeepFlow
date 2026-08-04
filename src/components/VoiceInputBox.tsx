import React from "react";

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSubmit: () => void;
  placeholder?: string;
  buttonText?: string;
}

/** 纯打字输入（系统语音 IME 可外挂）。 */
export const VoiceInputBox: React.FC<Props> = ({
  value,
  onChange,
  onSubmit,
  placeholder = "输入临时原因…",
  buttonText = "确认",
}) => (
  <form
    className="relative flex w-full max-w-xl items-center"
    onSubmit={(e) => {
      e.preventDefault();
      onSubmit();
    }}
  >
    <input
      type="text"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className="h-14 w-full rounded-2xl border border-white/20 bg-white/10 pl-5 pr-28 text-lg text-white placeholder-slate-400 shadow-xl backdrop-blur-md focus:outline-none focus:ring-2 focus:ring-amber-400/50"
    />
    <button
      type="submit"
      className="absolute right-2 h-10 rounded-xl bg-amber-500 px-4 font-bold text-slate-950 shadow-lg transition active:scale-95 hover:bg-amber-400"
    >
      {buttonText}
    </button>
  </form>
);
