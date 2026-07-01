import { useCallback, useEffect, useState } from 'react';
import { IconDesktop, IconPoweroff, IconRight } from '@arco-design/web-react/icon';

/**
 * Tray float panel. Three rows, per the milestone:
 *  1. toggle the main window's visibility
 *  2. toggle recording on/off (the whole row is the hit target; the switch on
 *     the right is a visual indicator, not a separate control)
 *  3. quit the whole application
 */
export function FloatPanel() {
    const [running, setRunning] = useState(false);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        void window.pi0.isRunning().then(setRunning);
        // Stay in sync when recording is toggled from the main window.
        return window.pi0.onRunningChanged(setRunning);
    }, []);

    const toggleRecording = useCallback(async () => {
        setBusy(true);
        setError(null);
        try {
            if (running) {
                await window.pi0.stopCapture();
            } else {
                const res = await window.pi0.startCapture();
                if (!res.running) {
                    setError(res.error ? 'Permission needed — open pi0' : 'Could not start');
                }
            }
        } finally {
            setBusy(false);
        }
    }, [running]);

    return (
        <div className="fp">
            <button className="fp-row" onClick={() => void window.pi0.toggleMainWindow()}>
                <span className="fp-icon">
                    <IconDesktop />
                </span>
                <span className="fp-label">Open pi0 window</span>
                <IconRight className="fp-chevron" />
            </button>

            <button
                className="fp-row"
                aria-pressed={running}
                disabled={busy}
                onClick={() => void toggleRecording()}
            >
                <span className="fp-icon">
                    <span className={`fp-dot ${running ? 'on' : ''}`} />
                </span>
                <span className="fp-label">
                    Recording
                    {error && <span className="fp-sub error">{error}</span>}
                </span>
                <span
                    className={`fp-switch ${running ? 'on' : ''} ${busy ? 'busy' : ''}`}
                    aria-hidden="true"
                >
                    <span className="fp-knob" />
                </span>
            </button>

            <div className="fp-divider" />

            <button className="fp-row danger" onClick={() => void window.pi0.quitApp()}>
                <span className="fp-icon">
                    <IconPoweroff />
                </span>
                <span className="fp-label">Quit pi0</span>
            </button>
        </div>
    );
}
