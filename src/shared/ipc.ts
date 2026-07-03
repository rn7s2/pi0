// IPC contract shared by main, preload, and renderer.
import type { PermissionKind, PermissionStatus, Settings } from './schemas';

/** IPC channel names (namespaced to avoid collisions). */
export const IPC = {
    getSettings: 'pi0:getSettings',
    saveSettings: 'pi0:saveSettings',
    startCapture: 'pi0:startCapture',
    stopCapture: 'pi0:stopCapture',
    isRunning: 'pi0:isRunning',
    permissionsStatus: 'pi0:permissionsStatus',
    requestPermission: 'pi0:requestPermission',
    openPermissionSettings: 'pi0:openPermissionSettings',
    toggleMainWindow: 'pi0:toggleMainWindow',
    quitApp: 'pi0:quitApp',
    relaunchApp: 'pi0:relaunchApp',
    /** Main → renderer broadcast: capture running state changed. */
    runningChanged: 'pi0:runningChanged',
} as const;

/** Result of a start-capture attempt (error carries the TCC hint, if any). */
export type StartResult = { running: boolean; error?: string };

/** The typed API the preload exposes on `window.pi0`. */
export interface Pi0Api {
    getSettings(): Promise<Settings>;
    saveSettings(settings: Partial<Settings>): Promise<Settings>;
    startCapture(): Promise<StartResult>;
    stopCapture(): Promise<{ running: boolean }>;
    isRunning(): Promise<boolean>;
    permissionsStatus(): Promise<PermissionStatus>;
    /** Trigger the macOS TCC prompt for a grant; resolves to the fresh status. */
    requestPermission(kind: PermissionKind): Promise<PermissionStatus>;
    /** Open the relevant System Settings > Privacy pane for a grant. */
    openPermissionSettings(kind: PermissionKind): Promise<void>;
    /** Show the main window if hidden, hide it if visible. */
    toggleMainWindow(): Promise<void>;
    /** Quit the whole application (stops capture first). */
    quitApp(): Promise<void>;
    /** Relaunch the application (used after granting screen recording). */
    relaunchApp(): Promise<void>;
    /**
     * Subscribe to capture running-state changes broadcast by the main process.
     * Returns an unsubscribe function.
     */
    onRunningChanged(cb: (running: boolean) => void): () => void;
}
