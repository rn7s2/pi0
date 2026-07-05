// Reflect the effective OS colour scheme into Arco's `arco-theme` attribute.
//
// The main process owns the actual choice via Electron's `nativeTheme.themeSource`
// (system / light / dark). That flips `prefers-color-scheme` in every renderer at
// once, so each window only has to mirror the media query — and they all update
// together, including when the choice is changed from the other window.
export function syncArcoTheme(): void {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = (): void => {
        document.body.setAttribute('arco-theme', mq.matches ? 'dark' : 'light');
    };
    apply();
    mq.addEventListener('change', apply);
}
