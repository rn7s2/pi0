import { useCallback, useEffect, useState } from 'react';

import { DataViewer } from './DataViewer';
import { SettingsView } from './SettingsView';

type Tab = 'data' | 'settings';

export function App() {
  const [running, setRunning] = useState(false);
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<Tab>('data');

  const refreshRunning = useCallback(async () => {
    setRunning(await window.pi0.isRunning());
  }, []);

  useEffect(() => {
    void refreshRunning();
  }, [refreshRunning]);

  const toggle = async () => {
    setBusy(true);
    try {
      if (running) {
        await window.pi0.stopCapture();
      } else {
        const res = await window.pi0.startCapture();
        if (!res.running && res.error) {
          window.alert(`Could not start capture:\n\n${res.error}`);
        }
      }
      await refreshRunning();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          pi0 <span className="tagline">personal intelligence workbench</span>
        </div>
        <div className="controls">
          <span className={`status ${running ? 'on' : 'off'}`}>
            {running ? '● recording' : '○ idle'}
          </span>
          <button
            onClick={toggle}
            disabled={busy}
            className={`btn ${running ? 'stop' : 'start'}`}
          >
            {running ? 'Stop' : 'Start'} capture
          </button>
        </div>
      </header>

      <nav className="tabs">
        <button className={tab === 'data' ? 'tab active' : 'tab'} onClick={() => setTab('data')}>
          Data
        </button>
        <button
          className={tab === 'settings' ? 'tab active' : 'tab'}
          onClick={() => setTab('settings')}
        >
          Settings
        </button>
      </nav>

      <main className="content">{tab === 'data' ? <DataViewer /> : <SettingsView />}</main>
    </div>
  );
}
