import React from 'react';
import Flag from './Flag';
import { RESOURCE_EMOJI_MAP } from '../resourceEmoji';

interface NationLite {
  id: number;
  name: string;
  color: string;
  nation_type: string;
  government_title?: string;
  flag_svg?: string;
}

interface Props {
  nations?: NationLite[];
  onClose: () => void;
}

const NATION_COLORS: Record<string, string> = {
  Yellow: '#ffd900', Orange: '#ff8c00', LightBlue: '#66b3ff',
  Red: '#e62626', Green: '#1abf1a', Purple: '#a633d9',
  Blue: '#3359e6',
  Crimson: '#b00020', Magenta: '#d913a8', Forest: '#1f5b2c',
  Gold: '#d4a52a', Aqua: '#00b8c4', Violet: '#8a2be2',
  BurntOrange: '#cc5500', HotPink: '#ff44a0', Turquoise: '#14b89c',
  Slate: '#5a6e8c', Mauve: '#b07ab0', Sage: '#7a9b6a',
  Mustard: '#b88a00',
  Gray: '#999', Brown: '#8c5926',
  Pink: '#ff80b3', Teal: '#00b3a6', Olive: '#808000',
  Maroon: '#800000', Navy: '#000080', Cyan: '#00bcd4',
  Lime: '#a0e000', Coral: '#ff7f50', Lavender: '#b399d4',
  Tan: '#d2b48c', Salmon: '#fa8072', Khaki: '#bdb76b',
  Indigo: '#4b0082',
};

const TERRAIN_LEGEND = [
  { name: 'Grassland', color: '#a8b860', desc: 'Fertile plains for farming' },
  { name: 'Hills', color: '#9a8a68', desc: 'Elevated terrain, +30% defense' },
  { name: 'Forest', color: '#3a7a3a', desc: 'Timber source, +20% defense' },
  { name: 'Mountain', color: '#7a7068', desc: 'Mineral-rich, +50% defense' },
  { name: 'Desert', color: '#d8c888', desc: 'Arid terrain, limited use' },
  { name: 'Swamp', color: '#5a7a5a', desc: 'Difficult terrain, +15% defense' },
  { name: 'Tundra', color: '#b8c8d0', desc: 'Cold terrain, limited farming' },
  { name: 'Sea', color: '#4a88b8', desc: 'Naval zones for trade and combat' },
];

const RESOURCE_LEGEND = [
  { name: 'Grain',     emoji: RESOURCE_EMOJI_MAP['Grain'],     desc: 'Food staple, farmed on grassland' },
  { name: 'Fruit',     emoji: RESOURCE_EMOJI_MAP['Fruit'],     desc: 'Food resource, farmed on grassland' },
  { name: 'Cotton',    emoji: RESOURCE_EMOJI_MAP['Cotton'],    desc: 'Textile raw material' },
  { name: 'Wool',      emoji: RESOURCE_EMOJI_MAP['Wool'],      desc: 'Textile raw material from ranching' },
  { name: 'Timber',    emoji: RESOURCE_EMOJI_MAP['Timber'],    desc: 'Wood from forests, makes lumber' },
  { name: 'Livestock', emoji: RESOURCE_EMOJI_MAP['Livestock'], desc: 'Food resource from ranching' },
  { name: 'Horses',    emoji: RESOURCE_EMOJI_MAP['Horses'],    desc: 'Required for cavalry units' },
  { name: 'Coal',      emoji: RESOURCE_EMOJI_MAP['Coal'],      desc: 'Industrial fuel, mined in mountains/hills' },
  { name: 'Iron',      emoji: RESOURCE_EMOJI_MAP['Iron'],      desc: 'Makes steel, mined in mountains/hills' },
  { name: 'Gold',      emoji: RESOURCE_EMOJI_MAP['Gold'],      desc: 'Monetary resource, high value' },
  { name: 'Gems',      emoji: RESOURCE_EMOJI_MAP['Gems'],      desc: 'Precious stones, very high value' },
  { name: 'Oil',       emoji: RESOURCE_EMOJI_MAP['Oil'],       desc: 'Late-game industrial resource' },
];

