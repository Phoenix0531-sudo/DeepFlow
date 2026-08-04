/** 占位：定稿不做内置语音识别，仅保留文件以符合目录 Spec。 */
export function useSpeechRecognition() {
  return { supported: false as const, start: () => {}, stop: () => {}, transcript: "" };
}
