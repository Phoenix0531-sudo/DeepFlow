/** 轻量 WebAudio 提示音（不依赖外部资源）。 */

type SoundKind = "chime" | "severe" | "inject" | string;

let ctx: AudioContext | null = null;

function audio(): AudioContext | null {
  try {
    const AC =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext?: typeof AudioContext })
        .webkitAudioContext;
    if (!AC) return null;
    if (!ctx) ctx = new AC();
    if (ctx.state === "suspended") void ctx.resume();
    return ctx;
  } catch {
    return null;
  }
}

function tone(
  freq: number,
  start: number,
  dur: number,
  type: OscillatorType,
  gain = 0.08,
) {
  const ac = audio();
  if (!ac) return;
  const osc = ac.createOscillator();
  const g = ac.createGain();
  osc.type = type;
  osc.frequency.value = freq;
  g.gain.setValueAtTime(0.0001, start);
  g.gain.exponentialRampToValueAtTime(gain, start + 0.02);
  g.gain.exponentialRampToValueAtTime(0.0001, start + dur);
  osc.connect(g);
  g.connect(ac.destination);
  osc.start(start);
  osc.stop(start + dur + 0.02);
}

export function playSound(kind: SoundKind, muted: boolean = false) {
  if (muted) return;
  const ac = audio();
  if (!ac) return;
  const t0 = ac.currentTime + 0.01;
  switch (kind) {
    case "chime":
      tone(880, t0, 0.12, "sine", 0.09);
      tone(1320, t0 + 0.12, 0.18, "sine", 0.07);
      break;
    case "severe":
      tone(220, t0, 0.2, "square", 0.07);
      tone(180, t0 + 0.22, 0.25, "square", 0.08);
      tone(140, t0 + 0.48, 0.35, "sawtooth", 0.06);
      break;
    case "inject":
      tone(660, t0, 0.08, "triangle", 0.06);
      tone(990, t0 + 0.09, 0.08, "triangle", 0.05);
      break;
    default:
      tone(520, t0, 0.1, "sine", 0.05);
  }
}
