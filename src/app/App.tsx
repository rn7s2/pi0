import { useEffect, useState } from 'react';
import { Spin } from '@arco-design/web-react';

import { PermissionGuard } from './PermissionGuard';
import { SettingsView } from './SettingsView';

// 'checking' → probing perms; 'guard' → blocking modal; 'ready' → app usable.
type Phase = 'checking' | 'guard' | 'ready';

// The main window is purely a settings window (M3): the permission guard, then
// the settings form. Recording is controlled from the tray float panel.
export function App() {
    const [phase, setPhase] = useState<Phase>('checking');

    // Boot gate: skip the modal entirely when both grants are already in place.
    useEffect(() => {
        void window.pi0.permissionsStatus().then((p) => {
            setPhase(p.inputMonitoring && p.screenRecording ? 'ready' : 'guard');
        });
    }, []);

    if (phase === 'checking') {
        return (
            <div className="app boot">
                <Spin size={32} tip="Starting pi0…" />
            </div>
        );
    }
    if (phase === 'guard') {
        return <PermissionGuard onGranted={() => setPhase('ready')} />;
    }

    return (
        <div className="app content">
            <SettingsView />
        </div>
    );
}
