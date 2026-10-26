export interface AppConfig {
  provider: {
    kind: string;
    endpoint: string;
    model: string;
    apiKey: string;
    appId: string;
    apiSecret: string;
    language: string;
  };
  correction: {
    enabled: boolean;
    endpoint: string;
    apiKey: string;
    model: string;
    prompt: string;
  };
  appearance: {
    anchor: 'top-left' | 'top' | 'top-right' | 'left' | 'center' | 'right' | 'bottom-left' | 'bottom' | 'bottom-right';
    effect: 'ribbon' | 'spectrum' | 'orbit' | 'bars';
    accent: string;
    opacity: number;
    intensity: number;
    scale: number;
    motion: 'full' | 'reduced';
    fps: number;
    combine: boolean;
    offsetX: number;
    offsetY: number;
  };
  injection: {
    mode: 'stable' | 'final' | 'preview';
    method: 'unicode' | 'clipboard';
    correctionAtEnd: boolean;
  };
  hotkey: string;
  cancelHotkey: string;
  micDevice: string;
  maxRecordingSecs: number;
  autostart: boolean;
  historyEnabled: boolean;
  hotwords: string[];
}