const CIVILIAN_LEGEND = [
  { name: 'Farmer', emoji: '\u{1F33E}', desc: 'Improves grassland for grain/fruit/cotton' },
  { name: 'Miner', emoji: '\u26CF\uFE0F', desc: 'Improves mountain/hill tiles for coal/iron/gold/gems' },
  { name: 'Engineer', emoji: '\u{1F527}', desc: 'Builds railroads and infrastructure' },
  { name: 'Forester', emoji: '\u{1FAA3}', desc: 'Improves forest tiles for timber' },
  { name: 'Rancher', emoji: '\u{1F920}', desc: 'Improves grassland for wool/livestock/horses' },
  { name: 'Driller', emoji: '\u{1F6E2}\uFE0F', desc: 'Extracts oil from desert/swamp tiles' },
  { name: 'Prospector', emoji: '\u{1F50D}', desc: 'Reveals hidden resources on tiles' },
];

const INFRASTRUCTURE_LEGEND = [
  { name: 'Capital', symbol: '\u2605', color: '#ffd900', desc: 'Nation capital (gold star)' },
  { name: 'Province Capital', symbol: '\u25CF', color: '#fff', desc: 'Province center (white dot)' },
  { name: 'Railroad', symbol: '\u2550', color: '#8B4513', desc: 'Transport network for resources' },
  { name: 'Depot', symbol: '\u25A0', color: '#8B4513', desc: 'Railroad junction point' },
  { name: 'Port', symbol: '\u2693', color: '#4a88b8', desc: 'Enables naval trade and transport' },
  { name: 'Fort', symbol: '\u{1F3F0}', color: '#7a7068', desc: 'Defensive fortification (L1-L3)' },
];

const UNIT_LEGEND = [
  { category: 'Infantry', units: ['Militia', 'Regulars', 'Grenadiers', 'Rifle Infantry', 'Guards', 'Sharpshooters', 'Modern Infantry', 'Machine Gunners', 'Rangers'] },
  { category: 'Cavalry', units: ['Cuirassiers', 'Scouts', 'Carbine Cavalry', 'Armour', 'Mechanised'] },
  { category: 'Artillery', units: ['Light Artillery', 'Standard Artillery', 'Field Artillery', 'Siege Artillery', 'Railroad Gun', 'Mobile Artillery'] },
  { category: 'Special', units: ['Sapper', 'General'] },
];

const DIPLO_LEGEND = [
  { color: '#ffd900', label: 'Self (your nation)' },
  { color: '#2ecc40', label: 'Alliance' },
  { color: '#7fdbff', label: 'Non-Aggression Pact' },
  { color: '#ff4136', label: 'At War' },
  { color: '#aaaaaa', label: 'Neutral' },
];

