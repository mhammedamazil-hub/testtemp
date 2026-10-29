import type { CapacitorConfig } from '@capacitor/cli';

/**
 * Android wrapper configuration. The web assets are copied from `dist/`, the
 * app runs in a WebView with a local origin (https://localhost) so IndexedDB,
 * service workers and fetch behave like the PWA. Network access is required
 * only for pages/repos the user imports and for AI providers they configure.
 */
const config: CapacitorConfig = {
  appId: 'dev.sourceforge.app',
  appName: 'SourceForge',
  webDir: 'dist',
  android: {
    allowMixedContent: false,
    captureInput: true,
    webContentsDebuggingEnabled: false,
  },
  server: {
    androidScheme: 'https',
    hostname: 'localhost',
  },
};

export default config;
