import init, {
  generate_nations_json,
  government_forms_json,
} from "./pkg/flavor.js";

const seedEl = document.getElementById("seed");
const countEl = document.getElementById("count");
const mixEl = document.getElementById("mix");
const rerollBtn = document.getElementById("reroll");
const randomSeedBtn = document.getElementById("random-seed");
const gridEl = document.getElementById("grid");
const formChipsEl = document.getElementById("form-chips");

function reroll() {
  const seed = BigInt(seedEl.value || "0");
  const count = Number(countEl.value || 10);
  const mixSpec = mixEl.value || "";
  const json = generate_nations_json(seed, count, mixSpec);
  const nations = JSON.parse(json);
  gridEl.replaceChildren(...nations.map(renderCard));
}

function renderCard(n) {
  const card = document.createElement("article");
  card.className = "card";

  const flag = document.createElement("div");
  flag.className = "flag";
  flag.innerHTML = n.flag_svg;

  const form = document.createElement("div");
  form.className = "form";
  form.textContent = n.government_title.replace(/ of .*/, "");

  const title = document.createElement("div");
  title.className = "title";
  title.textContent = n.government_title;

  const demonym = document.createElement("div");
  demonym.className = "demonym";
  demonym.textContent = `${n.adjective} · ${n.demonym_plural}`;

  card.append(flag, form, title, demonym);
  return card;
}

function renderFormChips(forms) {
  formChipsEl.replaceChildren(
    ...forms.map((f) => {
      const btn = document.createElement("button");
      btn.type = "button";
      const key = f.id.toLowerCase();
      btn.textContent = `${key} — ${f.label}`;
      btn.title = f.description;
      btn.dataset.key = key;
      btn.addEventListener("click", () => appendKey(key));
      return btn;
    }),
  );
}

function appendKey(key) {
  const current = mixEl.value.trim();
  const parts = current ? current.split(",").map((s) => s.trim()) : [];
  // If the key is already there, leave it alone — don't duplicate.
  if (parts.some((p) => p.toLowerCase().startsWith(`${key}=`))) return;
  parts.push(`${key}=25`);
  mixEl.value = parts.join(",");
  mixEl.focus();
}

function bindPresetButtons() {
  for (const btn of document.querySelectorAll(".presets button[data-mix]")) {
    btn.addEventListener("click", () => {
      mixEl.value = btn.dataset.mix;
      reroll();
    });
  }
}

async function main() {
  await init();
  const forms = JSON.parse(government_forms_json());
  renderFormChips(forms);
  bindPresetButtons();
  reroll();
}

rerollBtn.addEventListener("click", reroll);
randomSeedBtn.addEventListener("click", () => {
  seedEl.value = Math.floor(Math.random() * 1_000_000);
  reroll();
});
for (const el of [seedEl, countEl, mixEl]) {
  el.addEventListener("keydown", (e) => {
    if (e.key === "Enter") reroll();
  });
}

main().catch((err) => {
  gridEl.textContent = `Failed to load WASM: ${err}`;
});
