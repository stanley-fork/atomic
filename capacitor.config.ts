import type { CapacitorConfig } from '@capacitor/cli';

// When CAP_DEV_URL is set, point the webview at a running Vite dev server
// instead of the bundled assets — enables HMR in the simulator. Leave unset
// for production builds so the app loads from mobile/ios/App/App/public.
const devUrl = process.env.CAP_DEV_URL;

const config: CapacitorConfig = {
  appId: 'com.atomic.mobile',
  appName: 'Atomic',
  webDir: 'dist-web',
  ios: {
    path: 'mobile/ios',
    contentInset: 'never',
    backgroundColor: '#1e1e1e',
  },
  ...(devUrl
    ? { server: { url: devUrl, cleartext: true } }
    : {}),
};

export default config;
