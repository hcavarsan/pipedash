import { createServer } from "node:net";
import type { BrowserProvider } from "@e2e-dev/web";
import { type Browser, chromium } from "playwright";

// The published web engine has no channel option. Its supported provider
// contract lets us use installed Google Chrome without a Chromium fallback.
export function googleChrome(): BrowserProvider {
  const browsers = new Map<string, Browser>();
  return {
    name: "installed-google-chrome",
    async acquire(request) {
      const port = await new Promise<number>((resolve, reject) => {
        const server = createServer();
        server.once("error", reject);
        server.listen(0, "127.0.0.1", () => {
          const address = server.address();
          if (address == null || typeof address === "string") {
            server.close(() => reject(new Error("Cannot allocate Chrome CDP port")));
            return;
          }
          server.close(() => resolve(address.port));
        });
      });
      const browser = await chromium.launch({
        channel: "chrome",
        headless: request.env.SCREENSHOTS_E2E_HEADED !== "1",
        args: [`--remote-debugging-port=${port}`],
      });
      const id = `${request.runId}-${request.targetName}-${request.slot}`;
      try {
        request.signal.throwIfAborted();
        browsers.set(id, browser);
        request.log(`Google Chrome ${browser.version()} (channel: chrome)`);
        return { id, cdpEndpoint: `http://127.0.0.1:${port}` };
      } catch (error) {
        await browser.close();
        throw error;
      }
    },
    async release(lease) {
      const browser = browsers.get(lease.id);
      browsers.delete(lease.id);
      await browser?.close();
    },
  };
}
