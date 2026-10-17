(function () {
    var rawHashInput;
    globalThis._sha256 = function (input) {
        rawHashInput = String(input);
        return '0'.repeat(64);
    };
    generateWT('HydraGofileToken');
    return rawHashInput;
})();
