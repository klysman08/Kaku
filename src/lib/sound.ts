// Tiny synthesized UI sounds (ported from TextBetter) — no audio files shipped.
type SoundName = "click" | "success" | "error";

let ctx: AudioContext | null = null;

function tone(freq: number, start: number, duration: number, type: OscillatorType, gain = 0.05) {
  if (!ctx) return;
  const osc = ctx.createOscillator();
  const g = ctx.createGain();
  osc.type = type;
  osc.frequency.setValueAtTime(freq, ctx.currentTime + start);
  g.gain.setValueAtTime(0, ctx.currentTime + start);
  g.gain.linearRampToValueAtTime(gain, ctx.currentTime + start + 0.01);
  g.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + start + duration);
  osc.connect(g).connect(ctx.destination);
  osc.start(ctx.currentTime + start);
  osc.stop(ctx.currentTime + start + duration + 0.02);
}

export function playSound(name: SoundName, enabled: boolean) {
  if (!enabled) return;
  try {
    ctx ??= new AudioContext();
    if (name === "click") tone(880, 0, 0.05, "sine", 0.03);
    if (name === "success") {
      tone(660, 0, 0.12, "sine");
      tone(990, 0.08, 0.16, "sine");
    }
    if (name === "error") {
      tone(220, 0, 0.18, "triangle");
      tone(180, 0.12, 0.2, "triangle");
    }
  } catch {
    // Audio is best-effort.
  }
}
