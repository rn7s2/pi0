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

/** Bounds shared by all three interval settings (1s – 1h), in milliseconds. */
const INTERVAL_MIN_MS = 1_000;
const INTERVAL_MAX_MS = 3_600_000;
const intervalMs = (fallback: number) =>
    z.number().int().min(INTERVAL_MIN_MS).max(INTERVAL_MAX_MS).default(fallback);

/** User settings, persisted to `<userData>/settings.json`. */
export const SettingsSchema = z.object({
    /** Absolute directory where recorded data is written. */
    dataDir: z.string().min(1),
    /** Appearance: follow the system, or pin light/dark. */
    theme: ThemeSchema.default('system'),
    /**
     * Screenshot cadence while the user is *active* (input within the idle
     * timeout window). Screenshots are mandatory — they feed the on-device OCR
     * that produces the context store — so there is no master switch, only the
     * adaptive cadence: the tighter interval used when the user is at the machine.
     */
    activeIntervalMs: intervalMs(8_000),
    /**
     * Screenshot cadence while the user is *idle* (no keystroke or mouse
     * movement within the idle timeout). Coarser than the active interval to cut
     * power draw, CPU, and database growth when nothing is changing on screen.
     */
    idleIntervalMs: intervalMs(48_000),
    /**
     * How long after the last input (keystroke or mouse movement) the user is
     * still considered active. Past this window with no input, capture drops to
     * the idle interval.
     */
    idleTimeoutMs: intervalMs(180_000),
    /** Localhost port the MCP server (Streamable HTTP) listens on. */
    mcpPort: z.number().int().min(1024).max(65535).default(DEFAULT_MCP_PORT),
});
export type Settings = z.infer<typeof SettingsSchema>;

/** One keystroke record (mirrors the Rust `TextRecord`). */
export const TextRecordSchema = z.object({
    /** Epoch milliseconds — the UTC instant. */
    ts: z.number(),
    /** Local wall-clock at `ts`, ISO-8601 without offset. */
    localTime: z.string(),
    /** IANA timezone name the record was captured in. */
    tzName: z.string(),
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

/**
 * One entry in the merged activity timeline (mirrors the Rust `TimelineRecord`).
 * `kind` discriminates the payload: `"ocr"` entries carry `display` + `items`
 * (what the user saw), `"keys"` entries carry `text` (what the user typed). The
 * inapplicable fields are absent/null, so they're validated as nullish here and
 * narrowed by the MCP server before being handed to agents.
 */
export const TimelineRecordSchema = z.object({
    /** Epoch milliseconds — screenshot instant (ocr) or keystroke buffer start (keys). */
    ts: z.number(),
    /** Local wall-clock at `ts`, ISO-8601 without offset. */
    localTime: z.string(),
    /** IANA timezone name the record was captured in. */
    tzName: z.string(),
    app: z.string(),
    appRaw: z.string(),
    kind: z.enum(['ocr', 'keys']),
    /** OCR only: display index (0 = main). */
    display: z.number().nullish(),
    /** OCR only: recognised text lines with normalised coordinates. */
    items: z.array(OcrItemSchema).nullish(),
    /** Keystrokes only: the raw captured text for this buffer. */
    text: z.string().nullish(),
});
export type TimelineRecord = z.infer<typeof TimelineRecordSchema>;

/** One page of timeline records plus the range's total match count. */
export const TimelinePageSchema = z.object({
    total: z.number(),
    records: z.array(TimelineRecordSchema),
});
export type TimelinePage = z.infer<typeof TimelinePageSchema>;

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
