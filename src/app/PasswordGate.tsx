import { useEffect, useState } from 'react';
import { Alert, Button, Input, Modal, Space, Typography } from '@arco-design/web-react';
import { IconLock } from '@arco-design/web-react/icon';

/**
 * Boot-time password gate. pi0's data lives in an encrypted (SQLCipher) store;
 * this prompt unlocks it — or, on first run, creates it with a password the user
 * chooses. There is no recovery: forgetting the password means the recorded data
 * is unreadable, which the first-run copy makes explicit.
 */
export function PasswordGate({ onUnlocked }: { onUnlocked: () => void }) {
    // null while we're still checking whether a store already exists.
    const [exists, setExists] = useState<boolean | null>(null);
    const [password, setPassword] = useState('');
    const [confirm, setConfirm] = useState('');
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        void window.pi0.dbStatus().then((s) => setExists(s.exists));
    }, []);

    const firstRun = exists === false;

    const submit = async () => {
        setError(null);
        if (!password) {
            setError('Enter a password.');
            return;
        }
        if (firstRun && password !== confirm) {
            setError('The two passwords do not match.');
            return;
        }
        setBusy(true);
        try {
            const res = await window.pi0.unlockDb(password);
            if (res.ok) {
                onUnlocked();
            } else {
                setError(res.error ?? 'Could not open the store.');
            }
        } finally {
            setBusy(false);
        }
    };

    return (
        <Modal
            visible
            title={firstRun ? 'Set your pi0 password' : 'Unlock pi0'}
            closable={false}
            maskClosable={false}
            escToExit={false}
            style={{ width: 460 }}
            footer={
                <div className="guard-footer">
                    <Button size="small" status="danger" onClick={() => void window.pi0.quitApp()}>
                        Quit
                    </Button>
                    <span className="guard-footer-spacer" />
                    <Button
                        type="primary"
                        icon={<IconLock />}
                        loading={busy}
                        disabled={exists === null}
                        onClick={() => void submit()}
                    >
                        {firstRun ? 'Create & unlock' : 'Unlock'}
                    </Button>
                </div>
            }
        >
            <Typography.Paragraph type="secondary" style={{ marginTop: 0 }}>
                {firstRun
                    ? 'pi0 stores everything it records in an encrypted database on this Mac. Choose a password to protect it.'
                    : 'Enter your password to open pi0’s encrypted database.'}
            </Typography.Paragraph>

            <Space direction="vertical" size="medium" style={{ width: '100%' }}>
                <Input.Password
                    autoFocus
                    size="large"
                    placeholder="Password"
                    value={password}
                    onChange={setPassword}
                    onPressEnter={() => !firstRun && void submit()}
                />
                {firstRun && (
                    <Input.Password
                        size="large"
                        placeholder="Confirm password"
                        value={confirm}
                        onChange={setConfirm}
                        onPressEnter={() => void submit()}
                    />
                )}

                {firstRun && (
                    <Alert
                        type="warning"
                        content="This password cannot be recovered. If you forget it, your recorded data can never be read again."
                    />
                )}

                {error && <Alert type="error" content={error} />}
            </Space>
        </Modal>
    );
}
