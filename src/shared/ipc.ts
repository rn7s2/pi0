// IPC contract shared by main, preload, and renderer.
import type { PermissionStatus, QueryRange, Settings, TextRecord } from './schemas';

/** IPC channel names (namespaced to avoid collisions). */
export const IPC = {
  getSettings: 'pi0:getSettings',
  saveSettings: 'pi0:saveSettings',
  startCapture: 'pi0:startCapture',
  stopCapture: 'pi0:stopCapture',
  isRunning: 'pi0:isRunning',
  queryText: 'pi0:queryText',
  permissionsStatus: 'pi0:permissionsStatus',
  captureNow: 'pi0:captureNow',
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
  queryText(range: QueryRange): Promise<TextRecord[]>;
  permissionsStatus(): Promise<PermissionStatus>;
  captureNow(): Promise<string[]>;
}
