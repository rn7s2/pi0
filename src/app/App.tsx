import { useCallback, useEffect, useState } from 'react';
import { Spin } from '@arco-design/web-react';

import { PasswordGate } from './PasswordGate';
import { PermissionGuard } from './PermissionGuard';
import { SettingsView } from './SettingsView';

// 'checking' → probing store/perms; 'password' → unlock/create the encrypted
// store; 'guard' → macOS permission modal; 'ready' → settings usable.
type Phase = 'checking' | 'password' | 'guard' | 'ready';

// The main window gates on the encrypted store first (the password prompt), then
// the macOS permission guard, then the settings form. Recording can be toggled
// from the tray float panel or from Settings → Capture settings.
export function App() {
    const [phase, setPhase] = useState<Phase>('checking');

    // Once the store is unlocked, the only remaining gate is macOS permissions.
    const afterUnlock = useCallback(async () => {
        const p = await window.pi0.permissionsStatus();
        setPhase(p.inputMonitoring && p.screenRecording ? 'ready' : 'guard');
    }, []);

    useEffect(() => {
        void (async () => {
            const status = await window.pi0.dbStatus();
            if (status.unlocked) {
                await afterUnlock();
            } else {
                setPhase('password');
            }
        })();
    }, [afterUnlock]);

    if (phase === 'checking') {
        return (
            <div className="app boot">
                <Spin size={32} tip="Starting pi0…" />
            </div>
        );
    }
    if (phase === 'password') {
        return <PasswordGate onUnlocked={() => void afterUnlock()} />;
    }
    if (phase === 'guard') {
        return <PermissionGuard onGranted={() => setPhase('ready')} />;
    }

    return (
        <div className="app">
            <SettingsView />
        </div>
    );
}
