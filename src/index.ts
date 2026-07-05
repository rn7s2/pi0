import { app, BrowserWindow, ipcMain, Menu, nativeTheme, screen, shell, Tray } from 'electron';
import * as native from '@pi0/native';

import { McpHandle, startMcpServer } from './main/mcp/server';
import { defaultDataDir, loadSettings, saveSettings } from './main/settings';
import { trayIcon } from './main/trayIcon';
import {
    ChangePasswordResult,
    DbStatus,
    IPC,
    McpInfo,
    StartResult,
    UnlockResult,
} from './shared/ipc';
import {
    DEFAULT_MCP_PORT,
    PermissionKind,
    PermissionKindSchema,
    PermissionStatus,
    Settings,
    ThemeSchema,
} from './shared/schemas';

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

/** Turn a native store error into a short, capitalised message for the UI. */
function dbErrorMessage(err: unknown): string {
    const raw = (err as Error).message || 'Could not open the store';
    const msg = raw.replace(/^Error:\s*/i, '').trim();
    return msg.charAt(0).toUpperCase() + msg.slice(1);
}

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
    // The MCP bearer token, read from the encrypted store once it's unlocked.
    let mcpToken: string | null = null;
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
        // Capture writes into the encrypted store — refuse until it's unlocked.
        if (!native.isDbOpen()) {
            return { running: false, error: 'locked' };
        }
        try {
            native.start({
                dataDir: settings.dataDir,
                intervalMs: settings.intervalMs,
            });
            restartTimer();
            broadcastRunning();
            return { running: true };
        } catch (err) {
            return { running: false, error: (err as Error).message };
        }
    };

    // ---- MCP server ----------------------------------------------------------

    // The MCP server needs the store unlocked (its bearer token lives there), so
    // it only comes up after a successful unlock. A failed bind (e.g. port in
    // use) must not take the recorder down: log it, keep running, and let the
    // user pick another port in settings.
    const startMcp = async (): Promise<void> => {
        if (!settings || !native.isDbOpen()) return;
        try {
            mcpToken = native.mcpToken();
            mcp = await startMcpServer(settings.mcpPort, {
                getToken: () => mcpToken ?? '',
            });
            console.log(`[pi0] MCP server at http://127.0.0.1:${mcp.port}/mcp (bearer auth)`);
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

    // Dock presence mirrors the main window's visibility: shown while the window
    // is up, hidden when it's closed to the tray (pi0 then lives as an accessory).
    const syncDock = (): void => {
        const visible = !!(mainWindow && !mainWindow.isDestroyed() && mainWindow.isVisible());
        if (visible) {
            void app.dock?.show();
        } else {
            app.dock?.hide();
        }
    };

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
        // Keep the dock icon in lock-step with visibility.
        mainWindow.on('show', syncDock);
        mainWindow.on('hide', syncDock);
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
            syncDock();
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

        // ---- encrypted store (password gate) --------------------------------

        ipcMain.handle(IPC.dbStatus, (): DbStatus => {
            const dataDir = settings?.dataDir ?? defaultDataDir();
            return { exists: native.dbExists(dataDir), unlocked: native.isDbOpen() };
        });

        // Open (or create on first run) the store with the user's password. On
        // success we bring the MCP server up (it needs the token inside the store).
        ipcMain.handle(IPC.unlockDb, async (_event, rawPassword): Promise<UnlockResult> => {
            if (typeof rawPassword !== 'string' || rawPassword.length === 0) {
                return { ok: false, error: 'Password required' };
            }
            const dataDir = settings?.dataDir ?? defaultDataDir();
            try {
                const created = native.openDb(dataDir, rawPassword);
                if (!mcp) await startMcp();
                return { ok: true, created };
            } catch (err) {
                return { ok: false, error: dbErrorMessage(err) };
            }
        });

        ipcMain.handle(IPC.changePassword, (_event, current, next): ChangePasswordResult => {
            if (typeof current !== 'string' || typeof next !== 'string' || next.length === 0) {
                return { ok: false, error: 'Password required' };
            }
            try {
                native.changePassword(current, next);
                return { ok: true };
            } catch (err) {
                return { ok: false, error: dbErrorMessage(err) };
            }
        });

        ipcMain.handle(IPC.getMcpInfo, (): McpInfo => {
            const port = settings?.mcpPort ?? DEFAULT_MCP_PORT;
            return {
                token: mcpToken ?? '',
                url: `http://127.0.0.1:${port}/mcp`,
                running: mcp !== null,
            };
        });

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

        ipcMain.handle(IPC.openExternal, async (_event, rawUrl) => {
            // Only ever hand http(s) URLs to the OS — never file:// or custom schemes.
            const url = new URL(String(rawUrl));
            if (url.protocol !== 'http:' && url.protocol !== 'https:') {
                throw new Error(`Refusing to open non-http(s) URL: ${url.protocol}`);
            }
            await shell.openExternal(url.toString());
        });

        ipcMain.handle(IPC.setTheme, async (_event, rawTheme) => {
            const theme = ThemeSchema.parse(rawTheme);
            // Setting the source flips prefers-color-scheme in every window at
            // once; renderers re-mirror it (no per-window message needed for the
            // visual). The broadcast below only keeps the other window's *control*
            // (radio / segmented slider) in sync with the chosen value.
            nativeTheme.themeSource = theme;
            settings = await saveSettings({
                ...(settings ?? { dataDir: defaultDataDir() }),
                theme,
            });
            for (const win of [mainWindow, panelWindow]) {
                if (win && !win.isDestroyed()) {
                    win.webContents.send(IPC.themeChanged, theme);
                }
            }
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

        // The panel renderer reports its measured content height so the window
        // hugs the menu exactly — no hardcoded height to drift out of sync.
        ipcMain.on(IPC.panelResize, (_event, rawHeight) => {
            if (!panelWindow || panelWindow.isDestroyed()) return;
            const height = Math.round(rawHeight);
            if (!Number.isFinite(height) || height <= 0) return;
            const [width] = panelWindow.getContentSize();
            panelWindow.setContentSize(width, height);
        });
    };

    // ---- lifecycle ----------------------------------------------------------

    app.on('second-instance', () => showMainWindow());

    app.on('ready', async () => {
        settings = await loadSettings();
        // Drive the OS colour scheme for every window; renderers just mirror
        // prefers-color-scheme (see src/app/theme.ts).
        nativeTheme.themeSource = settings.theme;
        // Start as an accessory (no dock icon); the main window's show/hide keeps
        // the dock in sync from here on. The MCP server waits for the store to be
        // unlocked (see the unlock IPC handler).
        app.dock?.hide();
        registerIpc();
        createPanelWindow();
        createTray();
        createMainWindow();
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
        // Checkpoint the WAL and release the encrypted store.
        native.closeDb();
    });
}
