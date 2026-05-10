import type { ShipsData, ShipDetail, NavyMarker } from '../wasm';

// Per-ship-type emoji, mirroring UnitPanel's CATEGORY_ICONS pattern. Era buckets
// roughly match the ship roster in `crates/domain/src/military/ships.rs`.
const SHIP_ICONS: Record<string, string> = {
  // Merchant
  Trader: '\u{1F6F6}',         // 🛶
  Indiaman: '⛵',          // ⛵
  Clipper: '⛵',           // ⛵
  Paddlewheeler: '\u{1F6E5}️', // 🛥️
  Freighter: '\u{1F6A2}',      // 🚢
  // Warship
  Frigate: '\u{1F3F4}‍☠️', // 🏴‍☠️
  ShipOfTheLine: '⛵',     // ⛵
  Raider: '\u{1F3F4}‍☠️', // 🏴‍☠️
  Ironclad: '\u{1F6E5}️', // 🛥️
  AdvancedIronclad: '\u{1F6E5}️', // 🛥️
  ArmouredCruiser: '\u{1F6A2}', // 🚢
  Dreadnought: '\u{1F6A2}',    // 🚢
  Battlecruiser: '\u{1F6A2}',  // 🚢
};
const SHIP_ICON_FALLBACK = '⚓'; // ⚓

interface Props {
  ships: ShipsData;
  // When a navy fleet marker is selected, restrict the panel to that fleet's
  // warships (so the sidebar mirrors the in-game selection, like armies do).
  selectedNavyMarker?: NavyMarker | null;
  selectedShipIds?: number[];
  /** Destination zone for the queued fleet move from this fleet, if any.
   *  Renders a "→ <name>" banner with a Cancel button. */
  pendingMoveDestZone?: { id: number; name: string } | null;
  onCancelPendingMove?: () => void;
  onToggleShip?: (shipId: number) => void;
  onSelectAll?: () => void;
}

export default function NavalPanel({
  ships,
  selectedNavyMarker,
  selectedShipIds = [],
  pendingMoveDestZone,
  onCancelPendingMove,
  onToggleShip,
  onSelectAll,
}: Props) {
  const { warships, total_naval_fp } = ships;

  // Restrict to ships in the selected fleet's sea zone, when a marker is selected.
  let displayed: ShipDetail[] = warships;
  let scopeLabel = '';
  if (selectedNavyMarker && selectedNavyMarker.kind === 'fleet') {
    const zoneId = selectedNavyMarker.sea_zone_id;
    if (zoneId != null) {
      displayed = warships.filter(s => s.sea_zone === zoneId);
    }
    scopeLabel = selectedNavyMarker.sea_zone_name
      ? ` — ${selectedNavyMarker.sea_zone_name}`
      : '';
  }

  const interactive = !!onToggleShip;
  const allSelected = displayed.length > 0 && displayed.every(s => selectedShipIds.includes(s.id));
  const hasSelection = selectedShipIds.length > 0;

  return (
    <div style={{ fontSize: 13 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
        <span style={{ fontWeight: 'bold', color: '#ccc' }}>Warships{scopeLabel}</span>
        {interactive && displayed.length > 1 && (
          <button onClick={onSelectAll} style={btnStyle('#456')}>
            {allSelected ? 'Deselect' : 'Select All'}
          </button>
        )}
      </div>

      <div style={{ fontSize: 11, color: '#888', marginBottom: 4 }}>
        {displayed.length} ship{displayed.length === 1 ? '' : 's'} {selectedNavyMarker ? '' : `· ${total_naval_fp} FP`}
      </div>

      {pendingMoveDestZone && (
        <div style={{
          marginBottom: 4, padding: '3px 6px',
          background: 'rgba(218,165,32,0.12)', border: '1px solid rgba(218,165,32,0.4)',
          borderRadius: 3, fontSize: 11,
          display: 'flex', justifyContent: 'space-between', alignItems: 'center',
        }}>
          <span style={{ color: '#ffd700' }}>
            {'→'} {pendingMoveDestZone.name}
            <span style={{ color: '#888', marginLeft: 6 }}>(end of turn)</span>
          </span>
          {onCancelPendingMove && (
            <button onClick={onCancelPendingMove} style={btnStyle('#a33')}>
              Cancel
            </button>
          )}
        </div>
      )}

      {hasSelection && interactive && !pendingMoveDestZone && (
        <div style={{ fontSize: 10, color: '#aaa', marginBottom: 4, fontStyle: 'italic' }}>
          Click a highlighted sea hex to queue a move {'·'} click ships to toggle {'·'} Esc to cancel
        </div>
      )}

      {displayed.length > 0 ? displayed.map(s => {
        const isSelected = selectedShipIds.includes(s.id);
        const icon = SHIP_ICONS[s.type] ?? SHIP_ICON_FALLBACK;
        const hpPct = s.hull_max > 0 ? Math.max(0, Math.min(100, (s.hull / s.hull_max) * 100)) : 0;
        return (
          <div
            key={s.id}
            style={{
              padding: '4px 6px',
              background: isSelected ? 'rgba(218,165,32,0.15)' : 'rgba(255,255,255,0.05)',
              borderRadius: 4,
              marginBottom: 3,
              borderWidth: 1,
              borderStyle: 'solid',
              borderColor: isSelected ? 'rgba(218,165,32,0.4)' : 'transparent',
              cursor: interactive ? 'pointer' : 'default',
            }}
            onClick={interactive ? () => onToggleShip!(s.id) : undefined}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <span>
                {interactive && (
                  <span style={{
                    display: 'inline-block', width: 12, height: 12,
                    border: '1px solid #888', borderRadius: 2, marginRight: 4,
                    background: isSelected ? '#daa520' : 'transparent',
                    verticalAlign: 'middle', fontSize: 9, textAlign: 'center', lineHeight: '12px',
                  }}>
                    {isSelected ? '✓' : ''}
                  </span>
                )}
                {icon} {formatShipType(s.type)}
              </span>
              <span style={{ fontSize: 11, color: '#999' }}>
                FP {s.firepower}
              </span>
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 2 }}>
              <HullBar pct={hpPct} />
              <span style={{ fontSize: 10, color: '#888' }}>{s.hull}/{s.hull_max}</span>
            </div>
          </div>
        );
      }) : (
        <div style={{ color: '#666', fontStyle: 'italic', fontSize: 11 }}>No warships</div>
      )}
    </div>
  );
}

function HullBar({ pct }: { pct: number }) {
  const color = pct > 66 ? '#3a7' : pct > 33 ? '#daa520' : '#a33';
  return (
    <div style={{ flex: 1, height: 5, background: 'rgba(0,0,0,0.4)', borderRadius: 2, overflow: 'hidden' }}>
      <div style={{ width: `${pct}%`, height: '100%', background: color }} />
    </div>
  );
}

function formatShipType(t: string): string {
  return t.replace(/([A-Z])/g, ' $1').trim();
}

function btnStyle(bg: string): React.CSSProperties {
  return {
    background: bg, color: '#fff', border: 'none', borderRadius: 3,
    padding: '1px 6px', fontSize: 10, cursor: 'pointer',
  };
}
