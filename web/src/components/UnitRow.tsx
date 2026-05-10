import React from 'react';

const CATEGORY_ICONS: Record<string, string> = {
  Infantry: '⚔️',   // ⚔️
  Cavalry: '\u{1F40E}',       // 🐎
  Artillery: '\u{1F4A3}',     // 💣
  Special: '⭐',           // ⭐
  Garrison: '\u{1F6E1}️', // 🛡️
};

const UNIT_TYPE_CATEGORY: Record<string, string> = {
  // Garrison
  Minutemen: 'Garrison',
  Militia: 'Garrison',
  Conscript: 'Garrison',
  GarrisonArtillery: 'Garrison',
  // Skirmisher
  Skirmishers: 'Infantry',
  Sharpshooters: 'Infantry',
  Rangers: 'Infantry',
  // Line infantry
  Regulars: 'Infantry',
  RifleInfantry: 'Infantry',
  Infantry: 'Infantry',
  // Elite infantry
  Grenadiers: 'Infantry',
  Guards: 'Infantry',
  MachineGunners: 'Infantry',
  // Light cavalry
  Hussars: 'Cavalry',
  Carbineers: 'Cavalry',
  Mechanised: 'Cavalry',
  // Heavy cavalry
  Cuirassiers: 'Cavalry',
  Armour: 'Cavalry',
  // Light artillery
  LightArtillery: 'Artillery',
  FieldArtillery: 'Artillery',
  MobileArtillery: 'Artillery',
  // Heavy artillery
  Artillery: 'Artillery',
  SiegeArtillery: 'Artillery',
  RailroadGuns: 'Artillery',
  // Engineer
  Sapper: 'Special',
  CombatEngineer: 'Special',
  Saboteur: 'Special',
  // Special
  General: 'Special',
};

function iconForUnitType(unit_type: string): string {
  const category = UNIT_TYPE_CATEGORY[unit_type] || 'Infantry';
  return CATEGORY_ICONS[category] || '';
}

function splitCamel(s: string): string {
  return s.replace(/([A-Z])/g, ' $1').trim();
}

export function HealthBar({ health }: { health: number }) {
  const color = health > 60 ? '#2a2' : health > 30 ? '#ca2' : '#a22';
  return (
    <div style={{ width: 60, height: 5, background: 'rgba(255,255,255,0.1)', borderRadius: 2, overflow: 'hidden' }}>
      <div style={{ width: `${health}%`, height: '100%', background: color }} />
    </div>
  );
}

export interface UnitRowProps {
  unit_type: string;
  medals: number;
  health: number;
  effective_firepower: number;
  destroyed?: boolean;
  /**
   * Hide / show the FP value next to the unit name. Default `true` so the
   * main-map sidebar is unchanged. The battle screen passes `false` when
   * the user has the firepower toggle off, and `true` (with
   * `initialFirepower` populated) when the debug toggle is on.
   */
  showFirepower?: boolean;
  /**
   * When supplied, renders an "FP {init} → {final}" pair instead of just
   * the current effective_firepower. Used by the battle screen's debug
   * mode to show how a unit's contribution changed over the battle.
   */
  initialFirepower?: number;
  /** Extra suffix rendered under the FP line (defender bonus breakdown). */
  fpSuffix?: React.ReactNode;
  style?: React.CSSProperties;
}

/**
 * Visual-only unit row used on the main-map sidebar and the battle screen.
 * Renders icon + name + medals + firepower on top, HP bar below.
 * When `destroyed` is true, renders dimmed with strikethrough and no HP bar.
 */
export function UnitRow({
  unit_type,
  medals,
  health,
  effective_firepower,
  destroyed,
  showFirepower = true,
  initialFirepower,
  fpSuffix,
  style,
}: UnitRowProps) {
  const icon = iconForUnitType(unit_type);
  const stars = '★'.repeat(medals);
  const name = splitCamel(unit_type);
  const fpEl = showFirepower ? (
    initialFirepower !== undefined ? (
      <span style={{ fontSize: 11, color: '#999' }}>
        FP {initialFirepower.toFixed(1)} <span style={{ color: '#666' }}>→</span> {effective_firepower.toFixed(1)}
      </span>
    ) : (
      <span style={{ fontSize: 11, color: '#999' }}>
        FP {effective_firepower.toFixed(1)}
      </span>
    )
  ) : null;
  return (
    <div style={{
      background: 'rgba(255,255,255,0.05)',
      borderRadius: 4,
      padding: '4px 6px',
      marginBottom: 3,
      opacity: destroyed ? 0.45 : 1,
      ...style,
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <span style={{ textDecoration: destroyed ? 'line-through' : 'none' }}>
          {icon} {name}
          {!destroyed && stars && <span style={{ color: '#ffd700', marginLeft: 4 }}>{stars}</span>}
        </span>
        {!destroyed && fpEl}
        {destroyed && showFirepower && initialFirepower !== undefined && (
          <span style={{ fontSize: 11, color: '#a66' }}>
            FP {initialFirepower.toFixed(1)} <span style={{ color: '#653' }}>→</span> 0
          </span>
        )}
        {destroyed && !(showFirepower && initialFirepower !== undefined) && (
          <span style={{ fontSize: 10, color: '#a66' }}>Destroyed</span>
        )}
      </div>
      {!destroyed && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 2 }}>
          <HealthBar health={health} />
          <span style={{ fontSize: 10, color: '#888' }}>{health}%</span>
        </div>
      )}
      {fpSuffix && <div style={{ marginTop: 3 }}>{fpSuffix}</div>}
    </div>
  );
}
