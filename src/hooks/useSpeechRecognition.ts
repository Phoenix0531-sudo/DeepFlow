import { useCallback, useEffect, useRef, useState } from "react";

interface SpeechRecognitionType {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  start: () => void;
  stop: () => void;
  abort: () => void;
  onresult: ((e: any) => void) | null;
  onerror: ((e: any) => void) | null;
  onend: (() => void) | null;
}

type SpeechRecognitionCtor = new () => SpeechRecognitionType;

function getCtor(): SpeechRecognitionCtor | null {
  const w = window as unknown as {
    SpeechRecognition?: SpeechRecognitionCtor;
    webkitSpeechRecognition?: SpeechRecognitionCtor;
  };
  return (w.SpeechRecognition || w.webkitSpeechRecognition || null) ?? null;
}

export interface UseSpeechRecognition {
  supported: boolean;
  listening: boolean;
  transcript: string;
  err: string;
  start: () => void;
  stop: () => void;
  reset: () => void;
}

export function useSpeechRecognition(lang = "zh-CN"): UseSpeechRecognition {
  const ctorRef = useRef<SpeechRecognitionCtor | null>(null);
  const recRef = useRef<SpeechRecognitionType | null>(null);
  const [supported] = useState(() => getCtor() !== null);
  const [listening, setListening] = useState(false);
  const [transcript, setTranscript] = useState("");
  const [err, setErr] = useState<string>("");

  useEffect(() => {
    ctorRef.current = getCtor();
    return () => {
      try {
        recRef.current?.abort();
      } catch {
        /* ignore */
      }
      recRef.current = null;
    };
  }, []);

  const start = useCallback(() => {
    setErr("");
    const Ctor = ctorRef.current ?? getCtor();
    if (!Ctor) {
      setErr("当前浏览器不支持 Web 语音识别");
      return;
    }
    try {
      recRef.current?.abort();
    } catch {
      /* ignore */
    }
    const rec = new Ctor();
    rec.lang = lang;
    rec.continuous = false;
    rec.interimResults = true;
    let finalText = "";
    rec.onresult = (e: any) => {
      let interim = "";
      for (let i = e.resultIndex; i < e.results.length; i++) {
        const r = e.results[i];
        if (r.isFinal) finalText += r[0].transcript;
        else interim += r[0].transcript;
      }
      setTranscript((finalText + interim).trim());
    };
    rec.onerror = (e: any) => {
      const code = e?.error ?? "unknown";
      const msg =
        code === "not-allowed"
          ? "麦克风权限被拒绝"
          : code === "no-speech"
            ? "未识别到语音"
            : code === "aborted"
              ? ""
              : `识别失败：${code}`;
      if (msg) setErr(msg);
    };
    rec.onend = () => {
      setListening(false);
    };
    try {
      rec.start();
      setListening(true);
    } catch (e) {
      setErr(`启动失败：${String(e)}`);
      setListening(false);
    }
    recRef.current = rec;
  }, [lang]);

  const stop = useCallback(() => {
    try {
      recRef.current?.stop();
    } catch {
      /* ignore */
    }
    setListening(false);
  }, []);

  const reset = useCallback(() => {
    setTranscript("");
    setErr("");
  }, []);

  return { supported, listening, transcript, err, start, stop, reset };
}
