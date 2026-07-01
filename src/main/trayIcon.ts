// Menu-bar tray glyph for pi0 — a 32x32 (@2x → 16pt) macOS *template* PNG
// (black + alpha). macOS recolors template images to match the light/dark menu
// bar automatically. Embedded as base64 so the main bundle needs no asset copy.
// Regenerate with scratchpad/gen-icon.js if the glyph changes.
import { nativeImage, type NativeImage } from 'electron';

const TRAY_ICON_PNG_BASE64 =
    'iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAABBUlEQVR42tVXXRHDIAyuBKQgAQlIQEIl4AAJlYKESEHCxu7ysOUgZLQdGXffC+TnS4AQtu0Ph6lwFZHA4dptTkMFVDwGAJS9jMxeUQSOKQrqnoo6TzimyDPZsJ10v6JKFZ4YNTiXOtkCtCmOHBqOozASg7KlQUKUiXyG/SCLWXLgplh/kc2dEy4k7faCW2QbdptBBcI0XlhHIrEdWkIgYYnDk7OScU6aXWgJvDNMjLGDufMHo5eI7EeAjix6JvJR4ZHqOm6PjPCK9ipfbxu6Z4wS6A1p+ZXo6ybw8y1YfgiXX8PlhUhFKV7+GC1/jlU0JCpasuVNqYq2XMXHRM3XTMXn9LbxBEJ+ZjifcOniAAAAAElFTkSuQmCC';

/** Build the tray `NativeImage` at 16pt logical size, marked as a template. */
export function trayIcon(): NativeImage {
    const img = nativeImage.createFromBuffer(Buffer.from(TRAY_ICON_PNG_BASE64, 'base64'), {
        // The buffer is 32px, so it renders at 16pt logical (crisp on retina).
        scaleFactor: 2,
    });
    img.setTemplateImage(true);
    return img;
}
