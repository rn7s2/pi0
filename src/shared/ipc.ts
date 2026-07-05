// IPC contract shared by main, preload, and renderer.
import type { PermissionKind, PermissionStatus, Settings, Theme } from './schemas';

/** IPC channel names (namespaced to avoid collisions). */
export const IPC = {
    getSettings: 'pi0:getSettings',
    saveSettings: 'pi0:saveSettings',
    startCapture: 'pi0:startCapture',
    stopCapture: 'pi0:stopCapture',
    isRunning: 'pi0:isRunning',
    /** Renderer → main: is the encrypted store present / unlocked? */
    dbStatus: 'pi0:dbStatus',
    /** Renderer → main: open (or create) the store with a password. */
    unlockDb: 'pi0:unlockDb',
    /** Renderer → main: change the store password (verifies current first). */
    changePassword: 'pi0:changePassword',
    /** Renderer → main: fetch the MCP token + endpoint for the copy buttons. */
    getMcpInfo: 'pi0:getMcpInfo',
    permissionsStatus: 'pi0:permissionsStatus',
    requestPermission: 'pi0:requestPermission',
    openPermissionSettings: 'pi0:openPermissionSettings',
    /** Renderer → main: open an http(s) URL in the user's default browser. */
    openExternal: 'pi0:openExternal',
    /** Renderer → main: set the appearance (system/light/dark) and persist it. */
    setTheme: 'pi0:setTheme',
    /** Main → renderer broadcast: the appearance choice changed. */
    themeChanged: 'pi0:themeChanged',
    toggleMainWindow: 'pi0:toggleMainWindow',
    quitApp: 'pi0:quitApp',
    relaunchApp: 'pi0:relaunchApp',
    /** Renderer → main: the tray panel's measured content height changed. */
    panelResize: 'pi0:panelResize',
    /** Main → renderer broadcast: capture running state changed. */
    runningChanged: 'pi0:runningChanged',
} as const;

/** Result of a start-capture attempt (error carries the TCC/lock hint, if any). */
export type StartResult = { running: boolean; error?: string };

/** Whether the encrypted store exists on disk and whether it's unlocked. */
export type DbStatus = { exists: boolean; unlocked: boolean };

/** Result of an unlock/create attempt. `created` is true on first-run creation. */
export type UnlockResult = { ok: boolean; created?: boolean; error?: string };

/** Result of a password change (error is a human-readable reason on failure). */
export type ChangePasswordResult = { ok: boolean; error?: string };

/**
 * MCP connection info surfaced to Settings. `token` is empty and `running` is
 * false when the server couldn't start (e.g. the port was busy).
 */
export type McpInfo = { token: string; url: string; running: boolean };

/** The typed API the preload exposes on `window.pi0`. */
export interface Pi0Api {
    getSettings(): Promise<Settings>;
    saveSettings(settings: Partial<Settings>): Promise<Settings>;
    startCapture(): Promise<StartResult>;
    stopCapture(): Promise<{ running: boolean }>;
    isRunning(): Promise<boolean>;
    /** Whether the encrypted store exists on disk and whether it's unlocked. */
    dbStatus(): Promise<DbStatus>;
    /** Open (or create on first run) the encrypted store with `password`. */
    unlockDb(password: string): Promise<UnlockResult>;
    /** Change the store password (verifies `current` first). */
    changePassword(current: string, next: string): Promise<ChangePasswordResult>;
    /** Fetch the MCP token + endpoint (for the Copy Token / Copy for Agents buttons). */
    getMcpInfo(): Promise<McpInfo>;
    permissionsStatus(): Promise<PermissionStatus>;
    /** Trigger the macOS TCC prompt for a grant; resolves to the fresh status. */
    requestPermission(kind: PermissionKind): Promise<PermissionStatus>;
    /** Open the relevant System Settings > Privacy pane for a grant. */
    openPermissionSettings(kind: PermissionKind): Promise<void>;
    /** Open an http(s) URL in the user's default browser. */
    openExternal(url: string): Promise<void>;
    /** Set the appearance (system/light/dark); persists and applies immediately. */
    setTheme(theme: Theme): Promise<void>;
    /**
     * Subscribe to appearance changes broadcast by the main process (e.g. the
     * theme was changed from the other window). Returns an unsubscribe function.
     */
    onThemeChanged(cb: (theme: Theme) => void): () => void;
    /** Show the main window if hidden, hide it if visible. */
    toggleMainWindow(): Promise<void>;
    /** Quit the whole application (stops capture first). */
    quitApp(): Promise<void>;
    /** Relaunch the application (used after granting screen recording). */
    relaunchApp(): Promise<void>;
    /**
     * Ask the main process to resize the tray panel window to fit its content
     * (height in CSS px). Keeps the window flush with the menu, no blank space.
     */
    resizePanel(height: number): void;
    /**
     * Subscribe to capture running-state changes broadcast by the main process.
     * Returns an unsubscribe function.
     */
    onRunningChanged(cb: (running: boolean) => void): () => void;
}
