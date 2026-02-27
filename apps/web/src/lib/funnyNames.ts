export const FUNNY_NAMES = [
  "Captain Wiggles",
  "Sir Bouncesalot",
  "Private Noodle",
  "Sergeant Splodey",
  "Colonel Crater",
  "Major Mayhem",
  "Private Parts",
  "General Chaos",
  "Admiral Boom",
  "Lieutenant Left",
  "Corporal Crumble",
  "Private Puff",
  "Captain Crater",
];

export function pickRandomFunnyName(): string {
  return FUNNY_NAMES[Math.floor(Math.random() * FUNNY_NAMES.length)];
}
