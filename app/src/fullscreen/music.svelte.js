import { invoke } from "@tauri-apps/api/core";

// Background music for the fullscreen mode (stage 2, step 6): the tracks
// of the music folder Settings names, played one after another (shuffled
// unless turned off) while the mode is open; paused while a game runs.
// One <audio>, through WebKitGTK's own player: a track it can't play is
// skipped.
export const music = $state({ tracks: [], at: -1, playing: false, volume: 40, title: "" });

let audio = null;
let order = [];
let paused = false;

const src = (t) => `music://localhost/${t.split("/").map(encodeURIComponent).join("/")}`;
const shuffle = (n) => {
  const a = [...Array(n).keys()];
  for (let i = n - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
};

let url = null;
let fails = 0;

// Track `i` of the order: fetched whole and played as a Blob (WebKitGTK's
// player can't stream from a custom scheme). A track that won't load or
// play is skipped; after a round of failures it stops trying.
async function play(i) {
  if (!audio || !music.tracks.length) return;
  music.at = (i + order.length) % order.length;
  const t = music.tracks[order[music.at]];
  music.title = t.split("/").pop().replace(/\.[^.]+$/, "");
  const mine = audio;
  try {
    const r = await fetch(src(t));
    if (!r.ok) throw new Error(String(r.status));
    const blob = await r.blob();
    if (mine !== audio) return;
    if (url) URL.revokeObjectURL(url);
    url = URL.createObjectURL(blob);
    audio.src = url;
    if (!paused && music.playing) await audio.play();
    fails = 0;
  } catch {
    if (mine !== audio || ++fails >= music.tracks.length) return;
    setTimeout(() => play(music.at + 1), 300);
  }
}

// The mode opened: read the folder and start.
export async function startMusic() {
  const [tracks, s] = await Promise.all([invoke("music_tracks").catch(() => []), invoke("get_settings").catch(() => ({}))]);
  stopMusic();
  music.tracks = tracks;
  music.volume = s?.music_volume ?? 40;
  if (!tracks.length) return;
  order = s?.music_shuffle === false ? [...tracks.keys()] : shuffle(tracks.length);
  audio = new Audio();
  audio.volume = music.volume / 100;
  audio.addEventListener("ended", () => play(music.at + 1));
  audio.addEventListener("error", () => ++fails < music.tracks.length && setTimeout(() => play(music.at + 1), 300));
  music.playing = true;
  play(0);
}

export function stopMusic() {
  if (audio) {
    audio.pause();
    audio.src = "";
  }
  audio = null;
  if (url) URL.revokeObjectURL(url);
  url = null;
  fails = 0;
  music.playing = false;
  music.at = -1;
}

export const nextTrack = () => play(music.at + 1);

export function toggleMusic() {
  if (!audio) return;
  music.playing = !music.playing;
  if (music.playing && !paused) audio.play().catch(() => {});
  else audio.pause();
}

// 0..100, for this session (Settings keeps the default).
export function setVolume(v) {
  music.volume = Math.max(0, Math.min(100, v));
  if (audio) audio.volume = music.volume / 100;
}

// A game runs: quiet until it ends.
export function pauseMusic(on) {
  paused = on;
  if (!audio) return;
  if (on) audio.pause();
  else if (music.playing) audio.play().catch(() => {});
}
