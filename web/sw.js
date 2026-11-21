// CraftOffice launcher: the service worker only makes the page installable. It caches nothing,
// so every app always loads the version on the server; each app may bring its own worker.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));
self.addEventListener("fetch", () => {});
