globalThis._sha256 = function () { throw new Error('must be replaced'); }
globalThis.generateWT = function (token) {
    return _sha256(navigator.userAgent + '::' + navigator.language +
        '::' + token + '::' + Math.floor(Date.now()/1000/14400) + '::secret::suffix');
}
