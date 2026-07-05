// Settings persistence: `<userData>/settings.json`, validated with zod on every
// read and write. `dataDir` is owned by the main process (defaults under
// userData) and injected so the renderer can't point recording at an arbitrary
// path in M1.
import { promises as fs } from 'node:fs';
import path from 'node:path';

import { app } from 'electron';

import { Settings, SettingsSchema } from '../shared/schemas';

function settingsPath(): string {
    return path.join(app.getPath('userData'), 'settings.json');
}

export function defaultDataDir(): string {
    return path.join(app.getPath('userData'), 'pi0-data');
}

/** Load settings, falling back to (and filling) defaults on missing/invalid file. */
export async function loadSettings(): Promise<Settings> {
    const dataDir = defaultDataDir();
    try {
        const raw = await fs.readFile(settingsPath(), 'utf8');
        // Saved values win over the default dataDir; zod fills any missing fields.
        const parsed = SettingsSchema.safeParse(
            migrateLegacyInterval({ dataDir, ...JSON.parse(raw) }),
        );
        if (parsed.success) {
            return parsed.data;
        }
        console.error('[pi0] invalid settings.json, using defaults:', parsed.error.message);
    } catch {
        // Missing file → defaults.
    }
    return SettingsSchema.parse({ dataDir });
}

/**
 * Carry a pre-M5 single `intervalMs` onto the new `activeIntervalMs` when the
 * split active/idle fields aren't present yet, so upgrading users keep the
 * cadence they chose instead of silently resetting to the default.
 */
function migrateLegacyInterval(raw: Record<string, unknown>): Record<string, unknown> {
    if ('intervalMs' in raw && !('activeIntervalMs' in raw)) {
        return { ...raw, activeIntervalMs: raw.intervalMs };
    }
    return raw;
}

/** Validate and persist settings; returns the normalized value. */
export async function saveSettings(input: unknown): Promise<Settings> {
    const settings = SettingsSchema.parse(input);
    await fs.mkdir(app.getPath('userData'), { recursive: true });
    await fs.writeFile(settingsPath(), JSON.stringify(settings, null, 2), 'utf8');
    return settings;
}