export default function LegendScreen({ nations = [], onClose }: Props) {
  const flagNations = nations.filter(n => n.flag_svg);
  const greatPowers = flagNations.filter(n => n.nation_type === 'GreatPower');
  const minorNations = flagNations.filter(n => n.nation_type !== 'GreatPower');
  return (
    <div style={styles.overlay}>
      <div style={styles.container}>
        <div style={styles.header}>
          <h2 style={styles.title}>Legend</h2>
          <button onClick={onClose} style={styles.closeBtn}>Esc</button>
        </div>

        <div style={styles.body}>
          {/* Terrain */}
          <div style={styles.section}>
            <h3 style={styles.sectionTitle}>Terrain</h3>
            <div style={styles.grid}>
              {TERRAIN_LEGEND.map(t => (
                <div key={t.name} style={styles.legendItem}>
                  <span style={{ ...styles.swatch, background: t.color }} />
                  <div>
                    <div style={styles.itemName}>{t.name}</div>
                    <div style={styles.itemDesc}>{t.desc}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Resources */}
          <div style={styles.section}>
            <h3 style={styles.sectionTitle}>Resources</h3>
            <div style={styles.grid}>
              {RESOURCE_LEGEND.map(r => (
                <div key={r.name} style={styles.legendItem}>
                  <span style={styles.emoji}>{r.emoji}</span>
                  <div>
                    <div style={styles.itemName}>{r.name}</div>
                    <div style={styles.itemDesc}>{r.desc}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Civilians */}
          <div style={styles.section}>
            <h3 style={styles.sectionTitle}>Civilians</h3>
            <div style={styles.grid}>
              {CIVILIAN_LEGEND.map(c => (
                <div key={c.name} style={styles.legendItem}>
                  <span style={styles.emoji}>{c.emoji}</span>
                  <div>
                    <div style={styles.itemName}>{c.name}</div>
                    <div style={styles.itemDesc}>{c.desc}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Infrastructure */}
          <div style={styles.section}>
            <h3 style={styles.sectionTitle}>Infrastructure</h3>
            <div style={styles.grid}>
              {INFRASTRUCTURE_LEGEND.map(inf => (
                <div key={inf.name} style={styles.legendItem}>
                  <span style={{ ...styles.emoji, color: inf.color }}>{inf.symbol}</span>
                  <div>
                    <div style={styles.itemName}>{inf.name}</div>
                    <div style={styles.itemDesc}>{inf.desc}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Military Units */}
          <div style={styles.section}>
            <h3 style={styles.sectionTitle}>Military Units</h3>
            {UNIT_LEGEND.map(cat => (
              <div key={cat.category} style={{ marginBottom: 12 }}>
                <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#daa520', textTransform: 'uppercase' as const, letterSpacing: 1, marginBottom: 4 }}>
                  {cat.category}
                </div>
                <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#ccc', lineHeight: 1.6 }}>
                  {cat.units.join(' \u2022 ')}
                </div>
              </div>
            ))}
          </div>

          {/* Diplomatic Colors */}
          <div style={styles.section}>
            <h3 style={styles.sectionTitle}>Diplomatic Map Mode</h3>
            <div style={styles.grid}>
              {DIPLO_LEGEND.map(d => (
                <div key={d.label} style={styles.legendItem}>
                  <span style={{ ...styles.swatch, background: d.color }} />
                  <span style={styles.itemName}>{d.label}</span>
                </div>
              ))}
            </div>
          </div>

          {/* Strength Gradients */}
          <div style={styles.section}>
            <h3 style={styles.sectionTitle}>Strength Map Modes</h3>
            <div style={{ marginBottom: 12 }}>
              <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#999', marginBottom: 4 }}>Military / Naval Strength (relative to average)</div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ fontSize: 'var(--ui-font-size, 14px)' }}>Weak</span>
                <div style={{ flex: 1, height: 16, background: 'linear-gradient(to right, rgb(220,40,40), rgb(200,200,40) 50%, rgb(40,200,40))', borderRadius: 3 }} />
                <span style={{ fontSize: 'var(--ui-font-size, 14px)' }}>Strong</span>
              </div>
            </div>
            <div>
              <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#999', marginBottom: 4 }}>Relationship Score</div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ fontSize: 'var(--ui-font-size, 14px)' }}>-100</span>
                <div style={{ flex: 1, height: 16, background: 'linear-gradient(to right, rgb(220,40,40), rgb(160,160,160) 50%, rgb(40,200,40))', borderRadius: 3 }} />
                <span style={{ fontSize: 'var(--ui-font-size, 14px)' }}>+100</span>
              </div>
            </div>
          </div>

          {/* Nations & Flags */}
          {flagNations.length > 0 && (
            <div style={{ ...styles.section, borderBottom: 'none', marginBottom: 0 }}>
              <h3 style={styles.sectionTitle}>Nations</h3>
              {greatPowers.length > 0 && (
                <>
                  <div style={styles.flagGroupTitle}>Great Powers</div>
                  <div style={styles.flagGrid}>
                    {greatPowers.map(n => (
                      <div key={n.id} style={styles.flagCard}>
                        <Flag
                          svg={n.flag_svg || ''}
                          width={150}
                          height={100}
                          title={n.government_title || n.name}
                        />
                        <div style={styles.flagCaption}>
                          <span style={{ ...styles.flagDot, background: NATION_COLORS[n.color] || '#888' }} />
                          <div style={{ minWidth: 0 }}>
                            <div style={styles.flagName}>{n.name}</div>
                            {n.government_title && n.government_title !== n.name && (
                              <div style={styles.flagTitle}>{n.government_title}</div>
                            )}
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                </>
              )}
              {minorNations.length > 0 && (
                <>
                  <div style={{ ...styles.flagGroupTitle, marginTop: 16 }}>Minor Nations</div>
                  <div style={styles.flagGrid}>
                    {minorNations.map(n => (
                      <div key={n.id} style={styles.flagCard}>
                        <Flag
                          svg={n.flag_svg || ''}
                          width={120}
                          height={80}
                          title={n.government_title || n.name}
                        />
                        <div style={styles.flagCaption}>
                          <span style={{ ...styles.flagDot, background: NATION_COLORS[n.color] || '#888' }} />
                          <div style={{ minWidth: 0 }}>
                            <div style={styles.flagName}>{n.name}</div>
                            {n.government_title && n.government_title !== n.name && (
                              <div style={styles.flagTitle}>{n.government_title}</div>
                            )}
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  overlay: {
    flex: 1, minHeight: 0,
    background: '#1a1a2e', color: '#e0d8c0',
    display: 'flex', flexDirection: 'column',
    fontFamily: "'Georgia', serif",
  },
  container: {
    display: 'flex', flexDirection: 'column', height: '100%',
  },
  header: {
    display: 'flex', alignItems: 'center', justifyContent: 'space-between',
    padding: '12px 24px', borderBottom: '2px solid #3a3520',
    background: '#0f0f23',
  },
  title: { color: '#daa520', margin: 0, fontSize: 22 },
  closeBtn: {
    padding: '4px 12px', background: '#3a3520', color: '#e0d8c0',
    border: '1px solid #5a5030', cursor: 'pointer', fontFamily: "'Georgia', serif",
  },
  body: {
    flex: 1, overflowY: 'auto' as const, padding: '16px 32px',
  },
  section: {
    marginBottom: 28, paddingBottom: 16, borderBottom: '1px solid #3a3520',
  },
  sectionTitle: {
    color: '#daa520', margin: '0 0 12px', fontSize: 18,
    borderBottom: '1px solid #3a3520', paddingBottom: 6,
  },
  grid: {
    display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
    gap: 8,
  },
  legendItem: {
    display: 'flex', alignItems: 'center', gap: 10, padding: '4px 0',
  },
  swatch: {
    display: 'inline-block', width: 24, height: 24, borderRadius: 3,
    border: '1px solid rgba(255,255,255,0.15)', flexShrink: 0,
  },
  emoji: {
    fontSize: 20, width: 28, textAlign: 'center' as const, flexShrink: 0,
  },
  itemName: { fontSize: 'var(--ui-font-size, 14px)', fontWeight: 'bold' as const },
  itemDesc: { fontSize: 'var(--ui-font-size, 14px)', color: '#999' },
  flagGroupTitle: {
    fontSize: 'var(--ui-font-size, 14px)', color: '#daa520', textTransform: 'uppercase' as const,
    letterSpacing: 1, marginBottom: 10,
  },
  flagGrid: {
    display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))',
    gap: 16,
  },
  flagCard: {
    display: 'flex', flexDirection: 'column' as const, alignItems: 'flex-start',
    gap: 8, padding: 10, background: '#1a1a2e',
    border: '1px solid #3a3520', borderRadius: 4,
  },
  flagCaption: {
    display: 'flex', alignItems: 'center', gap: 8, width: '100%', minWidth: 0,
  },
  flagDot: {
    width: 12, height: 12, borderRadius: '50%',
    border: '1px solid rgba(255,255,255,0.2)', flexShrink: 0,
  },
  flagName: {
    fontSize: 'var(--ui-font-size, 14px)', fontWeight: 'bold' as const, color: '#e0d8c0',
    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' as const,
  },
  flagTitle: {
    fontSize: 11, color: '#9a9a9a', marginTop: 1,
    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' as const,
  },
};
