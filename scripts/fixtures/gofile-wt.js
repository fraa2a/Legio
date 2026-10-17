function _sha256(value) { throw new Error('must be replaced'); }
function generateWT(token) {
    return _sha256(navigator.userAgent + '::' + navigator.language +
        '::' + token + '::' + Math.floor(Date.now()/1000/14400) + '::secret::suffix');
}
