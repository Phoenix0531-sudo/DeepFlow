import React from "react";
import { Mic, MicOff } from "lucide-react";
import { useSpeechRecognition } from "../hooks/useSpeechRecognition";

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSubmit: () => void;
  placeholder?: string;
  buttonText?: string;
}

/**
 * 打字输入 + 内置 Web 语音识别。
 * - 浏览器不支持时只显示打字框
 * - 支持时右侧多一个麦克风按钮：点击开始/停止识别，识别结果实时回填
 */
export const VoiceInputBox: React.FC<Props> = ({
  value,
  onChange,
  onSubmit,
  placeholder = "输入临时原因…",
  buttonText = "确认",
}) => {
  const { supported, listening, transcript, err, start, stop } =
    useSpeechRecognition("zh-CN");

  // 识别结果实时回填：override 成识别文本（用户可继续手动编辑）
  React.useEffect(() => {
    if (transcript) onChange(transcript);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [transcript]);

  const toggleMic = () => {
    if (listening) stop();
    else start();
  };

  const micSlot = supported ? (
    <button
      type="button"
      onClick={toggleMic}
      title={listening ? "停止语音识别" : "语音输入"}
      className={`absolute right-24 h-10 w-10 rounded-xl flex items-center justify-center shadow-lg transition active:scale-95 ${
        listening
          ? "bg-red-500 text-white animate-pulse"
          : "bg-white/10 text-white hover:bg-white/20"
      }`}
    >
      {listening ? <Mic size={20} /> : <MicOff size={20} />}
    </button>
  ) : null;

  return (
    <div className="relative flex w-full max-w-xl items-center">
      <form
        className="relative flex w-full items-center"
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
        {micSlot}
        <button
          type="submit"
          className="absolute right-2 h-10 rounded-xl bg-amber-500 px-4 font-bold text-slate-950 shadow-lg transition active:scale-95 hover:bg-amber-400"
        >
          {buttonText}
        </button>
      </form>
      {err ? (
        <div className="absolute -bottom-6 left-0 text-xs text-red-300">
          {err}
        </div>
      ) : null}
      {!supported ? (
        <div className="absolute -bottom-6 left-0 text-xs text-slate-400">
          当前 WebView 不支持语音识别，请键盘输入。
        </div>
      ) : null}
    </div>
  );
};
