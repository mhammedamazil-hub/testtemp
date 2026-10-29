/** Service worker registration + update notification. */

export interface PwaUpdate {
  available: boolean;
  apply: () => void;
}

export async function registerServiceWorker(onUpdate: (update: PwaUpdate) => void): Promise<ServiceWorkerRegistration | null> {
  if (typeof navigator === 'undefined' || !('serviceWorker' in navigator)) return null;
  if (!import.meta.env.PROD) return null;
  try {
    const registration = await navigator.serviceWorker.register(`${import.meta.env.BASE_URL}sw.js`, {
      scope: import.meta.env.BASE_URL,
    });
    const notify = (worker: ServiceWorker) => {
      onUpdate({
        available: true,
        apply: () => {
          worker.postMessage({ type: 'SKIP_WAITING' });
          worker.addEventListener('statechange', () => {
            if (worker.state === 'activated') window.location.reload();
          });
        },
      });
    };
    if (registration.waiting) notify(registration.waiting);
    registration.addEventListener('updatefound', () => {
      const installing = registration.installing;
      if (!installing) return;
      installing.addEventListener('statechange', () => {
        if (installing.state === 'installed' && navigator.serviceWorker.controller) notify(installing);
      });
    });
    return registration;
  } catch {
    return null;
  }
}

export async function unregisterServiceWorkers(): Promise<void> {
  if (typeof navigator === 'undefined' || !('serviceWorker' in navigator)) return;
  const registrations = await navigator.serviceWorker.getRegistrations();
  await Promise.all(registrations.map((r) => r.unregister()));
}
