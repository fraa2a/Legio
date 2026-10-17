globalThis.legioBuildReleaseIndex = function (titles) {
    globalThis.legioReleaseIndex = new FlexSearch.Index();
    globalThis.legioReleaseCount = titles.length;
    for (let index = 0; index < titles.length; index++) {
        if (titles[index]) globalThis.legioReleaseIndex.add(index, titles[index]);
    }
};
globalThis.legioSearchReleases = function (title) {
    return JSON.stringify(globalThis.legioReleaseIndex.search(title, { limit: globalThis.legioReleaseCount }));
};
