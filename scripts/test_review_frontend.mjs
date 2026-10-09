import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createRequire } from 'node:module';
import { createModuleLoader, dataModule } from './test_module_loader.mjs';
const require = createRequire(new URL('../package.json', import.meta.url));
const { JSDOM } = require('jsdom');
const { compile } = require('svelte/compiler');
const dom = new JSDOM('<!doctype html><html><body></body></html>', { pretendToBeVisual: true });
for (const key of ['window','document','navigator','HTMLElement','HTMLDialogElement','HTMLMediaElement','HTMLInputElement','Element','Node','Text','Comment','Event','MouseEvent','CustomEvent','MutationObserver','getComputedStyle']) Object.defineProperty(globalThis,key,{ configurable:true,value:dom.window[key] });
globalThis.requestAnimationFrame = cb => setTimeout(cb, 0);
globalThis.cancelAnimationFrame = clearTimeout;
HTMLDialogElement.prototype.showModal = function() { this.setAttribute('open',''); };
HTMLDialogElement.prototype.close = function() { this.removeAttribute('open'); };
const svelteUrl = 'file://' + require.resolve('svelte');
const storeUrl = 'file://' + require.resolve('svelte/store');
const { mount, unmount, flushSync } = await import(svelteUrl);
const { get } = await import(storeUrl);
const storeImport = `import { writable } from ${JSON.stringify(storeUrl)};`;
const empty = dataModule('export default () => {};');
const d = dataModule;
const mocks = {
 'src/lib/services/theme-presets.json': d('export default ' + JSON.stringify(require('./src/lib/services/theme-presets.json')) + ';'),
 'src/lib/i18n': d(storeImport + 'export const language=writable("it");export const t=(value,_language,args=[])=>value.replace(/\\{(\\d+)\\}/g,(_,i)=>args[i]);'),
 'src/lib/utils/motion': d('export const reducedMotion=true;export const fadeDuration=0;'),
 'src/lib/stores/window-activity': d(storeImport + 'export const windowActive=writable(true);'),
 'src/lib/services/local-state': d('export const getSettings=async()=>globalThis.frontendRepro.current;export const saveSettings=value=>globalThis.frontendRepro.saveSettings(value);'),
 'src/lib/services/dialog': d('export const pickGameDirectory=async()=>globalThis.frontendRepro.pickedDirectory??null;export const pickExecutableFile=async()=>null;'),
 'src/lib/features/settings/SourceSettings.svelte': empty,
 'src/lib/components/ui/ErrorBanner.svelte': empty,
 'src/lib/stores/bootstrap': d('export const configureSteamScanInterval=()=>{};'),
 'src/lib/services/app-updater': d(storeImport + 'export const updateState=writable({});export const checkForAppUpdate=()=>{};export const installAppUpdate=()=>{};'),
 'src/lib/services/downloads': d(`
 export const listDownloads=async()=>[];export const getDownloadBandwidthLimit=async()=>globalThis.frontendRepro.bandwidth;
 export const getInstalledFolderInfo=async()=>({directory:(globalThis.frontendRepro.current.downloadPath??'/old')+'/installed',freeBytes:300,totalBytes:500});export const openInstalledFolder=async()=>{};
 export const setDownloadBandwidthLimit=async value=>{globalThis.frontendRepro.bandwidth=value;globalThis.frontendRepro.bandwidthCalls.push(value);};
 export const isActiveDownloadStatus=()=>false;export const isFinishedDownloadStatus=()=>false;
 export const cancelDownload=()=>{};export const pauseDownload=()=>{};export const queueDownload=()=>{};export const removeDownload=()=>{};
 export const removeFinishedDownloads=()=>{};export const reorderDownloads=()=>{};export const resumeDownload=()=>{};export const retryDownload=()=>{};`),
 'src/lib/stores/games': d('export const games={load:async()=>{}};export const reconcileGame=()=>{};export const saveGame=async()=>{};'),
 'src/lib/services/manual-import': d(`export const scanGameExecutables=(directory)=>globalThis.frontendRepro.scans.get(directory).promise;
 export const identifyManualGameSteamAppId=()=>{};export const importManualGame=()=>{};export const setGameExecutable=()=>{};`),
 'src/lib/stores/app-info': d(storeImport+'export const appInfo=writable({data:{platform:"linux",trayAvailable:true}});'),
 'src/lib/services/steam-accounts': d(`export const inspectSteamGameLaunch=async()=>({status:'unknown',steamRunning:true,targetAccountName:'saved'});
 export const launchSteamGame=async(id,confirmed)=>{globalThis.frontendRepro.launchCalls.push({id,confirmed});if(!confirmed)throw new Error("Confirm closing Steam before switching to this game's saved account.");};
 export const listGameLaunchStates=async()=>[];export const cancelGameLaunch=()=>{};export const onGameLaunchStates=()=>{};export const onShortcutLaunchFailure=()=>{};export const stopGame=()=>{};`),
 'src/lib/services/launch': d('export const launchConfiguredGameWithRunner=async()=>{};export const launchNativeGame=async()=>{};'),
 'src/lib/services/game-settings': d('export const getCompatibilityLogsDirectory=async()=>"/logs";'),
 'src/lib/services/window': d('export const hideWindow=async()=>{};export const showWindow=async()=>{};'),
 'src/lib/stores/toast': d('export const showToast=()=>{};'),
};
globalThis.frontendRepro={current:{},bandwidth:1049,bandwidthCalls:[],scans:new Map(),launchCalls:[],saveSettings:async value=>{globalThis.frontendRepro.current=value;return value;}};
const loader=createModuleLoader(mocks);
const settle=async()=>{for(let i=0;i<6;i++){await new Promise(resolve=>setImmediate(resolve));flushSync();}};
const defer=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
const settingsModule=await import(await loader.load('src/lib/stores/settings.ts'));
const {settings,defaultSettings}=settingsModule;
settings.set(structuredClone(defaultSettings));
const navigation=await import(await loader.load('src/lib/stores/navigation.ts'));


