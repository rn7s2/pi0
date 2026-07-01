import { useEffect, useState } from 'react';
import { Badge, Button, Layout, Message, Spin, Tabs, Typography } from '@arco-design/web-react';
import { IconPause, IconPlayArrow } from '@arco-design/web-react/icon';

import { DataViewer } from './DataViewer';
import { PermissionGuard } from './PermissionGuard';
import { SettingsView } from './SettingsView';

// 'checking' → probing perms; 'guard' → blocking modal; 'ready' → app usable.
type Phase = 'checking' | 'guard' | 'ready';

const { Header, Content } = Layout;

export function App() {
    const [running, setRunning] = useState(false);
    const [busy, setBusy] = useState(false);
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
                    Message.error({
                        content: `Could not start capture: ${res.error}`,
                        duration: 5000,
                    });
                }
            }
        } finally {
            setBusy(false);
        }
    };

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
        <Layout className="app">
            <Header className="topbar">
                <div className="brand">
                    <Typography.Text className="brand-name">pi0</Typography.Text>
                    <Typography.Text type="secondary" className="tagline">
                        personal intelligence workbench
                    </Typography.Text>
                </div>
                <div className="controls">
                    <Badge
                        status={running ? 'processing' : 'default'}
                        text={running ? 'Recording' : 'Idle'}
                    />
                    <Button
                        type="primary"
                        status={running ? 'danger' : 'success'}
                        loading={busy}
                        icon={running ? <IconPause /> : <IconPlayArrow />}
                        onClick={toggle}
                    >
                        {running ? 'Stop capture' : 'Start capture'}
                    </Button>
                </div>
            </Header>

            <Content className="content">
                <Tabs defaultActiveTab="data" className="main-tabs">
                    <Tabs.TabPane key="data" title="Data">
                        <DataViewer />
                    </Tabs.TabPane>
                    <Tabs.TabPane key="settings" title="Settings">
                        <SettingsView />
                    </Tabs.TabPane>
                </Tabs>
            </Content>
        </Layout>
    );
}
