// Screenshots: every one links to its full size image; with JavaScript a tap opens it in a dialog instead, where swipes,
// the arrow buttons and the arrow keys go through all of them. The hero's phones are a scroll-snapping carousel on
// narrow screens (CSS), with dots that follow the scrolling.
"use strict";

const shots = [...document.querySelectorAll("a.zoom")];
const dialog = document.getElementById("zoom");
const big = dialog.querySelector("img");
const caption = dialog.querySelector("figcaption");
let current = 0;

function show(index) {
  current = (index + shots.length) % shots.length;
  const thumb = shots[current].querySelector("img");
  big.src = shots[current].href;
  big.alt = thumb.alt;
  caption.textContent = shots[current].closest("figure")?.querySelector("figcaption")?.textContent || thumb.alt;
}

shots.forEach((link, index) =>
  link.addEventListener("click", (event) => {
    // A middle click or Ctrl/Cmd click still opens the image in a new tab.
    if (event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey) return;
    event.preventDefault();
    show(index);
    dialog.showModal();
  }),
);

dialog.querySelector(".close").addEventListener("click", () => dialog.close());
dialog.querySelector(".prev").addEventListener("click", () => show(current - 1));
dialog.querySelector(".next").addEventListener("click", () => show(current + 1));
// A tap outside the image closes, like the backdrop of a native viewer.
dialog.addEventListener("click", (event) => {
  if (event.target === dialog || event.target.tagName === "FIGURE") dialog.close();
});
dialog.addEventListener("keydown", (event) => {
  if (event.key === "ArrowLeft") show(current - 1);
  if (event.key === "ArrowRight") show(current + 1);
});

let touchX = null;
dialog.addEventListener("touchstart", (event) => {
  // Two fingers are a pinch zoom, not a swipe.
  touchX = event.touches.length === 1 ? event.touches[0].clientX : null;
}, { passive: true });
dialog.addEventListener("touchend", (event) => {
  if (touchX === null) return;
  const dx = event.changedTouches[0].clientX - touchX;
  if (Math.abs(dx) > 50) show(current + (dx < 0 ? 1 : -1));
  touchX = null;
});

// Carousel dots: one per phone, the one nearest the middle is current; tapping a dot scrolls to its phone.
const track = document.getElementById("phones");
const dots = document.getElementById("dots");
const phones = [...track.children];
phones.forEach((phone, index) => {
  const dot = document.createElement("button");
  dot.type = "button";
  dot.setAttribute("aria-label", `Screenshot ${index + 1}`);
  dot.addEventListener("click", () =>
    track.scrollTo({ left: phone.offsetLeft - (track.clientWidth - phone.clientWidth) / 2, behavior: "smooth" }),
  );
  dots.append(dot);
});

function updateDots() {
  const middle = track.scrollLeft + track.clientWidth / 2;
  let nearest = 0;
  phones.forEach((phone, index) => {
    const distance = Math.abs(phone.offsetLeft + phone.clientWidth / 2 - middle);
    const best = Math.abs(phones[nearest].offsetLeft + phones[nearest].clientWidth / 2 - middle);
    if (distance < best) nearest = index;
  });
  [...dots.children].forEach((dot, index) => dot.toggleAttribute("aria-current", index === nearest));
}
track.addEventListener("scroll", updateDots, { passive: true });
window.addEventListener("resize", updateDots);
updateDots();
