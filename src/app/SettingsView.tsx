import { useEffect, useState } from 'react';

import type { Settings } from '../shared/schemas';
import { PermissionsPanel } from './PermissionsPanel';

export function SettingsView() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [intervalSec, setIntervalSec] = useState(60);
  const [captureOnHotkey, setCaptureOnHotkey] = useState(true);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    void window.pi0.getSettings().then((s) => {
      setSettings(s);
      setIntervalSec(Math.round(s.intervalMs / 1000));
      setCaptureOnHotkey(s.captureOnHotkey);
    });
  }, []);

  const save = async () => {
    if (!settings) return;
    const next = await window.pi0.saveSettings({
      intervalMs: Math.min(3_600_000, Math.max(1000, Math.round(intervalSec) * 1000)),
      hotkey: settings.hotkey,
      captureOnHotkey,
    });
    setSettings(next);
    setSaved(true);
    window.setTimeout(() => setSaved(false), 1500);
  };

  if (!settings) {
    return <p className="muted">Loading…</p>;
  }

  return (
    <div className="panel">
      <h2>Capture settings</h2>

      <label className="field">
        <span>Screenshot interval (seconds)</span>
        <input
          type="number"
          min={1}
          max={3600}
          value={intervalSec}
          onChange={(e) => setIntervalSec(Number(e.target.value))}
        />
      </label>

      <label className="field checkbox">
        <input
          type="checkbox"
          checked={captureOnHotkey}
          onChange={(e) => setCaptureOnHotkey(e.target.checked)}
        />
        <span>Capture on hotkey ({settings.hotkey.join(' + ')})</span>
      </label>

      <label className="field">
        <span>Data folder</span>
        <input type="text" readOnly value={settings.dataDir} />
      </label>

      <button className="btn" onClick={save}>
        Save{saved ? ' ✓' : ''}
      </button>

      <PermissionsPanel />
    </div>
  );
}
