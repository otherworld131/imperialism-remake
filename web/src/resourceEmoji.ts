const EMOJI: Record<string, string> = {
  // Resources
  Grain:      '\u{1F33E}',
  Fruit:      '\u{1F34E}',
  Cotton:     '\u{1F331}',
  Wool:       '\u{1F411}',
  Timber:     '\u{1FAB5}',
  Livestock:  '\u{1F404}',
  Fish:       '\u{1F41F}',
  Horses:     '\u{1F434}',
  Coal:       '⚫',
  Iron:       '\u{1F518}',
  Gold:       '\u{1F4B0}',
  Gems:       '\u{1F48E}',
  Oil:        '\u{1F6E2}️',
  // Materials
  Lumber:     '🪵',
  Steel:      '⚙️',
  Fabric:     '🧵',
  Paper:      '📄',
  Arms:       '🔫',
  CannedFood: '🥫',
  'Canned Food': '🥫',
  // Goods
  Furniture:  '🪑',
  Clothing:   '👗',
  Hardware:   '🔧',
};

export function resourceEmoji(name: string): string {
  return EMOJI[name] ?? '';
}

export function resourceLabel(name: string): string {
  const e = EMOJI[name];
  return e ? `${e} ${name}` : name;
}

export { EMOJI as RESOURCE_EMOJI_MAP };
