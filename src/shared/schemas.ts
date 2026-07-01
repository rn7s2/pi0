// Shared zod schemas — the single source of truth for data crossing the
// main ↔ preload ↔ renderer boundaries, and for validating what the Rust
// addon returns. Imported by both the main and renderer bundles.
import { z } from 'zod';

/** Default hotkey combo (Ctrl+Shift+S) as keymap tokens the addon understands. */
export const DEFAULT_HOTKEY = ['LC', 'LS', 'S'];

/** User settings, persisted to `<userData>/settings.json`. */
export const SettingsSchema = z.object({
  /** Absolute directory where recorded data is written. */
  dataDir: z.string().min(1),
  /** Screenshot interval in milliseconds (1s – 1h). */
  intervalMs: z.number().int().min(1000).max(3_600_000).default(60_000),
  /** Screenshot hotkey as keymap tokens, e.g. ["LC","LS","S"]. */
  hotkey: z.array(z.string()).min(1).default(DEFAULT_HOTKEY),
  /** Whether the hotkey triggers a screenshot. */
  captureOnHotkey: z.boolean().default(true),
});
export type Settings = z.infer<typeof SettingsSchema>;

/** One keystroke record (mirrors the Rust `TextRecord`). */
export const TextRecordSchema = z.object({
  ts: z.number(),
  app: z.string(),
  appRaw: z.string(),
  text: z.string(),
});
export type TextRecord = z.infer<typeof TextRecordSchema>;

export const TextRecordArraySchema = z.array(TextRecordSchema);

/** A time-range query from the renderer (dataDir is injected by main). */
export const QueryRangeSchema = z
  .object({
    startMs: z.number(),
    endMs: z.number(),
  })
  .refine((v) => v.startMs <= v.endMs, {
    message: 'startMs must be <= endMs',
  });
export type QueryRange = z.infer<typeof QueryRangeSchema>;

/** macOS TCC permission status (mirrors the Rust `PermissionStatus`). */
export const PermissionStatusSchema = z.object({
  inputMonitoring: z.boolean(),
  screenRecording: z.boolean(),
});
export type PermissionStatus = z.infer<typeof PermissionStatusSchema>;

/** Capture on/off state reported to the renderer. */
export type CaptureState = { running: boolean };
