import { useCallback, useEffect, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import {
    IconDesktop,
    IconMoon,
    IconPoweroff,
    IconRight,
    IconSettings,
    IconSun,
} from '@arco-design/web-react/icon';

import type { Theme } from '../shared/schemas';

/** Appearance options, in slider order (left → right). */
const THEME_OPTIONS: { value: Theme; label: string; Icon: typeof IconDesktop }[] = [
    { value: 'system', label: 'System', Icon: IconDesktop },
    { value: 'light', label: 'Light', Icon: IconSun },
    { value: 'dark', label: 'Dark', Icon: IconMoon },
];

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
    const [theme, setTheme] = useState<Theme>('system');
    const cardRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        void window.pi0.isRunning().then(setRunning);
        void window.pi0.getSettings().then((s) => setTheme(s.theme));
        // Stay in sync when recording / theme is changed from the main window.
        const offRunning = window.pi0.onRunningChanged(setRunning);
        const offTheme = window.pi0.onThemeChanged(setTheme);
        return () => {
            offRunning();
            offTheme();
        };
    }, []);

    const changeTheme = useCallback((next: Theme) => {
        setTheme(next); // optimistic; main echoes it back via onThemeChanged
        void window.pi0.setTheme(next);
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
                    setError(res.error ? 'Open pi0 first' : 'Could not start');
                }
            }
        } finally {
            setBusy(false);
        }
    }, [running]);

    const themeIndex = THEME_OPTIONS.findIndex((t) => t.value === theme);
    const ThemeIcon = THEME_OPTIONS[themeIndex]?.Icon ?? IconDesktop;

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

            <div className="fp-row fp-static">
                <span className="fp-icon">
                    <ThemeIcon />
                </span>
                <span className="fp-label">Theme</span>
                <div
                    className="fp-seg"
                    role="radiogroup"
                    aria-label="Theme"
                    style={{ '--fp-seg-index': themeIndex } as CSSProperties}
                >
                    <span className="fp-seg-thumb" aria-hidden="true" />
                    {THEME_OPTIONS.map(({ value, label, Icon }) => (
                        <button
                            key={value}
                            className={`fp-seg-btn ${theme === value ? 'on' : ''}`}
                            role="radio"
                            aria-checked={theme === value}
                            aria-label={label}
                            title={label}
                            onClick={() => changeTheme(value)}
                        >
                            <Icon />
                        </button>
                    ))}
                </div>
            </div>

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
