import type { ShipsData } from '../wasm';

interface Props {
  ships: ShipsData;
}

export default function NavalPanel({ ships }: Props) {
  const { merchants, warships, total_cargo, total_naval_fp } = ships;

  return (
    <div style={{ fontSize: 13 }}>
      <div style={{ fontWeight: 'bold', marginBottom: 6, color: '#ccc' }}>Naval Forces</div>

      {/* Merchant fleet */}
      <div style={{ marginBottom: 8 }}>
        <div style={{ fontSize: 11, color: '#888', marginBottom: 3 }}>
          Merchants ({merchants.length} ships, {total_cargo} cargo)
        </div>
        {merchants.length > 0 ? merchants.map(s => (
          <div key={s.id} style={{
            display: 'flex', justifyContent: 'space-between',
            padding: '2px 4px', background: 'rgba(255,255,255,0.05)', borderRadius: 3, marginBottom: 2,
            fontSize: 12,
          }}>
            <span>\u2693 {formatShipType(s.type)}</span>
            <span style={{ color: '#888' }}>
              Hull {s.hull}/{s.hull_max} | Cargo {s.cargo}
            </span>
          </div>
        )) : (
          <div style={{ color: '#666', fontStyle: 'italic', fontSize: 11 }}>No merchant ships</div>
        )}
      </div>

      {/* Warship fleet */}
      <div style={{ marginBottom: 8 }}>
        <div style={{ fontSize: 11, color: '#888', marginBottom: 3 }}>
          Warships ({warships.length} ships, {total_naval_fp} FP)
        </div>
        {warships.length > 0 ? warships.map(s => (
          <div key={s.id} style={{
            display: 'flex', justifyContent: 'space-between',
            padding: '2px 4px', background: 'rgba(255,255,255,0.05)', borderRadius: 3, marginBottom: 2,
            fontSize: 12,
          }}>
            <span>\u2693 {formatShipType(s.type)}</span>
            <span style={{ color: '#888' }}>
              Hull {s.hull}/{s.hull_max} | FP {s.firepower}
            </span>
          </div>
        )) : (
          <div style={{ color: '#666', fontStyle: 'italic', fontSize: 11 }}>No warships</div>
        )}
      </div>

    </div>
  );
}

function formatShipType(t: string): string {
  return t.replace(/([A-Z])/g, ' $1').trim();
}
