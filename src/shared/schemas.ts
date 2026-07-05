// Shared zod schemas — the single source of truth for data crossing the
// main ↔ preload ↔ renderer boundaries, and for validating what the Rust
// addon returns. Imported by both the main and renderer bundles.
import { z } from 'zod';

/** Default localhost port the MCP server listens on. */
export const DEFAULT_MCP_PORT = 31415;

/**
 * Appearance choice. `system` follows the OS light/dark setting; `light`/`dark`
 * pin it. Applied by the main process via Electron's `nativeTheme.themeSource`,
 * which every window mirrors through `prefers-color-scheme`.
 */
export const ThemeSchema = z.enum(['system', 'light', 'dark']);
export type Theme = z.infer<typeof ThemeSchema>;

/** User settings, persisted to `<userData>/settings.json`. */
export const SettingsSchema = z.object({
    /** Absolute directory where recorded data is written. */
    dataDir: z.string().min(1),
    /** Appearance: follow the system, or pin light/dark. */
    theme: ThemeSchema.default('system'),
    /**
     * Screenshot interval in milliseconds (1s – 1h). Screenshots are mandatory
     * in M3 — they feed the on-device OCR that produces the context store — so
     * there is no master switch anymore, only the cadence.
     */
    intervalMs: z.number().int().min(1000).max(3_600_000).default(8_000),
    /** Localhost port the MCP server (Streamable HTTP) listens on. */
    mcpPort: z.number().int().min(1024).max(65535).default(DEFAULT_MCP_PORT),
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

/**
 * One OCR'd text line from a screenshot. Coordinates are normalised to the
 * [0, 1] range relative to the captured display — (x, y) is the top-left of
 * the line's bounding box, (w, h) its size — so agents can reason about where
 * on screen a text sat (title bar vs sidebar vs content) at any resolution.
 */
export const OcrItemSchema = z.object({
    text: z.string(),
    /** Recognition confidence in [0, 1]. */
    score: z.number(),
    x: z.number(),
    y: z.number(),
    w: z.number(),
    h: z.number(),
});
export type OcrItem = z.infer<typeof OcrItemSchema>;

/** One screenshot's OCR context (mirrors the Rust `ContextRecord`). */
export const ContextRecordSchema = z.object({
    /** Epoch milliseconds the screenshot was taken. */
    ts: z.number(),
    /** Sanitized app name (folder-safe). */
    app: z.string(),
    /** Original localizedName of the frontmost app. */
    appRaw: z.string(),
    /** Display index the shot came from (0 = main display). */
    display: z.number(),
    items: z.array(OcrItemSchema),
});
export type ContextRecord = z.infer<typeof ContextRecordSchema>;

export const ContextRecordArraySchema = z.array(ContextRecordSchema);

/** Per-app usage aggregate for a time range (mirrors the Rust `AppUsage`). */
export const AppUsageSchema = z.object({
    app: z.string(),
    appRaw: z.string(),
    /** Epoch ms of the first/last record seen in the range. */
    firstTs: z.number(),
    lastTs: z.number(),
    /** Number of keystroke records in the range. */
    textRecords: z.number(),
    /** Number of OCR'd screenshot contexts in the range. */
    contextRecords: z.number(),
});
export type AppUsage = z.infer<typeof AppUsageSchema>;

export const AppUsageArraySchema = z.array(AppUsageSchema);

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

/** Which TCC grant a request/open-settings call targets. */
export const PermissionKindSchema = z.enum(['inputMonitoring', 'screenRecording']);
export type PermissionKind = z.infer<typeof PermissionKindSchema>;

/** Capture on/off state reported to the renderer. */
export type CaptureState = { running: boolean };
