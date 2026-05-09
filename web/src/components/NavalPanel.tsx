import type { ShipsData, ShipDetail, NavyMarker } from '../wasm';

interface Props {
  ships: ShipsData;
  // When a navy fleet marker is selected, restrict the panel to that fleet's
  // warships (so the sidebar mirrors the in-game selection, like armies do).
  selectedNavyMarker?: NavyMarker | null;
  selectedShipIds?: number[];
  onToggleShip?: (shipId: number) => void;
  onSelectAll?: () => void;
}

export default function NavalPanel({
  ships,
  selectedNavyMarker,
  selectedShipIds = [],
  onToggleShip,
  onSelectAll,
}: Props) {
  const { warships, total_naval_fp } = ships;

  // Restrict to ships in the selected fleet's sea zone, when a marker is selected.
  let displayed: ShipDetail[] = warships;
  let scopeLabel = '';
  if (selectedNavyMarker && selectedNavyMarker.kind === 'fleet') {
    const zoneId = selectedNavyMarker.sea_zone_id;
    displayed = warships.filter(s => s.sea_zone === (zoneId ?? null));
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

      {hasSelection && interactive && (
        <div style={{ fontSize: 10, color: '#aaa', marginBottom: 4, fontStyle: 'italic' }}>
          Click an adjacent sea hex to move {selectedShipIds.length} warship{selectedShipIds.length > 1 ? 's' : ''} {'·'} click ships to toggle {'·'} Esc to cancel
        </div>
      )}

      {displayed.length > 0 ? displayed.map(s => {
        const isSelected = selectedShipIds.includes(s.id);
        return (
          <div
            key={s.id}
            style={{
              display: 'flex', justifyContent: 'space-between', alignItems: 'center',
              padding: '3px 5px',
              background: isSelected ? 'rgba(218,165,32,0.15)' : 'rgba(255,255,255,0.05)',
              border: isSelected ? '1px solid rgba(218,165,32,0.4)' : '1px solid transparent',
              borderRadius: 3, marginBottom: 2, fontSize: 12,
              cursor: interactive ? 'pointer' : 'default',
            }}
            onClick={interactive ? () => onToggleShip!(s.id) : undefined}
          >
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
              {'⚓'} {formatShipType(s.type)}
            </span>
            <span style={{ color: '#888' }}>
              Hull {s.hull}/{s.hull_max} | FP {s.firepower}
            </span>
          </div>
        );
      }) : (
        <div style={{ color: '#666', fontStyle: 'italic', fontSize: 11 }}>No warships</div>
      )}
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
