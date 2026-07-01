// Preload: expose a minimal, typed IPC surface on `window.pi0`. The renderer
// never gets Node or the native addon directly.
import { contextBridge, ipcRenderer } from 'electron';

import { IPC, Pi0Api } from './shared/ipc';

const api: Pi0Api = {
  getSettings: () => ipcRenderer.invoke(IPC.getSettings),
  saveSettings: (settings) => ipcRenderer.invoke(IPC.saveSettings, settings),
  startCapture: () => ipcRenderer.invoke(IPC.startCapture),
  stopCapture: () => ipcRenderer.invoke(IPC.stopCapture),
  isRunning: () => ipcRenderer.invoke(IPC.isRunning),
  queryText: (range) => ipcRenderer.invoke(IPC.queryText, range),
  permissionsStatus: () => ipcRenderer.invoke(IPC.permissionsStatus),
  captureNow: () => ipcRenderer.invoke(IPC.captureNow),
};

contextBridge.exposeInMainWorld('pi0', api);
