import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";
const dom=new JSDOM('<!doctype html><html><body></body></html>');
const {load}=createModuleLoader({"src/lib/i18n":dataModule("export const t=value=>value;"),"hls.js":dataModule(`export default class Hls {
 static Events={ERROR:"error"};static ErrorTypes={NETWORK_ERROR:"network",MEDIA_ERROR:"media"};static isSupported(){return true;}
 constructor(){globalThis.trailerFixture.hls=this;this.restarts=[];this.recoveries=0;this.destroyed=0;}
 on(_event,callback){this.callback=callback;}loadSource(){}attachMedia(){}startLoad(time){this.restarts.push(time);}recoverMediaError(){this.recoveries++;}destroy(){this.destroyed++;}
}`)});
const {loadSteamTrailer}=await import(await load("src/lib/services/steam-trailer.ts"));
const movie={id:1,name:"Test",thumbnail:null,hls:"https://video.akamai.steamstatic.com/movie.m3u8",mp4:null,webm:null};
const settle=()=>new Promise(resolve=>setImmediate(resolve));
function video(){const v=dom.window.document.createElement("video");v.pause=()=>{};v.load=()=>{};return v;}

test("stream resets retry twice at the current position, then fail once",async()=>{
 globalThis.trailerFixture={};const v=video();v.currentTime=12.5;const errors=[];
 const destroy=loadSteamTrailer(v,movie,message=>errors.push(message));await settle();
 try {
  const hls=globalThis.trailerFixture.hls;
  v.dispatchEvent(new dom.window.Event("error"));assert.equal(errors.length,0,"HLS owns recovery while attached");
  for(let i=0;i<2;i++)hls.callback("error",{fatal:true,type:"network",details:"Connection reset by peer"});
  assert.deepEqual(hls.restarts,[12.5,12.5]);assert.equal(errors.length,0);
  hls.callback("error",{fatal:true,type:"network",details:"Connection reset by peer"});
  hls.callback("error",{fatal:true,type:"network",details:"Connection reset by peer"});
  assert.equal(errors.length,1);assert.equal(hls.destroyed,1);
 } finally {destroy();delete globalThis.trailerFixture;}
});
test("decoder recovery shares the finite recovery budget",async()=>{
 globalThis.trailerFixture={};const errors=[];const destroy=loadSteamTrailer(video(),movie,message=>errors.push(message));await settle();
 try {
  const hls=globalThis.trailerFixture.hls;
  hls.callback("error",{fatal:true,type:"media",details:"decoder"});assert.equal(hls.recoveries,1);
  hls.callback("error",{fatal:true,type:"network",details:"reset"});assert.equal(hls.restarts.length,1);
  hls.callback("error",{fatal:true,type:"media",details:"decoder"});assert.equal(errors.length,1);
 } finally {destroy();delete globalThis.trailerFixture;}
});
test("switching trailers cancels pending setup and ignores late failures",async()=>{
 globalThis.trailerFixture={};const errors=[];const v=video();const destroy=loadSteamTrailer(v,movie,message=>errors.push(message));destroy();await settle();
 assert.equal(globalThis.trailerFixture.hls,undefined);v.dispatchEvent(new dom.window.Event("error"));assert.deepEqual(errors,[]);delete globalThis.trailerFixture;
});
