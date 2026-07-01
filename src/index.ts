import { app, BrowserWindow, ipcMain } from 'electron';
import * as native from '@pi0/native';

import { defaultDataDir, loadSettings, saveSettings } from './main/settings';
import { IPC, StartResult } from './shared/ipc';
import {
  PermissionStatus,
  QueryRangeSchema,
  Settings,
  TextRecord,
  TextRecordArraySchema,
} from './shared/schemas';

// Magic constants injected by Forge's Webpack plugin.
declare const MAIN_WINDOW_WEBPACK_ENTRY: string;
declare const MAIN_WINDOW_PRELOAD_WEBPACK_ENTRY: string;

// Handle creating/removing shortcuts on Windows when installing/uninstalling.
if (require('electron-squirrel-startup')) {
  app.quit();
}

// Single-instance lock — essential for a recorder: a second instance would
// double-register the IOHIDManager, double-capture screenshots, and race on the
// append-only data files. If we're not the primary instance, exit immediately.
if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  bootstrap();
}

function bootstrap(): void {
  let mainWindow: BrowserWindow | null = null;
  let settings: Settings | null = null;
  let snapshotTimer: ReturnType<typeof setInterval> | null = null;

  // ---- capture control ----------------------------------------------------

  const captureNow = async (): Promise<string[]> => {
    try {
      return (await native.captureSnapshot()) as string[];
    } catch (err) {
      console.error('[pi0] snapshot failed:', (err as Error).message);
      return [];
    }
  };

  const clearTimer = (): void => {
    if (snapshotTimer) {
      clearInterval(snapshotTimer);
      snapshotTimer = null;
    }
  };

  const restartTimer = (): void => {
    clearTimer();
    if (settings && native.isRunning()) {
      snapshotTimer = setInterval(() => void captureNow(), settings.intervalMs);
    }
  };

  const startCapture = (): StartResult => {
    if (!settings) {
      return { running: false, error: 'settings not loaded' };
    }
    try {
      native.start(
        {
          dataDir: settings.dataDir,
          intervalMs: settings.intervalMs,
          hotkey: settings.hotkey,
          captureOnHotkey: settings.captureOnHotkey,
        },
        // Hotkey fires this on the JS main thread (via the addon's TSFN).
        () => void captureNow(),
      );
      restartTimer();
      return { running: true };
    } catch (err) {
      return { running: false, error: (err as Error).message };
    }
  };

  const stopCapture = (): { running: boolean } => {
    clearTimer();
    try {
      native.stop();
    } catch (err) {
      console.error('[pi0] stop failed:', (err as Error).message);
    }
    return { running: false };
  };

  // ---- IPC ----------------------------------------------------------------

  const registerIpc = (): void => {
    ipcMain.handle(IPC.getSettings, () => settings);

    ipcMain.handle(IPC.saveSettings, async (_event, raw) => {
      // Renderer is untrusted; keep dataDir under our control and validate.
      const next = await saveSettings({
        ...(raw as Record<string, unknown>),
        dataDir: settings?.dataDir ?? defaultDataDir(),
      });
      settings = next;
      if (native.isRunning()) {
        native.updateSettings(next.intervalMs, next.hotkey, next.captureOnHotkey);
        restartTimer();
      }
      return next;
    });

    ipcMain.handle(IPC.startCapture, () => startCapture());
    ipcMain.handle(IPC.stopCapture, () => stopCapture());
    ipcMain.handle(IPC.isRunning, () => native.isRunning());
    ipcMain.handle(IPC.captureNow, () => captureNow());

    ipcMain.handle(IPC.permissionsStatus, (): PermissionStatus => {
      const p = native.permissionsStatus();
      return { inputMonitoring: p.inputMonitoring, screenRecording: p.screenRecording };
    });

    ipcMain.handle(IPC.queryText, async (_event, rawRange): Promise<TextRecord[]> => {
      const range = QueryRangeSchema.parse(rawRange);
      const records = await native.queryText({
        dataDir: settings?.dataDir ?? defaultDataDir(),
        startMs: range.startMs,
        endMs: range.endMs,
      });
      // Validate what the addon returned (catches on-disk drift).
      return TextRecordArraySchema.parse(records);
    });
  };

  // ---- window / lifecycle -------------------------------------------------

  const createWindow = (): void => {
    mainWindow = new BrowserWindow({
      width: 1000,
      height: 720,
      webPreferences: {
        preload: MAIN_WINDOW_PRELOAD_WEBPACK_ENTRY,
      },
    });
    mainWindow.loadURL(MAIN_WINDOW_WEBPACK_ENTRY);
    mainWindow.on('closed', () => {
      mainWindow = null;
    });
  };

  app.on('second-instance', () => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.focus();
    }
  });

  app.on('ready', async () => {
    settings = await loadSettings();
    registerIpc();
    createWindow();
  });

  // Recorder without a window has no controls, so quit (and stop) on close.
  app.on('window-all-closed', () => {
    stopCapture();
    app.quit();
  });

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });

  app.on('before-quit', () => {
    stopCapture();
  });
}
