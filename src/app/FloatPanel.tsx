import { useCallback, useEffect, useRef, useState } from 'react';
import { IconPoweroff, IconRight, IconSettings } from '@arco-design/web-react/icon';

/**
 * Tray float panel. Three rows, per the milestone:
 *  1. toggle the main (settings) window's visibility
 *  2. toggle recording on/off (the whole row is the hit target; the switch on
 *     the right is a visual indicator, not a separate control)
 *  3. quit the whole application
 */
export function FloatPanel() {
    const [running, setRunning] = useState(false);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const cardRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        void window.pi0.isRunning().then(setRunning);
        // Stay in sync when recording is toggled from the main window.
        return window.pi0.onRunningChanged(setRunning);
    }, []);

    // Size the window to the card's real height so the panel hugs its content
    // (no blank space below the last row, whatever the row count/heights are).
    useEffect(() => {
        const card = cardRef.current;
        if (!card) return;
        const report = () => window.pi0.resizePanel(card.offsetHeight);
        report();
        const observer = new ResizeObserver(report);
        observer.observe(card);
        return () => observer.disconnect();
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
        <div className="fp" ref={cardRef}>
            <button className="fp-row" onClick={() => void window.pi0.toggleMainWindow()}>
                <span className="fp-icon">
                    <IconSettings />
                </span>
                <span className="fp-label">Settings</span>
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