const downloads=await import(await loader.load('src/lib/stores/downloads.ts'));
const DownloadSettings=(await import(await loader.load('src/lib/features/settings/DownloadSettings.svelte'))).default;
const manual=await import(await loader.load('src/lib/stores/manual-import.ts'));

test("sidebar opens the general settings category", async () => {
// Actual SidebarButton and actual navigation callback, using the same binding as Sidebar.
const wrapper=compile(`<script>import SidebarButton from '../src/lib/components/ui/SidebarButton.svelte';import {openSettings} from '../src/lib/stores/navigation';</script><SidebarButton label="Settings" expanded={true} onClick={openSettings}/>`,{generate:'client',filename:'scripts/repro-wrapper.svelte'}).js.code;
const Wrapper=(await import(d(await loader.resolveImports(wrapper,'scripts/repro-wrapper.svelte')))).default;
let target=document.createElement('div');document.body.append(target);
let instance=mount(Wrapper,{target});await settle();target.querySelector('button').click();await settle();
assert.equal(get(navigation.settingsOpen),true);assert.equal(get(navigation.settingsCategory),'general');
await unmount(instance);target.remove();
});

test("display rounding does not save a bandwidth limit", async () => {
let target, instance;
// Actual DownloadSettings mounts from a persisted precise byte count, without any user input.
downloads.bandwidthLimit.set(1049);

target=document.createElement('div');document.body.append(target);instance=mount(DownloadSettings,{target});await settle();
assert.equal(target.querySelector('#download-bandwidth-limit').value,'0');await new Promise(resolve=>setTimeout(resolve,550));await settle();
assert.deepEqual(frontendRepro.bandwidthCalls,[]);assert.equal(get(downloads.bandwidthLimit).data,1049);
await unmount(instance);target.remove();
});

