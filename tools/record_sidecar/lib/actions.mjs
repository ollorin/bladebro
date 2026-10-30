// HARNESS-ONLY scripted actions for the measurement runs (--script). Emulation overrides and
// window-bounds changes are page-visible: they change innerWidth, matchMedia and the window
// state a detector can read. They must NEVER appear in the product recorder (src/); they exist
// only so the sidecar can be driven through a viewport change or a minimize on purpose.
//
// ctx = { browser, session(): current screencast sessionId, target(): current targetId }

export async function runScriptAction(ctx, step) {
  const { browser } = ctx;
  const sid = ctx.session();
  switch (step.action) {
    case 'emulate':
      await browser.send('Emulation.setDeviceMetricsOverride', {
        width: step.arg.w, height: step.arg.h, deviceScaleFactor: 0, mobile: false,
      }, sid);
      return;
    case 'emulate-clear':
      await browser.send('Emulation.clearDeviceMetricsOverride', {}, sid);
      return;
    case 'window': {
      const windowId = await windowIdFor(ctx);
      // Restore first: a window that is minimized or maximized ignores explicit sizes.
      await browser.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'normal' } });
      await browser.send('Browser.setWindowBounds', { windowId, bounds: { width: step.arg.w, height: step.arg.h } });
      return;
    }
    case 'minimize': {
      const windowId = await windowIdFor(ctx);
      await browser.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'minimized' } });
      return;
    }
    case 'restore': {
      const windowId = await windowIdFor(ctx);
      await browser.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'normal' } });
      return;
    }
    default:
      throw new Error(`runScriptAction: unknown action ${step.action}`);
  }
}

async function windowIdFor(ctx) {
  // Browser-level call: no sessionId. Resolves the window hosting the page being recorded.
  const r = await ctx.browser.send('Browser.getWindowForTarget', { targetId: ctx.target() });
  return r.windowId;
}
