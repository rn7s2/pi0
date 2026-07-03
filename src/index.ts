import { app, BrowserWindow, ipcMain, Menu, screen, shell, Tray } from 'electron';
import * as native from '@pi0/native';

import { McpHandle, startMcpServer } from './main/mcp/server';
import { defaultDataDir, loadSettings, saveSettings } from './main/settings';
import { trayIcon } from './main/trayIcon';
import { IPC, StartResult } from './shared/ipc';
import { PermissionKind, PermissionKindSchema, PermissionStatus, Settings } from './shared/schemas';

// Magic constants injected by Forge's Webpack plugin (one pair per entry point).
declare const MAIN_WINDOW_WEBPACK_ENTRY: string;
declare const MAIN_WINDOW_PRELOAD_WEBPACK_ENTRY: string;
declare const PANEL_WINDOW_WEBPACK_ENTRY: string;
declare const PANEL_WINDOW_PRELOAD_WEBPACK_ENTRY: string;

// The System Settings > Privacy pane deep-links for each grant the guard needs.
const SETTINGS_URL: Record<PermissionKind, string> = {
    inputMonitoring: 'x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent',
    screenRecording:
        'x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture',
};

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
    let panelWindow: BrowserWindow | null = null;
    let tray: Tray | null = null;
    let settings: Settings | null = null;
    let mcp: McpHandle | null = null;
    let snapshotTimer: ReturnType<typeof setInterval> | null = null;
    // Distinguishes an explicit quit from a main-window close (which hides to tray).
    let isQuitting = false;
    // Debounce so a tray click that blurs (and hides) the panel doesn't reopen it.
    let panelHiddenAt = 0;

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

    // Screenshots are mandatory (they feed the OCR context store): the timer
    // always runs while capture is on.
    const restartTimer = (): void => {
        clearTimer();
        if (settings && native.isRunning()) {
            snapshotTimer = setInterval(() => void captureNow(), settings.intervalMs);
        }
    };

    // Push the current running state to every live renderer (main + panel) so the
    // topbar and the float-panel switch stay in sync no matter who toggled it.
    const broadcastRunning = (): void => {
        const running = native.isRunning();
        for (const win of [mainWindow, panelWindow]) {
            if (win && !win.isDestroyed()) {
                win.webContents.send(IPC.runningChanged, running);
            }
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
            broadcastRunning();
            return { running: true };
        } catch (err) {
            return { running: false, error: (err as Error).message };
        }
    };

    // ---- MCP server ----------------------------------------------------------

    // A failed bind (e.g. port in use) must not take the recorder down: log it,
    // keep running, and let the user pick another port in settings.
    const startMcp = async (): Promise<void> => {
        if (!settings) return;
        try {
            mcp = await startMcpServer(settings.mcpPort, {
                getDataDir: () => settings?.dataDir ?? defaultDataDir(),
            });
            console.log(`[pi0] MCP server at http://127.0.0.1:${mcp.port}/mcp`);
        } catch (err) {
            mcp = null;
            console.error('[pi0] MCP server failed to start:', (err as Error).message);
        }
    };

    const restartMcp = async (): Promise<void> => {
        await mcp?.close();
        mcp = null;
        await startMcp();
    };

    const stopCapture = (): { running: boolean } => {
        clearTimer();
        try {
            native.stop();
        } catch (err) {
            console.error('[pi0] stop failed:', (err as Error).message);
        }
        broadcastRunning();
        return { running: false };
    };

    // ---- windows ------------------------------------------------------------

    const createMainWindow = (): void => {
        // Purely a settings window (M3) — compact, form-sized.
        mainWindow = new BrowserWindow({
            width: 620,
            height: 760,
            show: false,
            webPreferences: {
                preload: MAIN_WINDOW_PRELOAD_WEBPACK_ENTRY,
            },
        });
        mainWindow.loadURL(MAIN_WINDOW_WEBPACK_ENTRY);
        mainWindow.once('ready-to-show', () => mainWindow?.show());
        // Closing the main window hides it to the tray; the app keeps recording and
        // is only really quit from the tray/panel (which flips isQuitting first).
        mainWindow.on('close', (event) => {
            if (!isQuitting) {
                event.preventDefault();
                mainWindow?.hide();
            }
        });
        mainWindow.on('closed', () => {
            mainWindow = null;
        });
    };

    const showMainWindow = (): void => {
        if (!mainWindow || mainWindow.isDestroyed()) {
            createMainWindow();
            return;
        }
        mainWindow.show();
        mainWindow.focus();
    };

    const toggleMainWindow = (): void => {
        if (mainWindow && !mainWindow.isDestroyed() && mainWindow.isVisible()) {
            mainWindow.hide();
        } else {
            showMainWindow();
        }
    };

    const hidePanel = (): void => {
        if (panelWindow && !panelWindow.isDestroyed()) {
            panelWindow.hide();
            panelHiddenAt = Date.now();
        }
    };

    const createPanelWindow = (): void => {
        panelWindow = new BrowserWindow({
            width: 256,
            height: 142,
            show: false,
            frame: false,
            resizable: false,
            movable: false,
            transparent: true,
            alwaysOnTop: true,
            skipTaskbar: true,
            fullscreenable: false,
            hasShadow: true,
            webPreferences: {
                preload: PANEL_WINDOW_PRELOAD_WEBPACK_ENTRY,
            },
        });
        panelWindow.loadURL(PANEL_WINDOW_WEBPACK_ENTRY);
        // Follow the user across Spaces / over fullscreen apps.
        panelWindow.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
        // Dismiss when focus leaves (click elsewhere / Esc handled in renderer).
        panelWindow.on('blur', hidePanel);
        panelWindow.on('closed', () => {
            panelWindow = null;
        });
    };

    // Position the panel just below the clicked tray icon, clamped to the display.
    const showPanelAt = (bounds: Electron.Rectangle): void => {
        if (!panelWindow || panelWindow.isDestroyed()) {
            createPanelWindow();
        }
        if (!panelWindow) return;
        const [pw] = panelWindow.getSize();
        const display = screen.getDisplayNearestPoint({ x: bounds.x, y: bounds.y });
        const work = display.workArea;
        const gap = 6;
        const x = Math.round(
            Math.max(
                work.x + gap,
                Math.min(bounds.x + bounds.width / 2 - pw / 2, work.x + work.width - pw - gap),
            ),
        );
        const y = Math.round(bounds.y + bounds.height + 2);
        panelWindow.setPosition(x, y, false);
        broadcastRunning();
        panelWindow.show();
        panelWindow.focus();
    };

    const togglePanel = (bounds: Electron.Rectangle): void => {
        if (panelWindow && !panelWindow.isDestroyed() && panelWindow.isVisible()) {
            hidePanel();
        } else if (Date.now() - panelHiddenAt < 200) {
            // The click that reached the tray just blurred (and hid) the panel; leave it closed.
        } else {
            showPanelAt(bounds);
        }
    };

    const createTray = (): void => {
        tray = new Tray(trayIcon());
        tray.setToolTip('pi0 — personal intelligence workbench');
        tray.on('click', (_event, bounds) => togglePanel(bounds));
        // Right-click safety net so pi0 is always quittable even if the panel fails.
        tray.on('right-click', () => {
            tray?.popUpContextMenu(
                Menu.buildFromTemplate([
                    { label: 'Show pi0', click: () => showMainWindow() },
                    { type: 'separator' },
                    {
                        label: 'Quit pi0',
                        click: () => {
                            isQuitting = true;
                            app.quit();
                        },
                    },
                ]),
            );
        });
    };

    // ---- IPC ----------------------------------------------------------------

    const registerIpc = (): void => {
        ipcMain.handle(IPC.getSettings, () => settings);

        ipcMain.handle(IPC.saveSettings, async (_event, raw) => {
            // Renderer is untrusted; keep dataDir under our control and validate.
            const previousPort = settings?.mcpPort;
            const next = await saveSettings({
                ...(raw as Record<string, unknown>),
                dataDir: settings?.dataDir ?? defaultDataDir(),
            });
            settings = next;
            if (native.isRunning()) {
                native.updateSettings(next.intervalMs, next.hotkey, next.captureOnHotkey);
                restartTimer();
            }
            if (next.mcpPort !== previousPort) {
                await restartMcp();
            }
            return next;
        });

        ipcMain.handle(IPC.startCapture, () => startCapture());
        ipcMain.handle(IPC.stopCapture, () => stopCapture());
        ipcMain.handle(IPC.isRunning, () => native.isRunning());

        ipcMain.handle(IPC.permissionsStatus, (): PermissionStatus => {
            const p = native.permissionsStatus();
            return { inputMonitoring: p.inputMonitoring, screenRecording: p.screenRecording };
        });

        ipcMain.handle(IPC.requestPermission, (_event, rawKind): PermissionStatus => {
            const kind = PermissionKindSchema.parse(rawKind);
            // Fire the TCC prompt (also registers pi0 in the relevant Settings list).
            if (kind === 'inputMonitoring') {
                native.requestInputMonitoring();
            } else {
                native.requestScreenRecording();
            }
            const p = native.permissionsStatus();
            return { inputMonitoring: p.inputMonitoring, screenRecording: p.screenRecording };
        });

        ipcMain.handle(IPC.openPermissionSettings, async (_event, rawKind) => {
            const kind = PermissionKindSchema.parse(rawKind);
            await shell.openExternal(SETTINGS_URL[kind]);
        });

        ipcMain.handle(IPC.toggleMainWindow, () => toggleMainWindow());

        ipcMain.handle(IPC.quitApp, () => {
            isQuitting = true;
            app.quit();
        });

        ipcMain.handle(IPC.relaunchApp, () => {
            isQuitting = true;
            app.relaunch();
            app.exit(0);
        });
    };

    // ---- lifecycle ----------------------------------------------------------

    app.on('second-instance', () => showMainWindow());

    app.on('ready', async () => {
        settings = await loadSettings();
        registerIpc();
        createPanelWindow();
        createTray();
        createMainWindow();
        await startMcp();
    });

    // Recorder lives in the tray: closing every window must NOT quit the app.
    app.on('window-all-closed', () => {
        /* keep running in the background; quit is explicit via tray/panel */
    });

    app.on('activate', () => showMainWindow());

    app.on('before-quit', () => {
        isQuitting = true;
        stopCapture();
        void mcp?.close();
        mcp = null;
    });
}
