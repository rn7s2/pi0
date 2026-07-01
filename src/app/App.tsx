import { useEffect, useState } from 'react';

import { DataViewer } from './DataViewer';
import { PermissionGuard } from './PermissionGuard';
import { SettingsView } from './SettingsView';

type Tab = 'data' | 'settings';
// 'checking' → probing perms; 'guard' → blocking modal; 'ready' → app usable.
type Phase = 'checking' | 'guard' | 'ready';

export function App() {
    const [running, setRunning] = useState(false);
    const [busy, setBusy] = useState(false);
    const [tab, setTab] = useState<Tab>('data');
    const [phase, setPhase] = useState<Phase>('checking');

    // Boot gate: skip the modal entirely when both grants are already in place.
    useEffect(() => {
        void window.pi0.permissionsStatus().then((p) => {
            setPhase(p.inputMonitoring && p.screenRecording ? 'ready' : 'guard');
        });
    }, []);

    useEffect(() => {
        void window.pi0.isRunning().then(setRunning);
        // Stay in sync when recording is toggled from the tray float panel.
        return window.pi0.onRunningChanged(setRunning);
    }, []);

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
        } finally {
            setBusy(false);
        }
    };

    if (phase === 'checking') {
        return <div className="app boot" />;
    }
    if (phase === 'guard') {
        return <PermissionGuard onGranted={() => setPhase('ready')} />;
    }

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
                <button
                    className={tab === 'data' ? 'tab active' : 'tab'}
                    onClick={() => setTab('data')}
                >
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