test("obsolete scans cannot overwrite a newer directory or reset", async () => {
// Actual manual import store, delayed scan responses from two folders.

frontendRepro.scans.set('/A',defer());frontendRepro.scans.set('/B',defer());
const a=manual.rescanDirectory('/A');const b=manual.rescanDirectory('/B');
frontendRepro.scans.get('/B').resolve({candidates:[{path:'/B/game.exe'}],selectedPath:'/B/game.exe'});await b;
frontendRepro.scans.get('/A').resolve({candidates:[{path:'/A/game.exe'}],selectedPath:'/A/game.exe'});await a;
assert.equal(get(manual.manualImport).directory,'/B');assert.equal(get(manual.manualImport).selectedPath,'/B/game.exe');
frontendRepro.scans.set('/A',defer());const closing=manual.rescanDirectory('/A');manual.resetManualImport();
frontendRepro.scans.get('/A').resolve({candidates:[{path:'/A/game.exe'}],selectedPath:'/A/game.exe'});await closing;
assert.equal(get(manual.manualImport).directory,null);assert.equal(get(manual.manualImport).selectedPath,null);
});

test("concurrent preference edits merge against the last saved document", async () => {
let target, instance;
// Actual GeneralSettings and nested DiscordPresenceSettings submit stale full snapshots concurrently.
settings.set(structuredClone(defaultSettings));const saves=[];
frontendRepro.saveSettings=value=>{const pending=defer();saves.push({value,pending});return pending.promise;};
const GeneralSettings=(await import(await loader.load('src/lib/features/settings/GeneralSettings.svelte'))).default;
target=document.createElement('div');document.body.append(target);instance=mount(GeneralSettings,{target});await settle();
const checkboxFor=label=>[...target.querySelectorAll('label')].find(element=>element.textContent.includes(label)).querySelector('input');
checkboxFor('Nascondi Legio').click();await settle();const discord=checkboxFor('Mostra la tua attività su Discord');assert.equal(discord.disabled,false);discord.click();await settle();
assert.equal(saves.length,1);assert.equal(saves[0].value.hideOnGameStart,false);
saves[0].pending.resolve(saves[0].value);await settle();assert.equal(saves.length,2);assert.equal(saves[1].value.hideOnGameStart,false);assert.equal(saves[1].value.discordPresence.enabled,false);saves[1].pending.resolve(saves[1].value);await settle();
assert.equal(get(settings).data.hideOnGameStart,false);assert.equal(get(settings).data.discordPresence.enabled,false);
await unmount(instance);target.remove();
});

test("unknown running Steam account opens the confirmation dialog", async () => {
// Actual launch store receives an inspection produced by native unknown-active-account path.
const launch=await import(await loader.load('src/lib/stores/launch.ts'));
await launch.playGame({id:'steam-game',steamInstallPath:'/steam/game',executablePath:null});
assert.equal(get(launch.accountSwitchGame).id,'steam-game');assert.deepEqual(frontendRepro.launchCalls,[]);assert.equal(get(launch.launchError),null);launch.dismissAccountSwitch();
});

test("changing download root reloads installed folder and disk information", async () => {
let target, instance;
// Changing the download root updates settings but never invalidates installed-folder metadata.
settings.set({...structuredClone(defaultSettings), downloadPath:null});
downloads.bandwidthLimit.set(0);
downloads.installedFolder.set({directory:'/old/installed',freeBytes:100,totalBytes:200});
frontendRepro.pickedDirectory='/new';frontendRepro.saveSettings=async value=>{frontendRepro.current=value;return value;};
target=document.createElement('div');document.body.append(target);instance=mount(DownloadSettings,{target});await settle();
[...target.querySelectorAll('button')].find(button=>button.textContent.includes('Imposta cartella...')).click();await settle();
assert.equal(get(settings).data.downloadPath,'/new');assert.equal(get(downloads.installedFolder).data.directory,'/new/installed');assert.equal(get(downloads.installedFolder).data.freeBytes,300);
assert.ok(target.textContent.includes('/new'));assert.ok(target.textContent.includes('/new/installed'));
await unmount(instance);target.remove();
});

