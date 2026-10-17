globalThis.appdata = {};
globalThis.navigator = { userAgent: 'HydraGofileUserAgent', language: 'HydraGofileLanguage' };
globalThis.crypto = { subtle: { digest: function () { return Promise.resolve(new ArrayBuffer(32)); } } };
globalThis.window = { crypto: crypto, navigator: navigator, location: { hostname: 'gofile.io', search: '' } };
globalThis.console = { log: function () {}, warn: function () {}, error: function () {} };
globalThis.URLSearchParams = class {
    get() { return null; }
    has() { return false; }
    toString() { return ''; }
};
