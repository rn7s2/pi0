import { useEffect, useMemo, useState } from 'react';
import {
    Alert,
    Button,
    Input,
    InputNumber,
    Message,
    Space,
    Spin,
    Typography,
} from '@arco-design/web-react';
import { IconCopy, IconLock, IconSave, IconUndo } from '@arco-design/web-react/icon';

import type { McpInfo } from '../shared/ipc';
import type { Settings } from '../shared/schemas';

/** The subset of settings the Revert/Save footer governs. */
interface FormState {
    intervalSec: number;
    mcpPort: number;
}

const INTERVAL_MIN = 1;
const INTERVAL_MAX = 3600;
const PORT_MIN = 1024;
const PORT_MAX = 65535;

const inRange = (v: number | undefined, min: number, max: number): v is number =>
    v !== undefined && Number.isInteger(v) && v >= min && v <= max;

/** Paste-ready MCP client config (the "install instruction" agents consume). */
const agentConfig = (mcp: McpInfo): string =>
    JSON.stringify(
        {
            mcpServers: {
                pi0: {
                    url: mcp.url,
                    headers: { Authorization: `Bearer ${mcp.token}` },
                },
            },
        },
        null,
        2,
    );

export function SettingsView() {
    const [settings, setSettings] = useState<Settings | null>(null);
    const [saved, setSaved] = useState<FormState | null>(null);
    const [intervalSec, setIntervalSec] = useState<number | undefined>(undefined);
    const [mcpPort, setMcpPort] = useState<number | undefined>(undefined);
    const [saving, setSaving] = useState(false);
    const [mcp, setMcp] = useState<McpInfo | null>(null);

    // Password-reset section (independent of the Revert/Save footer).
    const [currentPw, setCurrentPw] = useState('');
    const [newPw, setNewPw] = useState('');
    const [confirmPw, setConfirmPw] = useState('');
    const [changingPw, setChangingPw] = useState(false);
    const [pwError, setPwError] = useState<string | null>(null);

    useEffect(() => {
        void window.pi0.getSettings().then((s) => {
            setSettings(s);
            const form = { intervalSec: Math.round(s.intervalMs / 1000), mcpPort: s.mcpPort };
            setSaved(form);
            setIntervalSec(form.intervalSec);
            setMcpPort(form.mcpPort);
        });
        void window.pi0.getMcpInfo().then(setMcp);
    }, []);

    const dirty = useMemo(
        () => !!saved && (intervalSec !== saved.intervalSec || mcpPort !== saved.mcpPort),
        [saved, intervalSec, mcpPort],
    );
    const valid =
        inRange(intervalSec, INTERVAL_MIN, INTERVAL_MAX) && inRange(mcpPort, PORT_MIN, PORT_MAX);

    const revert = () => {
        if (!saved) return;
        setIntervalSec(saved.intervalSec);
        setMcpPort(saved.mcpPort);
    };

    const save = async () => {
        if (!dirty || !valid) return;
        setSaving(true);
        try {
            const next = await window.pi0.saveSettings({
                intervalMs: (intervalSec as number) * 1000,
                mcpPort: mcpPort as number,
            });
            const form = { intervalSec: Math.round(next.intervalMs / 1000), mcpPort: next.mcpPort };
            setSettings(next);
            setSaved(form);
            setIntervalSec(form.intervalSec);
            setMcpPort(form.mcpPort);
            // The port may have changed, restarting the server on a new URL.
            void window.pi0.getMcpInfo().then(setMcp);
            Message.success('Settings saved');
        } finally {
            setSaving(false);
        }
    };

    const copy = async (label: string, text: string) => {
        try {
            await navigator.clipboard.writeText(text);
            Message.success(`${label} copied`);
        } catch {
            Message.error('Copy failed — select and copy manually');
        }
    };

    const changePassword = async () => {
        setPwError(null);
        if (!currentPw || !newPw) {
            setPwError('Enter your current and new password.');
            return;
        }
        if (newPw !== confirmPw) {
            setPwError('The new passwords do not match.');
            return;
        }
        setChangingPw(true);
        try {
            const res = await window.pi0.changePassword(currentPw, newPw);
            if (res.ok) {
                Message.success('Password changed');
                setCurrentPw('');
                setNewPw('');
                setConfirmPw('');
            } else {
                setPwError(res.error ?? 'Could not change the password.');
            }
        } finally {
            setChangingPw(false);
        }
    };

    if (!settings || !saved) {
        return <Spin loading style={{ display: 'block', marginTop: 40 }} />;
    }

    return (
        <div className="settings">
            <div className="settings-scroll">
                {/* ---- Capture settings ---- */}
                <section className="settings-section">
                    <Typography.Title heading={5} style={{ marginTop: 0 }}>
                        Capture settings
                    </Typography.Title>
                    <div className="field">
                        <div className="field-label">Screenshot interval</div>
                        <InputNumber
                            min={INTERVAL_MIN}
                            max={INTERVAL_MAX}
                            step={1}
                            suffix="seconds"
                            style={{ width: 220 }}
                            value={intervalSec}
                            onChange={(v) => setIntervalSec(v ?? undefined)}
                        />
                        <div className="field-hint">
                            Screenshots are OCR&apos;d into text on-device and the image is deleted
                            right after — only the recognised text is kept.
                        </div>
                    </div>
                </section>

                {/* ---- MCP server ---- */}
                <section className="settings-section">
                    <Typography.Title heading={5}>MCP server</Typography.Title>
                    <div className="field">
                        <div className="field-label">Port</div>
                        <InputNumber
                            min={PORT_MIN}
                            max={PORT_MAX}
                            step={1}
                            style={{ width: 220 }}
                            value={mcpPort}
                            onChange={(v) => setMcpPort(v ?? undefined)}
                        />
                        <div className="field-hint">
                            Agents connect via Streamable HTTP at{' '}
                            {mcp?.url ?? `http://127.0.0.1:${saved.mcpPort}/mcp`}
                        </div>
                    </div>

                    {mcp && !mcp.running && (
                        <Alert
                            type="warning"
                            style={{ marginBottom: 12 }}
                            content="The MCP server isn’t running — the port may be in use. Pick another port and save."
                        />
                    )}

                    <div className="field">
                        <div className="field-label">Access</div>
                        <Space>
                            <Button
                                icon={<IconCopy />}
                                disabled={!mcp?.token}
                                onClick={() => void copy('Token', mcp?.token ?? '')}
                            >
                                Copy Token
                            </Button>
                            <Button
                                icon={<IconCopy />}
                                disabled={!mcp?.token}
                                onClick={() =>
                                    void copy('Agent config', mcp ? agentConfig(mcp) : '')
                                }
                            >
                                Copy for Agents
                            </Button>
                        </Space>
                        <div className="field-hint">
                            The token authenticates every connection — treat it like a password.
                            &quot;Copy for Agents&quot; copies a ready-to-paste MCP install snippet.
                        </div>
                    </div>
                </section>

                {/* ---- Password reset ---- */}
                <section className="settings-section">
                    <Typography.Title heading={5}>Password reset</Typography.Title>
                    <Typography.Paragraph type="secondary" style={{ marginTop: 0 }}>
                        Change the password that encrypts your recorded data. Enter your current
                        password to confirm it&apos;s you.
                    </Typography.Paragraph>
                    <Space direction="vertical" size="small" style={{ width: 320 }}>
                        <Input.Password
                            placeholder="Current password"
                            value={currentPw}
                            onChange={setCurrentPw}
                        />
                        <Input.Password
                            placeholder="New password"
                            value={newPw}
                            onChange={setNewPw}
                        />
                        <Input.Password
                            placeholder="Confirm new password"
                            value={confirmPw}
                            onChange={setConfirmPw}
                        />
                        {pwError && <Alert type="error" content={pwError} />}
                        <Button
                            icon={<IconLock />}
                            loading={changingPw}
                            disabled={!currentPw || !newPw || !confirmPw}
                            onClick={() => void changePassword()}
                        >
                            Change password
                        </Button>
                    </Space>
                </section>
            </div>

            {/* ---- fixed action row ---- */}
            <div className="settings-footer">
                <Button icon={<IconUndo />} disabled={!dirty} onClick={revert}>
                    Revert
                </Button>
                <Button
                    type="primary"
                    icon={<IconSave />}
                    loading={saving}
                    disabled={!dirty || !valid}
                    onClick={() => void save()}
                >
                    Save
                </Button>
            </div>
        </div>
    );
}