test("closing game settings flushes the final draft after an in-flight save", async () => {
let target, instance;
// Actual per-game panel snapshots across category destruction while a previous save is awaiting its reply.
const nativeSaves=[];frontendRepro.nativeConfig={arguments:[],workingDirectory:null,environment:{}};
frontendRepro.saveNative=value=>{frontendRepro.nativeConfig=structuredClone(value);const pending=defer();nativeSaves.push({value,pending});return pending.promise;};
const nativeMocks={...mocks,
 'src/lib/stores/app-info':d(storeImport+'export const appInfo=writable({status:"ready",data:{platform:"windows"}});'),
 'src/lib/services/game-settings':d(`
 export const emptyGameCompatibilityOverrides={};export const getCompatibilityDefaults=async()=>({});export const getGameCompatibilityOverrides=async()=>({});
 export const getGameOnlineFixDetected=async()=>false;export const listCompatibilityRunners=async()=>({});export const saveGameCompatibilityOverrides=async()=>({});
 export const isGraphicsRenderer=()=>false;export const isWaylandMode=()=>false;
 export const getNativeLaunchConfig=async()=>structuredClone(globalThis.frontendRepro.nativeConfig);
 export const saveNativeLaunchConfig=async(id,value)=>globalThis.frontendRepro.saveNative(value);`),
};
const nativeLoader=createModuleLoader(nativeMocks);
const GameSettingsPanel=(await import(await nativeLoader.load('src/lib/features/library/GameSettingsPanel.svelte'))).default;
const manualGame={id:'native-game',name:'Native Game',steamAppId:null,steamInstallPath:null,executablePath:'C:/game.exe'};
target=document.createElement('div');document.body.append(target);instance=mount(GameSettingsPanel,{target,props:{game:manualGame,section:'launch'}});await settle();
const edit=(input,value)=>{input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}));};
edit(target.querySelector('#native-arguments'),'-a');await settle();await new Promise(resolve=>setTimeout(resolve,600));await settle();assert.equal(nativeSaves.length,1);
edit(target.querySelector('#native-arguments'),'-b');await settle();
assert.equal(target.querySelector('#native-arguments').value,'-b');
await unmount(instance);target.remove();
nativeSaves[0].pending.resolve(nativeSaves[0].value);await settle();
assert.equal(nativeSaves.length,2);assert.deepEqual(frontendRepro.nativeConfig.arguments,['-b']);nativeSaves[1].pending.resolve(nativeSaves[1].value);await settle();
});

test("fullscreen artwork uses a native modal and restores its opener", async () => {
  const viewerLoader = createModuleLoader({ ...mocks, "src/lib/features/library/SteamArtwork.svelte": empty });
  const ArtworkViewer = (await import(await viewerLoader.load("src/lib/features/library/ArtworkViewer.svelte"))).default;
  const opener = document.createElement("button");
  const target = document.createElement("div");
  document.body.append(opener, target);
  opener.focus();
  let closed = 0;
  const instance = mount(ArtworkViewer, { target, props: { steamAppId: 1, asset: "hero", onClose: () => closed++ } });
  try {
    await settle();
    const dialog = target.querySelector("dialog");
    assert.ok(dialog, "fullscreen artwork must participate in the browser modal lifecycle");
    assert.equal(dialog.open, true);
    dialog.dispatchEvent(new Event("cancel", { cancelable: true }));
    assert.equal(closed, 1);
  } finally {
    await unmount(instance);
    assert.equal(document.activeElement, opener);
    opener.remove(); target.remove();
  }
});
