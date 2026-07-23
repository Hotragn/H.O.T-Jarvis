// Scroll reveals + performance-correct lazy video. The whole point: nothing
// heavy loads until it's needed, and never under reduced motion.

const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// ---- scroll reveals ----
const revealables = document.querySelectorAll(".reveal");
if (reduceMotion || !("IntersectionObserver" in window)) {
  revealables.forEach((el) => el.classList.add("in"));
} else {
  const revealer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          entry.target.classList.add("in");
          revealer.unobserve(entry.target);
        }
      }
    },
    { rootMargin: "0px 0px -12% 0px", threshold: 0.15 },
  );
  revealables.forEach((el) => revealer.observe(el));
}

// ---- lazy video ----
// Each .video-slot ships a real SVG poster and, once a clip exists, flips
// data-has-asset="true". Only then do we attach sources — AV1/WebM first,
// MP4 fallback — and only when the slot scrolls into view. Under reduced
// motion we never load video at all; the poster is the experience.
function attachVideo(slot) {
  const name = slot.dataset.slot;
  const poster = slot.querySelector(".poster");
  const video = document.createElement("video");
  video.muted = true;
  video.loop = true;
  video.playsInline = true;
  video.autoplay = true;
  video.preload = "auto";
  if (poster) video.poster = poster.getAttribute("src");

  // AV1/WebM first when a slot advertises one (data-webm="true"), else the
  // MP4 that plays everywhere. We only emit sources that exist, so no 404s.
  if (slot.dataset.webm === "true") {
    const webm = document.createElement("source");
    webm.src = `assets/video/${name}.webm`;
    webm.type = "video/webm";
    video.append(webm);
  }
  const mp4 = document.createElement("source");
  mp4.src = `assets/video/${name}.mp4`;
  mp4.type = "video/mp4";
  video.append(mp4);

  // Only swap the poster out once the clip can actually play, so a missing or
  // slow asset never leaves an empty box.
  video.addEventListener(
    "canplay",
    () => {
      if (poster) poster.style.opacity = "0";
      video.play().catch(() => {});
    },
    { once: true },
  );
  slot.appendChild(video);
}

const videoSlots = document.querySelectorAll('.video-slot[data-has-asset="true"]');
if (!reduceMotion && videoSlots.length && "IntersectionObserver" in window) {
  const loader = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          attachVideo(entry.target);
          loader.unobserve(entry.target);
        }
      }
    },
    { rootMargin: "200px 0px", threshold: 0.01 },
  );
  videoSlots.forEach((slot) => loader.observe(slot));
}
