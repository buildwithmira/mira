// Video frames: a pill to play and pause, a progress line on the bottom
// edge, and data-playing on the frame so its handles turn ember.
for (const frame of document.querySelectorAll(".mira-frame--video")) {
  const video = frame.querySelector("video");
  const pill = frame.querySelector(".mira-frame__play");
  const progress = frame.querySelector(".mira-frame__progress");
  if (!video || !pill) continue;

  const sync = () => {
    const playing = !video.paused && !video.ended;
    frame.toggleAttribute("data-playing", playing);
    pill.textContent = playing ? "Pause" : video.ended ? "Replay" : "Play";
    pill.setAttribute("aria-label", pill.textContent);
  };
  pill.addEventListener("click", () => (video.paused || video.ended ? video.play() : video.pause()));
  video.addEventListener("click", () => (video.paused ? video.play() : video.pause()));
  for (const event of ["play", "pause", "ended"]) video.addEventListener(event, sync);
  video.addEventListener("timeupdate", () => {
    const p = video.duration ? video.currentTime / video.duration : 0;
    progress?.style.setProperty("--p", p.toFixed(4));
  });
  video.addEventListener("loadeddata", () => frame.setAttribute("data-loaded", ""), { once: true });
}
