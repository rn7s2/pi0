// Preload: expose a minimal, typed IPC surface on `window.pi0`. The renderer
// never gets Node or the native addon directly. Shared by the main window and
// the tray float panel.
import { contextBridge, ipcRenderer } from 'electron';

import { IPC, Pi0Api } from './shared/ipc';

const api: Pi0Api = {
    getSettings: () => ipcRenderer.invoke(IPC.getSettings),
    saveSettings: (settings) => ipcRenderer.invoke(IPC.saveSettings, settings),
    startCapture: () => ipcRenderer.invoke(IPC.startCapture),
    stopCapture: () => ipcRenderer.invoke(IPC.stopCapture),
    isRunning: () => ipcRenderer.invoke(IPC.isRunning),
    permissionsStatus: () => ipcRenderer.invoke(IPC.permissionsStatus),
    requestPermission: (kind) => ipcRenderer.invoke(IPC.requestPermission, kind),
    openPermissionSettings: (kind) => ipcRenderer.invoke(IPC.openPermissionSettings, kind),
    toggleMainWindow: () => ipcRenderer.invoke(IPC.toggleMainWindow),
    quitApp: () => ipcRenderer.invoke(IPC.quitApp),
    relaunchApp: () => ipcRenderer.invoke(IPC.relaunchApp),
    onRunningChanged: (cb) => {
        const listener = (_event: unknown, running: boolean) => cb(running);
        ipcRenderer.on(IPC.runningChanged, listener);
        return () => ipcRenderer.removeListener(IPC.runningChanged, listener);
    },
};

contextBridge.exposeInMainWorld('pi0', api);
