import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";
const dom = new JSDOM("<!doctype html><html><body></body></html>");
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLMediaElement", "Element", "Node", "Text", "Comment", "Event", "MutationObserver"]) Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
dom.window.HTMLElement.prototype.scrollIntoView = function () {};
globalThis.matchMedia = () => ({ matches: true });
const { mount, unmount, flushSync } = await import("svelte");
const stub = async source => dataModule(await createModuleLoader({}).resolveImports(compile(source, { generate: "client" }).js.code, "stub.svelte"));
const store = import.meta.resolve("svelte/store");
const { load } = createModuleLoader({
  "src/lib/i18n": dataModule(`import { writable } from ${JSON.stringify(store)}; export const language=writable('it'); export const t=(text,_lang,args=[])=>text.replace(/\\{(\\d+)\\}/g,(_,i)=>args[i]);`),
  "src/lib/features/library/SteamArtwork.svelte": await stub('<script>let {alt="",index}=$props();</script><img {alt} data-screenshot={index} />'),
  "src/lib/features/library/ArtworkViewer.svelte": await stub('<script>let {index,onClose}=$props();</script><div role="dialog" aria-label="Zoom" data-index={index}><button onclick={onClose}>Close</button></div>'),
  "src/lib/services/steam-trailer": dataModule('export function loadSteamTrailer(video,movie,onError){globalThis.galleryFixture.opened.push(movie.id);globalThis.galleryFixture.onError=onError;return ()=>globalThis.galleryFixture.closed.push(movie.id);}'),
});

test("trailers and screenshots share a gallery and changing media stops playback", async () => {
  globalThis.galleryFixture = { opened: [], closed: [] };
  const Component = (await import(await load("src/lib/features/library/ScreenshotGallery.svelte"))).default;
  const target = document.createElement("div");document.body.append(target);
  const instance = mount(Component, {target,props:{steamAppId:400,name:"Portal",screenshots:[{thumbnail:"https://cdn.steamstatic.com/s.jpg",full:"https://cdn.steamstatic.com/full.jpg"}],movies:[{id:922,name:"Portal Trailer",thumbnail:"https://cdn.steamstatic.com/m.jpg",hls:"https://video.akamai.steamstatic.com/movie.m3u8",mp4:null,webm:null}]}});
  try {
    flushSync();
    const video = target.querySelector("video");
    assert.ok(video, "the first trailer appears alongside the screenshots");
    assert.equal(video.controls, false);
    assert.equal(video.autoplay, false);
    const screenshots = [...target.querySelectorAll("button")].filter(button=>button.getAttribute("aria-label")==="Mostra schermata 1");
    screenshots[0].click();flushSync();
    assert.equal(target.querySelector("video"), null);
    assert.deepEqual(globalThis.galleryFixture.closed, [922]);
    [...target.querySelectorAll("button")].find(button=>button.getAttribute("aria-label")==="Ingrandisci schermata 1 di 1").click();flushSync();
    assert.equal(target.querySelector('[role="dialog"]').dataset.index, "0", "trailer slots must not offset screenshot zoom indices");
    target.querySelector('[role="dialog"] button').click();flushSync();
    [...target.querySelectorAll("button")].find(button=>button.getAttribute("aria-label")==="Mostra trailer: Portal Trailer").click();flushSync();
    assert.ok(target.querySelector("video"));
  } finally { await unmount(instance);target.remove();delete globalThis.galleryFixture; }
});


test("custom trailer controls play, seek, mute and change volume", async () => {
  globalThis.galleryFixture = { opened: [], closed: [] };
  const Component = (await import(await load("src/lib/features/library/TrailerPlayer.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(Component, {target,props:{movie:{id:922,name:"Portal",thumbnail:null,hls:null,mp4:"https://cdn.steamstatic.com/movie.mp4",webm:null}}});
  let plays=0;let pauses=0;
  try {
    flushSync();const video=target.querySelector("video");
    video.play=async()=>{plays++;video.dispatchEvent(new Event("play"));};
    video.pause=()=>{pauses++;video.dispatchEvent(new Event("pause"));};
    Object.defineProperty(video,"duration",{value:90});
    video.dispatchEvent(new Event("loadedmetadata"));flushSync();
    const button=name=>[...target.querySelectorAll("button")].find(button=>button.getAttribute("aria-label")===name);
    assert.equal(video.controls,false);
    assert.ok(button("Riproduci trailer"));
    button("Riproduci trailer").click();await new Promise(resolve=>setImmediate(resolve));flushSync();
    assert.equal(plays,1);button("Metti in pausa").click();flushSync();assert.equal(pauses,1);
    const seek=target.querySelector('input[aria-label="Avanzamento del trailer"]');
    assert.equal(seek.disabled,false);seek.value="45";seek.dispatchEvent(new Event("input",{bubbles:true}));flushSync();
    assert.equal(video.currentTime,45);assert.ok(target.textContent.includes("0:45 / 1:30"));
    button("Disattiva audio").click();flushSync();assert.equal(video.muted,true);
    const volume=target.querySelector('input[aria-label="Volume del trailer"]');volume.value="0.4";
    volume.dispatchEvent(new Event("input",{bubbles:true}));flushSync();assert.equal(video.volume,0.4);assert.equal(video.muted,false);
  } finally { await unmount(instance);target.remove();delete globalThis.galleryFixture; }
});


test("failed trailers show a useful retry action and replace the failed player", async () => {
  globalThis.galleryFixture={opened:[],closed:[]};
  const Component=(await import(await load("src/lib/features/library/TrailerPlayer.svelte"))).default;
  const target=document.createElement("div");document.body.append(target);
  const instance=mount(Component,{target,props:{movie:{id:7,name:"Test",thumbnail:null,hls:"https://video.akamai.steamstatic.com/test.m3u8",mp4:null,webm:null}}});
  try {
    flushSync();
    globalThis.galleryFixture.onError("Connection reset by peer");flushSync();
    assert.ok(target.querySelector('[role="alert"]'));
    assert.equal(target.textContent.includes("Connection reset by peer"),false,"raw network errors must not replace the page or reach the user");
    const retry=[...target.querySelectorAll("button")].find(button=>button.textContent.trim()==="Riprova");
    assert.ok(retry);retry.click();flushSync();
    assert.equal(target.querySelector('[role="alert"]'),null);
    assert.deepEqual(globalThis.galleryFixture.opened,[7,7]);
    assert.deepEqual(globalThis.galleryFixture.closed,[7]);
  } finally {await unmount(instance);target.remove();delete globalThis.galleryFixture;}
});
