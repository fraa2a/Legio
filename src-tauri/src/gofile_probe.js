var appdata = {};
var navigator = { userAgent: 'HydraGofileUserAgent', language: 'HydraGofileLanguage' };
var crypto = { subtle: { digest: function () { return Promise.resolve(new ArrayBuffer(32)); } } };
var window = { crypto: crypto, navigator: navigator, location: { hostname: 'gofile.io', search: '' } };
var console = { log: function () {}, warn: function () {}, error: function () {} };
var URLSearchParams = class {
    get() { return null; }
    has() { return false; }
    toString() { return ''; }
};
